pub(crate) mod events;
pub(crate) mod keybindings;
pub(crate) mod render;
pub(crate) mod syntax;

use std::io::{self, Stdout};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::api::{GeminiClient, GeminiResponse};
use crate::app::App;
use crate::config::StoredConfig;

type Tui = Terminal<CrosstermBackend<Stdout>>;

pub static RUNNING_PROCESS_PID: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);
pub static RUNNING_PROCESS_STDIN: std::sync::Mutex<Option<std::process::ChildStdin>> =
    std::sync::Mutex::new(None);
type AskUserChannel = (std::sync::mpsc::Sender<String>, String, Vec<String>);

pub static ASK_USER_CHANNEL: std::sync::Mutex<Option<AskUserChannel>> = std::sync::Mutex::new(None);

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

struct BackgroundProcess {
    command: String,
    child: Arc<Mutex<std::process::Child>>,
    stdout_accumulator: Arc<Mutex<String>>,
    stderr_accumulator: Arc<Mutex<String>>,
    exit_status: Arc<Mutex<Option<i32>>>,
}

static BACKGROUND_PROCESSES: OnceLock<Mutex<HashMap<u32, BackgroundProcess>>> = OnceLock::new();

struct PersistentSession {
    stdin: std::process::ChildStdin,
    stdout_accumulator: Arc<Mutex<String>>,
    stderr_accumulator: Arc<Mutex<String>>,
}

static PERSISTENT_SESSIONS: OnceLock<Mutex<HashMap<String, PersistentSession>>> = OnceLock::new();

fn register_background_process(
    pid: u32,
    command: String,
    child: std::process::Child,
    stdout_acc: Arc<Mutex<String>>,
    stderr_acc: Arc<Mutex<String>>,
) {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    let child_arc = Arc::new(Mutex::new(child));
    let exit_status = Arc::new(Mutex::new(None));

    let child_clone = child_arc.clone();
    let exit_status_clone = exit_status.clone();
    
    // Spawn monitor thread to wait for process exit
    std::thread::spawn(move || {
        let mut child_guard = child_clone.lock().unwrap();
        if let Ok(status) = child_guard.wait() {
            let mut status_guard = exit_status_clone.lock().unwrap();
            *status_guard = status.code();
        }
    });

    if let Ok(mut map) = registry.lock() {
        map.insert(
            pid,
            BackgroundProcess {
                command,
                child: child_arc,
                stdout_accumulator: stdout_acc,
                stderr_accumulator: stderr_acc,
                exit_status,
            },
        );
    }
}

fn run_check_process(pid: u32) -> serde_json::Value {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = registry.lock() {
        if let Some(proc) = map.get_mut(&pid) {
            let mut exit_code_guard = proc.exit_status.lock().unwrap();
            if exit_code_guard.is_none() {
                if let Ok(mut child_guard) = proc.child.lock() {
                    match child_guard.try_wait() {
                        Ok(Some(status)) => {
                            *exit_code_guard = status.code();
                        }
                        _ => {}
                    }
                }
            }
            let is_alive = exit_code_guard.is_none();
            serde_json::json!({
                "alive": is_alive,
                "exit_code": *exit_code_guard
            })
        } else {
            serde_json::json!({ "error": format!("No background process found with PID {}", pid) })
        }
    } else {
        serde_json::json!({ "error": "Failed to lock background process registry" })
    }
}

fn run_kill_process(pid: u32) -> serde_json::Value {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = registry.lock() {
        if let Some(proc) = map.remove(&pid) {
            let _ = proc.child.lock().map(|mut c| c.kill());
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(&["-9", &format!("-{}", pid)])
                    .status();
            }
            serde_json::json!({ "success": true })
        } else {
            serde_json::json!({ "error": format!("No background process found with PID {}", pid) })
        }
    } else {
        serde_json::json!({ "error": "Failed to lock background process registry" })
    }
}

fn run_get_logs(pid: u32, limit: Option<usize>) -> serde_json::Value {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = registry.lock() {
        if let Some(proc) = map.get(&pid) {
            let stdout = proc.stdout_accumulator.lock().unwrap().clone();
            let stderr = proc.stderr_accumulator.lock().unwrap().clone();
            
            let stdout_lines = if let Some(lim) = limit {
                let lines: Vec<&str> = stdout.lines().collect();
                let start = lines.len().saturating_sub(lim);
                lines[start..].join("\n")
            } else {
                stdout
            };

            let stderr_lines = if let Some(lim) = limit {
                let lines: Vec<&str> = stderr.lines().collect();
                let start = lines.len().saturating_sub(lim);
                lines[start..].join("\n")
            } else {
                stderr
            };

            let mut exit_code_guard = proc.exit_status.lock().unwrap();
            if exit_code_guard.is_none() {
                if let Ok(mut child_guard) = proc.child.lock() {
                    if let Ok(Some(status)) = child_guard.try_wait() {
                        *exit_code_guard = status.code();
                    }
                }
            }

            serde_json::json!({
                "stdout": stdout_lines,
                "stderr": stderr_lines,
                "exit_code": *exit_code_guard
            })
        } else {
            serde_json::json!({ "error": format!("No background process found with PID {}", pid) })
        }
    } else {
        serde_json::json!({ "error": "Failed to lock background process registry" })
    }
}

fn run_persistent_bash(
    session_id: &str,
    cmd: &str,
    input: Option<&str>,
    sender: Sender<WorkerEvent>,
) -> Result<serde_json::Value, std::io::Error> {
    let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock().unwrap();
    
    let entry = map.entry(session_id.to_owned()).or_insert_with(|| {
        let mut command = std::process::Command::new("bash");
        command
            .arg("--noprofile")
            .arg("--norc")
            .arg("-i")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    unsafe extern "C" {
                        fn setpgid(pid: i32, pgid: i32) -> i32;
                    }
                    setpgid(0, 0);
                    Ok(())
                });
            }
        }

        let mut child = command.spawn().expect("Failed to spawn persistent bash");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let stdout_acc = Arc::new(Mutex::new(String::new()));
        let stderr_acc = Arc::new(Mutex::new(String::new()));

        let sender_stdout = sender.clone();
        let stdout_acc_clone = stdout_acc.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buffer = [0; 1024];
            let mut reader = stdout;
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buffer[..n]).into_owned();
                if let Ok(mut guard) = stdout_acc_clone.lock() {
                    guard.push_str(&chunk);
                }
                let _ = sender_stdout.send(WorkerEvent::BashStdout(chunk));
            }
        });

        let sender_stderr = sender.clone();
        let stderr_acc_clone = stderr_acc.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buffer = [0; 1024];
            let mut reader = stderr;
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buffer[..n]).into_owned();
                if let Ok(mut guard) = stderr_acc_clone.lock() {
                    guard.push_str(&chunk);
                }
                let _ = sender_stderr.send(WorkerEvent::BashStderr(chunk));
            }
        });

        PersistentSession {
            stdin,
            stdout_accumulator: stdout_acc,
            stderr_accumulator: stderr_acc,
        }
    });

    use std::io::Write;
    
    let start_stdout_len = entry.stdout_accumulator.lock().unwrap().len();
    let start_stderr_len = entry.stderr_accumulator.lock().unwrap().len();

    let nonce = format!("CMD_DONE_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis());
    let sentinel = format!("___SENTINEL_{}___", nonce);

    writeln!(entry.stdin, "{}", cmd)?;
    if let Some(inp) = input {
        writeln!(entry.stdin, "{}", inp)?;
    }
    writeln!(entry.stdin, "echo \"{}\"", sentinel)?;
    let _ = entry.stdin.flush();

    let mut check_count = 0;
    let max_checks = 100;
    let mut found = false;

    while check_count < max_checks {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let stdout_guard = entry.stdout_accumulator.lock().unwrap();
        if stdout_guard[start_stdout_len..].contains(&sentinel) {
            found = true;
            break;
        }
        check_count += 1;
    }

    let raw_stdout = entry.stdout_accumulator.lock().unwrap();
    let raw_stderr = entry.stderr_accumulator.lock().unwrap();

    let mut stdout_diff = raw_stdout[start_stdout_len..].to_owned();
    let stderr_diff = raw_stderr[start_stderr_len..].to_owned();

    if let Some(idx) = stdout_diff.find(&sentinel) {
        stdout_diff.truncate(idx);
    }
    let clean_stdout = stdout_diff.trim_end().to_owned();

    Ok(serde_json::json!({
        "status": if found { serde_json::json!(0) } else { serde_json::Value::Null },
        "stdout": clean_stdout,
        "stderr": stderr_diff,
        "error": if found { serde_json::Value::Null } else { serde_json::json!("Command timed out / is still running") }
    }))
}

pub(crate) enum WorkerEvent {
    StreamChunk(usize, GeminiResponse),
    StreamDone(usize),
    StreamError(usize, String),
    Models(Result<Vec<String>, String>),
    ToolResult(String, serde_json::Value),
    ResetStream(usize),
    BashStdout(String),
    BashStderr(String),
}

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = start_terminal()?;
    let (sender, receiver) = mpsc::channel();
    let result = run_loop(&mut terminal, &mut app, &sender, &receiver);
    stop_terminal(&mut terminal)?;

    let session_id = app.chat.session_id.clone();
    println!("\nTo resume this session, run:");
    println!("  darwincode --session {}", session_id);
    println!("or continue the last session with:");
    println!("  darwincode --continue\n");

    result
}

fn start_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableFocusChange,
        event::EnableBracketedPaste
    )?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(Into::into)
}

fn stop_terminal(terminal: &mut Tui) -> Result<()> {
    let _ = execute!(
        io::stdout(),
        event::DisableFocusChange,
        event::DisableBracketedPaste
    );
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_loop(
    terminal: &mut Tui,
    app: &mut App,
    sender: &Sender<WorkerEvent>,
    receiver: &Receiver<WorkerEvent>,
) -> Result<()> {
    while !app.should_quit {
        let ask_user_req = crate::tui::ASK_USER_CHANNEL
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|(_, q, opts)| (q.clone(), opts.clone())))
            .filter(|_| app.screen != crate::app::Screen::AskUser);

        if let Some((question, options)) = ask_user_req {
            app.screen = crate::app::Screen::AskUser;
            app.ask_user.question = question;
            app.ask_user.options = options;
            app.ask_user.selected_idx = 0;
            app.ask_user.custom_input.clear();
            app.ask_user.is_custom = app.ask_user.options.is_empty();
            app.status = "Answer the question. Enter to submit.".to_owned();
        }

        app.advance_tick();
        while let Ok(event) = receiver.try_recv() {
            handle_worker_event(app, event, sender);
        }

        if app.screen == crate::app::Screen::Chat
            && !app.is_busy()
            && !app.chat.message_queue.is_empty()
            && let Some(action) = app.pop_and_start_next_queue_item()
        {
            match action {
                crate::app::SubmitAction::Generate(request) => {
                    spawn_generation_worker(
                        request.config,
                        request.history,
                        request.cancel_token,
                        request.generation_id,
                        sender.clone(),
                    );
                }
                crate::app::SubmitAction::LoadModels(config) => {
                    spawn_models_worker(config, sender.clone());
                }
                crate::app::SubmitAction::ExecuteFunction { name, args, config } => {
                    handle_function_action(
                        crate::app::FunctionAction::Execute { name, args, config },
                        sender,
                    );
                }
            }
        }

        let _ = terminal.draw(|frame| render::render(frame, app));

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != event::KeyEventKind::Release {
                        events::handle_key(app, sender, key)?;
                    }
                }
                Event::Paste(text) => events::handle_paste(app, text),
                Event::Resize(_, _) => {
                    let _ = terminal.autoresize();
                    let _ = terminal.clear();
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_worker_event(app: &mut App, event: WorkerEvent, sender: &Sender<WorkerEvent>) {
    match event {
        WorkerEvent::StreamChunk(id, chunk) => {
            if id == app.generation_id {
                app.handle_stream_chunk(chunk);
            }
        }
        WorkerEvent::StreamDone(id) if id == app.generation_id => {
            if let Some(action) = app.complete_stream() {
                handle_function_action(action, sender);
            }
        }
        WorkerEvent::StreamDone(_) => {}
        WorkerEvent::StreamError(id, err) => {
            if id == app.generation_id {
                app.handle_stream_error(err);
            }
        }
        WorkerEvent::Models(result) => app.complete_load_models(result),
        WorkerEvent::ToolResult(name, response) => {
            if let Some(crate::app::FunctionAction::ResumeGeneration(request)) =
                app.complete_function_execution(name, response)
            {
                spawn_generation_worker(
                    request.config,
                    request.history,
                    request.cancel_token,
                    request.generation_id,
                    sender.clone(),
                );
            }
        }
        WorkerEvent::ResetStream(id) => {
            if id == app.generation_id {
                app.chat.streaming_parts.clear();
            }
        }
        WorkerEvent::BashStdout(chunk) => {
            app.handle_bash_stdout(chunk);
        }
        WorkerEvent::BashStderr(chunk) => {
            app.handle_bash_stderr(chunk);
        }
    }
}

fn load_gitignore_rules() -> Vec<String> {
    let mut rules = vec![
        ".git".to_owned(),
        "node_modules".to_owned(),
        "target".to_owned(),
        "dist".to_owned(),
        "build".to_owned(),
        ".next".to_owned(),
    ];
    if let Ok(content) = std::fs::read_to_string(".gitignore") {
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                let rule = trimmed
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .to_owned();
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
        }
    }
    rules
}

fn should_ignore(path: &std::path::Path, rules: &[String]) -> bool {
    for component in path.components() {
        if let Some(comp_str) = component.as_os_str().to_str() {
            for rule in rules {
                if comp_str == rule || (rule.starts_with('*') && comp_str.ends_with(&rule[1..])) {
                    return true;
                }
            }
        }
    }
    false
}

fn recursive_search(
    dir: &std::path::Path,
    pattern: &str,
    rules: &[String],
    matches: &mut Vec<String>,
) -> std::io::Result<()> {
    if dir.is_dir() {
        if should_ignore(dir, rules) {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if should_ignore(&path, rules) {
                continue;
            }
            if path.is_dir() {
                let _ = recursive_search(&path, pattern, rules, matches);
            } else if path.is_file()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(pattern) {
                        matches.push(format!("{}:{}:{}", path.display(), line_num + 1, line));
                        if matches.len() >= 1000 {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_function_action(
    action: crate::app::FunctionAction,
    sender: &Sender<WorkerEvent>,
) {
    match action {
        crate::app::FunctionAction::Execute { name, args, config } => {
            let sender = sender.clone();
            thread::spawn(move || {
                let result = match name.as_str() {
                    "read_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        if config.respect_gitignore
                            && should_ignore(std::path::Path::new(path), &rules)
                        {
                            serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore and respect_gitignore is enabled", path) })
                        } else {
                            match std::fs::read_to_string(path) {
                                Ok(content) => serde_json::json!({ "content": content }),
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        }
                    }
                    "read_files" => {
                        let paths = args.get("paths").and_then(|v| v.as_array());
                        if let Some(paths) = paths {
                            let rules = if config.respect_gitignore {
                                load_gitignore_rules()
                            } else {
                                Vec::new()
                            };
                            let mut results = serde_json::Map::new();
                            for path_val in paths {
                                if let Some(path) = path_val.as_str() {
                                    if config.respect_gitignore
                                        && should_ignore(std::path::Path::new(path), &rules)
                                    {
                                        results.insert(path.to_owned(), serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) }));
                                    } else {
                                        match std::fs::read_to_string(path) {
                                            Ok(content) => {
                                                results.insert(
                                                    path.to_owned(),
                                                    serde_json::json!({ "content": content }),
                                                );
                                            }
                                            Err(e) => {
                                                results.insert(
                                                    path.to_owned(),
                                                    serde_json::json!({ "error": e.to_string() }),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            serde_json::json!({ "files": results })
                        } else {
                            serde_json::json!({ "error": "Invalid arguments: paths array is required" })
                        }
                    }
                    "edit_files" => {
                        let edits = args.get("edits").and_then(|v| v.as_array());
                        if let Some(edits) = edits {
                            let rules = if config.respect_gitignore {
                                load_gitignore_rules()
                            } else {
                                Vec::new()
                            };
                            let mut parsed_edits = Vec::new();
                            let mut validation_errors = Vec::new();

                            for (idx, edit_val) in edits.iter().enumerate() {
                                let path =
                                    edit_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                                let old_string = edit_val
                                    .get("old_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let new_string = edit_val
                                    .get("new_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                if path.is_empty() {
                                    validation_errors
                                        .push(format!("Edit at index {} is missing 'path'", idx));
                                    continue;
                                }
                                if config.respect_gitignore
                                    && should_ignore(std::path::Path::new(path), &rules)
                                {
                                    validation_errors.push(format!(
                                        "Access denied: `{}` is ignored by .gitignore",
                                        path
                                    ));
                                    continue;
                                }
                                parsed_edits.push((
                                    path.to_owned(),
                                    old_string.to_owned(),
                                    new_string.to_owned(),
                                ));
                            }

                            if !validation_errors.is_empty() {
                                serde_json::json!({ "error": validation_errors.join("; ") })
                            } else {
                                let mut original_contents = std::collections::HashMap::new();
                                let mut apply_errors = Vec::new();

                                for (path, _, _) in &parsed_edits {
                                    if !original_contents.contains_key(path) {
                                        match std::fs::read_to_string(path) {
                                            Ok(content) => {
                                                original_contents.insert(path.clone(), content);
                                            }
                                            Err(e) => {
                                                apply_errors.push(format!(
                                                    "Failed to read `{}`: {}",
                                                    path, e
                                                ));
                                            }
                                        }
                                    }
                                }

                                if !apply_errors.is_empty() {
                                    serde_json::json!({ "error": apply_errors.join("; ") })
                                } else {
                                    let mut working_contents = original_contents.clone();
                                    let mut diffs = Vec::new();

                                    for (path, old_string, new_string) in &parsed_edits {
                                        let current_content =
                                            working_contents.get_mut(path).unwrap();
                                        if current_content.contains(old_string) {
                                            let mut diff = format!("--- {}\n+++ {}\n", path, path);
                                            let old_lines: Vec<&str> = old_string.lines().collect();
                                            let new_lines: Vec<&str> = new_string.lines().collect();

                                            for line in &old_lines {
                                                diff.push_str("- ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            for line in &new_lines {
                                                diff.push_str("+ ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            diffs.push(diff);

                                            *current_content =
                                                current_content.replacen(old_string, new_string, 1);
                                        } else {
                                            apply_errors.push(format!(
                                                "old_string not found in `{}`",
                                                path
                                            ));
                                            break;
                                        }
                                    }

                                    if !apply_errors.is_empty() {
                                        serde_json::json!({ "error": apply_errors.join("; ") })
                                    } else {
                                        let mut written_files = Vec::new();
                                        let mut write_error = None;

                                        for (path, new_content) in &working_contents {
                                            match std::fs::write(path, new_content) {
                                                Ok(_) => {
                                                    written_files.push(path.clone());
                                                }
                                                Err(e) => {
                                                    write_error = Some(format!(
                                                        "Failed to write `{}`: {}",
                                                        path, e
                                                    ));
                                                    break;
                                                }
                                            }
                                        }

                                        if let Some(err) = write_error {
                                            for path in written_files {
                                                let orig = original_contents.get(&path).unwrap();
                                                let _ = std::fs::write(&path, orig);
                                            }
                                            serde_json::json!({ "error": format!("Write failed, transaction rolled back. Error: {}", err) })
                                        } else {
                                            let mut combined_diff = String::new();
                                            combined_diff.push_str("```diff\n");
                                            combined_diff.push_str(&diffs.join("\n"));
                                            combined_diff.push_str("```");

                                            serde_json::json!({ "success": true, "diff": combined_diff })
                                        }
                                    }
                                }
                            }
                        } else {
                            serde_json::json!({ "error": "Invalid arguments: edits array is required" })
                        }
                    }
                    "list_directory" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        match std::fs::read_dir(path) {
                            Ok(entries) => {
                                let mut files = Vec::new();
                                for entry in entries.filter_map(Result::ok) {
                                    let entry_path = entry.path();
                                    if !config.respect_gitignore
                                        || !should_ignore(&entry_path, &rules)
                                    {
                                        files.push(entry_path.display().to_string());
                                    }
                                }
                                serde_json::json!({ "files": files })
                            }
                            Err(e) => serde_json::json!({ "error": e.to_string() }),
                        }
                    }
                    "search_files" => {
                        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

                        let mut matches = Vec::new();
                        let search_path = std::path::Path::new(path);
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };

                        let run_res = if search_path.is_file() {
                            if (!config.respect_gitignore || !should_ignore(search_path, &rules))
                                && let Ok(content) = std::fs::read_to_string(search_path)
                            {
                                for (line_num, line) in content.lines().enumerate() {
                                    if line.contains(pattern) {
                                        matches.push(format!(
                                            "{}:{}:{}",
                                            search_path.display(),
                                            line_num + 1,
                                            line
                                        ));
                                    }
                                }
                            }
                            Ok(())
                        } else {
                            recursive_search(search_path, pattern, &rules, &mut matches)
                        };

                        match run_res {
                            Ok(_) => {
                                let stdout = matches.join("\n");
                                serde_json::json!({ "matches": stdout })
                            }
                            Err(e) => serde_json::json!({ "error": e.to_string() }),
                        }
                    }
                    "edit_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        if config.respect_gitignore
                            && should_ignore(std::path::Path::new(path), &rules)
                        {
                            serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                        } else {
                            let old_string = args
                                .get("old_string")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let new_string = args
                                .get("new_string")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            match std::fs::read_to_string(path) {
                                Ok(content) => {
                                    if content.contains(old_string) {
                                        let mut diff = String::new();

                                        let old_lines: Vec<&str> = old_string.lines().collect();
                                        let new_lines: Vec<&str> = new_string.lines().collect();

                                        diff.push_str("```diff\n");
                                        for line in &old_lines {
                                            diff.push_str("- ");
                                            diff.push_str(line);
                                            diff.push('\n');
                                        }
                                        for line in &new_lines {
                                            diff.push_str("+ ");
                                            diff.push_str(line);
                                            diff.push('\n');
                                        }
                                        diff.push_str("```");

                                        let new_content =
                                            content.replacen(old_string, new_string, 1);
                                        match std::fs::write(path, new_content) {
                                            Ok(_) => {
                                                serde_json::json!({ "success": true, "diff": diff })
                                            }
                                            Err(e) => {
                                                serde_json::json!({ "error": format!("Failed to write file: {}", e) })
                                            }
                                        }
                                    } else {
                                        serde_json::json!({ "error": "old_string not found in file." })
                                    }
                                }
                                Err(e) => {
                                    serde_json::json!({ "error": format!("Failed to read file: {}", e) })
                                }
                            }
                        }
                    }
                    "write_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        if config.respect_gitignore
                            && should_ignore(std::path::Path::new(path), &rules)
                        {
                            serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                        } else {
                            let content =
                                args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let write_res = (|| -> Result<(), std::io::Error> {
                                if let Some(parent) = std::path::Path::new(path).parent()
                                    && !parent.as_os_str().is_empty()
                                {
                                    std::fs::create_dir_all(parent)?;
                                }
                                std::fs::write(path, content)?;
                                Ok(())
                            })();
                            match write_res {
                                Ok(_) => serde_json::json!({ "success": true }),
                                Err(e) => {
                                    serde_json::json!({ "error": format!("Failed to write file: {}", e) })
                                }
                            }
                        }
                    }
                    "run_bash_command" => {
                        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                        let background = args.get("background").and_then(|v| v.as_bool()).unwrap_or(false);
                        let input = args.get("input").and_then(|v| v.as_str());
                        let persistent_session_id = args.get("persistent_session_id").and_then(|v| v.as_str());

                        if let Some(session_id) = persistent_session_id {
                            match run_persistent_bash(session_id, cmd, input, sender.clone()) {
                                Ok(val) => val,
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        } else {
                            let run_result = (|| -> Result<serde_json::Value, std::io::Error> {
                                let mut command = std::process::Command::new("bash");
                                command
                                    .arg("-c")
                                    .arg(cmd)
                                    .stdin(std::process::Stdio::piped())
                                    .stdout(std::process::Stdio::piped())
                                    .stderr(std::process::Stdio::piped());

                                #[cfg(unix)]
                                {
                                    use std::os::unix::process::CommandExt;
                                    unsafe {
                                        command.pre_exec(|| {
                                            unsafe extern "C" {
                                                fn setpgid(pid: i32, pgid: i32) -> i32;
                                            }
                                            setpgid(0, 0);
                                            Ok(())
                                        });
                                    }
                                }

                                let mut child = command.spawn()?;

                                let pid = child.id();
                                
                                if let Some(mut stdin) = child.stdin.take() {
                                    if let Some(ref inp) = input {
                                        use std::io::Write;
                                        let _ = stdin.write_all(inp.as_bytes());
                                        let _ = stdin.flush();
                                    }
                                    if !background {
                                        if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_STDIN.lock() {
                                            *guard = Some(stdin);
                                        }
                                    }
                                }

                                let stdout = child.stdout.take().unwrap();
                                let stderr = child.stderr.take().unwrap();

                                let stdout_accumulator =
                                    std::sync::Arc::new(std::sync::Mutex::new(String::new()));
                                let stderr_accumulator =
                                    std::sync::Arc::new(std::sync::Mutex::new(String::new()));

                                let sender_stdout = sender.clone();
                                let stdout_acc_clone = stdout_accumulator.clone();
                                let stdout_handle = std::thread::spawn(move || {
                                    use std::io::Read;
                                    let mut buffer = [0; 1024];
                                    let mut reader = stdout;
                                    while let Ok(n) = reader.read(&mut buffer) {
                                        if n == 0 {
                                            break;
                                        }
                                        let chunk = String::from_utf8_lossy(&buffer[..n]).into_owned();
                                        if let Ok(mut guard) = stdout_acc_clone.lock() {
                                            guard.push_str(&chunk);
                                        }
                                        let _ = sender_stdout.send(WorkerEvent::BashStdout(chunk));
                                    }
                                });

                                let sender_stderr = sender.clone();
                                let stderr_acc_clone = stderr_accumulator.clone();
                                let stderr_handle = std::thread::spawn(move || {
                                    use std::io::Read;
                                    let mut buffer = [0; 1024];
                                    let mut reader = stderr;
                                    while let Ok(n) = reader.read(&mut buffer) {
                                        if n == 0 {
                                            break;
                                        }
                                        let chunk = String::from_utf8_lossy(&buffer[..n]).into_owned();
                                        if let Ok(mut guard) = stderr_acc_clone.lock() {
                                            guard.push_str(&chunk);
                                        }
                                        let _ = sender_stderr.send(WorkerEvent::BashStderr(chunk));
                                    }
                                });

                                if background {
                                    register_background_process(
                                        pid,
                                        cmd.to_owned(),
                                        child,
                                        stdout_accumulator.clone(),
                                        stderr_accumulator.clone(),
                                    );
                                    std::thread::spawn(move || {
                                        let _ = stdout_handle.join();
                                        let _ = stderr_handle.join();
                                    });
                                    Ok(serde_json::json!({
                                        "status": "running",
                                        "pid": pid
                                    }))
                                } else {
                                    if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                                        *guard = Some(pid);
                                    }
                                    let status = child.wait()?;
                                    if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                                        *guard = None;
                                    }
                                    if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_STDIN.lock() {
                                        *guard = None;
                                    }

                                    let _ = stdout_handle.join();
                                    let _ = stderr_handle.join();

                                    let stdout_content = stdout_accumulator
                                        .lock()
                                        .map(|g| g.clone())
                                        .unwrap_or_default();
                                    let stderr_content = stderr_accumulator
                                        .lock()
                                        .map(|g| g.clone())
                                        .unwrap_or_default();
                                    let status_code = status.code();
                                    let mut err_val = serde_json::Value::Null;

                                    if status_code.is_none() {
                                        err_val =
                                            serde_json::json!("Process terminated by user via Ctrl+C");
                                    }

                                    Ok(serde_json::json!({
                                        "status": status_code,
                                        "stdout": stdout_content,
                                        "stderr": stderr_content,
                                        "error": err_val,
                                    }))
                                }
                            })();

                            match run_result {
                                Ok(val) => val,
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                            }
                        }
                    }
                    "check_process" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        run_check_process(pid)
                    }
                    "kill_process" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        run_kill_process(pid)
                    }
                    "get_logs" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);
                        run_get_logs(pid, limit)
                    }
                    "edit_file_lines" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };
                        if config.respect_gitignore
                            && should_ignore(std::path::Path::new(path), &rules)
                        {
                            serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                        } else {
                            let start_line = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                            let end_line = args.get("end_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                            let new_content = args.get("new_content").and_then(|v| v.as_str()).unwrap_or("");

                            if start_line == 0 {
                                serde_json::json!({ "error": "start_line must be greater than or equal to 1" })
                            } else if start_line > end_line {
                                serde_json::json!({ "error": "start_line cannot be greater than end_line" })
                            } else {
                                match std::fs::read_to_string(path) {
                                    Ok(content) => {
                                        let lines: Vec<&str> = content.lines().collect();
                                        if start_line > lines.len() {
                                            serde_json::json!({ "error": format!("start_line {} is beyond file line count {}", start_line, lines.len()) })
                                        } else {
                                            let end_idx = std::cmp::min(end_line, lines.len());
                                            
                                            let mut diff = String::new();
                                            diff.push_str("```diff\n");
                                            for i in (start_line - 1)..end_idx {
                                                diff.push_str("- ");
                                                diff.push_str(lines[i]);
                                                diff.push('\n');
                                            }
                                            for line in new_content.lines() {
                                                diff.push_str("+ ");
                                                diff.push_str(line);
                                                diff.push('\n');
                                            }
                                            diff.push_str("```");

                                            let mut new_lines = Vec::new();
                                            for i in 0..(start_line - 1) {
                                                new_lines.push(lines[i]);
                                            }
                                            for line in new_content.lines() {
                                                new_lines.push(line);
                                            }
                                            for i in end_idx..lines.len() {
                                                new_lines.push(lines[i]);
                                            }
                                            let mut new_content_str = new_lines.join("\n");
                                            if content.ends_with('\n') && !new_content_str.is_empty() {
                                                new_content_str.push('\n');
                                            }

                                            match std::fs::write(path, new_content_str) {
                                                Ok(_) => {
                                                    serde_json::json!({ "success": true, "diff": diff })
                                                }
                                                Err(e) => {
                                                    serde_json::json!({ "error": format!("Failed to write file: {}", e) })
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        serde_json::json!({ "error": format!("Failed to read file: {}", e) })
                                    }
                                }
                            }
                        }
                    }
                    "apply_patch" => {
                        let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
                        let run_res = (|| -> Result<(), String> {
                            let mut cmd = std::process::Command::new("git");
                            cmd.arg("apply").arg("-");
                            cmd.stdin(std::process::Stdio::piped());
                            cmd.stdout(std::process::Stdio::piped());
                            cmd.stderr(std::process::Stdio::piped());
                            let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn git apply: {}", e))?;
                            if let Some(mut stdin) = child.stdin.take() {
                                use std::io::Write;
                                stdin.write_all(patch.as_bytes()).map_err(|e| format!("Failed to write to stdin: {}", e))?;
                            }
                            let output = child.wait_with_output().map_err(|e| format!("Failed waiting for git apply: {}", e))?;
                            if output.status.success() {
                                Ok(())
                            } else {
                                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                                Err(format!("git apply failed:\nstdout: {}\nstderr: {}", stdout, stderr))
                            }
                        })();
                        match run_res {
                            Ok(_) => serde_json::json!({ "success": true }),
                            Err(e) => serde_json::json!({ "error": e }),
                        }
                    }
                    "web_search" => {
                        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                        let res = if query.starts_with("http://") || query.starts_with("https://") {
                            (|| -> Result<serde_json::Value, String> {
                                let body: String = ureq::get(query)
                                    .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                                    .call()
                                    .map_err(|e| e.to_string())?
                                    .into_string()
                                    .map_err(|e| e.to_string())?;
                                let plain_text = html_to_plain_text(&body);
                                let truncated: String = plain_text.chars().take(8000).collect();
                                Ok(serde_json::json!({ "content": truncated }))
                            })()
                        } else {
                            (|| -> Result<serde_json::Value, String> {
                                let encoded_query = url_encode(query);
                                let search_url = format!(
                                    "https://html.duckduckgo.com/html/?q={}",
                                    encoded_query
                                );
                                let body: String = ureq::get(&search_url)
                                    .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                                    .call()
                                    .map_err(|e| e.to_string())?
                                    .into_string()
                                    .map_err(|e| e.to_string())?;
                                let results = parse_ddg_html(&body);
                                Ok(serde_json::json!({ "results": results }))
                            })()
                        };
                        match res {
                            Ok(val) => val,
                            Err(e) => serde_json::json!({ "error": e }),
                        }
                    }
                    "ask_user" => {
                        let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("");
                        let options = args
                            .get("options")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                                    .collect::<Vec<String>>()
                            })
                            .unwrap_or_default();

                        let (tx, rx) = std::sync::mpsc::channel();
                        if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
                            *guard = Some((tx, question.to_owned(), options));
                        }

                        let answer = rx.recv().unwrap_or_default();

                        if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
                            *guard = None;
                        }

                        serde_json::json!({ "answer": answer })
                    }
                    _ => serde_json::json!({ "error": "Unknown function" }),
                };
                let _ = sender.send(WorkerEvent::ToolResult(name, result));
            });
        }
        crate::app::FunctionAction::ResumeGeneration(request) => {
            spawn_generation_worker(
                request.config,
                request.history,
                request.cancel_token,
                request.generation_id,
                sender.clone(),
            );
        }
    }
}

pub(crate) fn spawn_generation_worker(
    config: StoredConfig,
    history: Vec<crate::api::ChatMessage>,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    generation_id: usize,
    sender: Sender<WorkerEvent>,
) {
    thread::spawn(move || {
        let sender_clone = sender.clone();
        let cancel_clone = cancel_token.clone();

        let mut retries = 0;
        loop {
            if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = sender.send(WorkerEvent::StreamError(
                    generation_id,
                    "Stream cancelled".to_owned(),
                ));
                return;
            }
            let sender_c = sender_clone.clone();
            let cancel_c = cancel_clone.clone();
            let history_c = history.clone();
            let result = GeminiClient::new(config.clone()).generate_stream(
                &history_c,
                cancel_c,
                move |chunk| {
                    let _ = sender_c.send(WorkerEvent::StreamChunk(generation_id, chunk));
                    Ok(())
                },
            );
            match result {
                Ok(_) => {
                    let _ = sender.send(WorkerEvent::StreamDone(generation_id));
                    return;
                }
                Err(error) => {
                    if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = sender.send(WorkerEvent::StreamError(
                            generation_id,
                            "Stream cancelled".to_owned(),
                        ));
                        return;
                    }
                    retries += 1;
                    if retries < 3 {
                        let _ = sender.send(WorkerEvent::ResetStream(generation_id));
                        thread::sleep(Duration::from_millis(500));
                    } else {
                        let _ =
                            sender.send(WorkerEvent::StreamError(generation_id, error.to_string()));
                        return;
                    }
                }
            }
        }
    });
}

pub(crate) fn spawn_models_worker(config: StoredConfig, sender: Sender<WorkerEvent>) {
    thread::spawn(move || {
        let result = GeminiClient::new(config)
            .list_models()
            .map_err(|error| error.to_string());
        let _ = sender.send(WorkerEvent::Models(result));
    });
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn percent_decode(input: &str) -> String {
    let mut decoded = String::new();
    let mut bytes = input.as_bytes().iter();
    while let Some(&b) = bytes.next() {
        if b == b'%' {
            let hex_opt = bytes.next().zip(bytes.next()).and_then(|(&h1, &h2)| {
                let hex_bytes = [h1, h2];
                std::str::from_utf8(&hex_bytes)
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
            });
            if let Some(val) = hex_opt {
                decoded.push(val as char);
                continue;
            }
        }
        decoded.push(b as char);
    }
    decoded
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in input.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .trim()
        .to_owned()
}

fn html_to_plain_text(html: &str) -> String {
    let mut cleaned = html.to_owned();
    while let Some(start) = cleaned.find("<script") {
        let rest = &cleaned[start..];
        if let Some(end) = rest.find("</script>") {
            cleaned.replace_range(start..start + end + 9, " ");
        } else {
            break;
        }
    }
    while let Some(start) = cleaned.find("<style") {
        let rest = &cleaned[start..];
        if let Some(end) = rest.find("</style>") {
            cleaned.replace_range(start..start + end + 8, " ");
        } else {
            break;
        }
    }
    strip_html_tags(&cleaned)
}

fn parse_ddg_html(html: &str) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    for block in html.split("<div class=\"result").skip(1) {
        if results.len() >= 6 {
            break;
        }

        let href = if let Some(href_start) = block.find("href=\"") {
            let rest = &block[href_start + 6..];
            if let Some(href_end) = rest.find("\"") {
                let raw_url = &rest[..href_end];
                if let Some(uddg_idx) = raw_url.find("uddg=") {
                    let encoded_url = &raw_url[uddg_idx + 5..];
                    let decoded_url = percent_decode(encoded_url);
                    if let Some(amp_idx) = decoded_url.find('&') {
                        decoded_url[..amp_idx].to_owned()
                    } else {
                        decoded_url
                    }
                } else {
                    raw_url.to_owned()
                }
            } else {
                continue;
            }
        } else {
            continue;
        };

        let title = if let Some(title_start) = block.find("class=\"result__a\"") {
            let rest = &block[title_start..];
            if let Some(tag_close) = rest.find('>') {
                let rest_text = &rest[tag_close + 1..];
                if let Some(tag_open) = rest_text.find("</a>") {
                    strip_html_tags(&rest_text[..tag_open])
                } else {
                    "Untitled".to_owned()
                }
            } else {
                "Untitled".to_owned()
            }
        } else {
            "Untitled".to_owned()
        };

        let snippet = if let Some(snippet_start) = block.find("class=\"result__snippet\"") {
            let rest = &block[snippet_start..];
            if let Some(tag_close) = rest.find('>') {
                let rest_text = &rest[tag_close + 1..];
                if let Some(tag_open) = rest_text.find("</a>") {
                    strip_html_tags(&rest_text[..tag_open])
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        results.push(serde_json::json!({
            "title": title,
            "url": href,
            "snippet": snippet
        }));
    }
    results
}
