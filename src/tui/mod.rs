pub(crate) mod events;
pub(crate) mod render;
pub(crate) mod syntax;
pub(crate) mod keybindings;

use std::io::{self, Stdout};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::execute;
use crossterm::event::{self, Event};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::config::StoredConfig;
use crate::gemini::{GeminiClient, GeminiResponse};

type Tui = Terminal<CrosstermBackend<Stdout>>;

pub static RUNNING_PROCESS_PID: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);

pub(crate) enum WorkerEvent {
    StreamChunk(usize, GeminiResponse),
    StreamDone(usize),
    StreamError(usize, String),
    Models(Result<Vec<String>, String>),
    ToolResult(String, serde_json::Value),
    ResetStream(usize),
}

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = start_terminal()?;
    let (sender, receiver) = mpsc::channel();
    let result = run_loop(&mut terminal, &mut app, &sender, &receiver);
    stop_terminal(&mut terminal)?;
    result
}

fn start_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, event::EnableFocusChange)?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(Into::into)
}

fn stop_terminal(terminal: &mut Tui) -> Result<()> {
    let _ = execute!(io::stdout(), event::DisableFocusChange);
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
        app.advance_tick();
        while let Ok(event) = receiver.try_recv() {
            handle_worker_event(app, event, sender);
        }

        if app.screen == crate::app::Screen::Chat && !app.is_busy() && !app.chat.message_queue.is_empty()
            && let Some(action) = app.pop_and_start_next_queue_item() {
                match action {
                    crate::app::SubmitAction::Generate(request) => {
                        spawn_generation_worker(request.config, request.history, request.cancel_token, request.generation_id, sender.clone());
                    }
                    crate::app::SubmitAction::LoadModels(config) => {
                        spawn_models_worker(config, sender.clone());
                    }
                    crate::app::SubmitAction::ExecuteFunction { name, args } => {
                        handle_function_action(crate::app::FunctionAction::Execute { name, args }, sender);
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
        WorkerEvent::StreamDone(id) => {
            if id == app.generation_id {
                if let Some(action) = app.complete_stream() {
                    handle_function_action(action, sender);
                }
            }
        }
        WorkerEvent::StreamError(id, err) => {
            if id == app.generation_id {
                app.handle_stream_error(err);
            }
        }
        WorkerEvent::Models(result) => app.complete_load_models(result),
        WorkerEvent::ToolResult(name, response) => {
            if let Some(crate::app::FunctionAction::ResumeGeneration(request)) = app.complete_function_execution(name, response) {
                spawn_generation_worker(request.config, request.history, request.cancel_token, request.generation_id, sender.clone());
            }
        }
        WorkerEvent::ResetStream(id) => {
            if id == app.generation_id {
                app.chat.streaming_parts.clear();
            }
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
                let rule = trimmed.trim_end_matches('/').to_owned();
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
                && let Ok(content) = std::fs::read_to_string(&path) {
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

pub(crate) fn handle_function_action(action: crate::app::FunctionAction, sender: &Sender<WorkerEvent>) {
    match action {
        crate::app::FunctionAction::Execute { name, args } => {
            let sender = sender.clone();
            thread::spawn(move || {
                let result = match name.as_str() {
                    "read_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        match std::fs::read_to_string(path) {
                            Ok(content) => serde_json::json!({ "content": content }),
                            Err(e) => serde_json::json!({ "error": e.to_string() }),
                        }
                    }
                    "read_files" => {
                        let paths = args.get("paths").and_then(|v| v.as_array());
                        if let Some(paths) = paths {
                            let mut results = serde_json::Map::new();
                            for path_val in paths {
                                if let Some(path) = path_val.as_str() {
                                    match std::fs::read_to_string(path) {
                                        Ok(content) => {
                                            results.insert(path.to_owned(), serde_json::json!({ "content": content }));
                                        }
                                        Err(e) => {
                                            results.insert(path.to_owned(), serde_json::json!({ "error": e.to_string() }));
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
                            let mut parsed_edits = Vec::new();
                            let mut validation_errors = Vec::new();
                            
                            for (idx, edit_val) in edits.iter().enumerate() {
                                let path = edit_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                                let old_string = edit_val.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                                let new_string = edit_val.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                                if path.is_empty() {
                                    validation_errors.push(format!("Edit at index {} is missing 'path'", idx));
                                    continue;
                                }
                                parsed_edits.push((path.to_owned(), old_string.to_owned(), new_string.to_owned()));
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
                                                apply_errors.push(format!("Failed to read `{}`: {}", path, e));
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
                                        let current_content = working_contents.get_mut(path).unwrap();
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
                                            
                                            *current_content = current_content.replacen(old_string, new_string, 1);
                                        } else {
                                            apply_errors.push(format!("old_string not found in `{}`", path));
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
                                                    write_error = Some(format!("Failed to write `{}`: {}", path, e));
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
                        match std::fs::read_dir(path) {
                            Ok(entries) => {
                                let mut files = Vec::new();
                                for entry in entries.filter_map(Result::ok) {
                                    files.push(entry.path().display().to_string());
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
                        let rules = load_gitignore_rules();
                        
                        let run_res = if search_path.is_file() {
                            if !should_ignore(search_path, &rules)
                                && let Ok(content) = std::fs::read_to_string(search_path) {
                                    for (line_num, line) in content.lines().enumerate() {
                                        if line.contains(pattern) {
                                            matches.push(format!("{}:{}:{}", search_path.display(), line_num + 1, line));
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
                        let old_string = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                        let new_string = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
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

                                    let new_content = content.replacen(old_string, new_string, 1);
                                    match std::fs::write(path, new_content) {
                                        Ok(_) => serde_json::json!({ "success": true, "diff": diff }),
                                        Err(e) => serde_json::json!({ "error": format!("Failed to write file: {}", e) }),
                                    }
                                } else {
                                    serde_json::json!({ "error": "old_string not found in file." })
                                }
                            }
                            Err(e) => serde_json::json!({ "error": format!("Failed to read file: {}", e) }),
                        }
                    }
                    "write_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let write_res = (|| -> Result<(), std::io::Error> {
                            if let Some(parent) = std::path::Path::new(path).parent()
                                && !parent.as_os_str().is_empty() {
                                    std::fs::create_dir_all(parent)?;
                                }
                            std::fs::write(path, content)?;
                            Ok(())
                        })();
                        match write_res {
                            Ok(_) => serde_json::json!({ "success": true }),
                            Err(e) => serde_json::json!({ "error": format!("Failed to write file: {}", e) }),
                        }
                    }
                    "run_bash_command" => {
                        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                        
                        let run_result = (|| -> Result<serde_json::Value, std::io::Error> {
                            let child = std::process::Command::new("bash")
                                .arg("-c")
                                .arg(cmd)
                                .stdout(std::process::Stdio::piped())
                                .stderr(std::process::Stdio::piped())
                                .spawn()?;
                            
                            let pid = child.id();
                            if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                                *guard = Some(pid);
                            }
                            
                            let output = child.wait_with_output();
                            
                            if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                                *guard = None;
                            }
                            
                            let output = output?;
                            let stdout_content = String::from_utf8_lossy(&output.stdout).into_owned();
                            let mut stderr_content = String::from_utf8_lossy(&output.stderr).into_owned();
                            let status_code = output.status.code();
                            let mut err_val = serde_json::Value::Null;
                            
                            if status_code.is_none() {
                                err_val = serde_json::json!("Process terminated by user via Ctrl+C");
                                if !stderr_content.is_empty() {
                                    stderr_content.push('\n');
                                }
                                stderr_content.push_str("[Process terminated by user via Ctrl+C]");
                            }
                            
                            Ok(serde_json::json!({
                                "status": status_code,
                                "stdout": stdout_content,
                                "stderr": stderr_content,
                                "error": err_val,
                            }))
                        })();
                        
                        match run_result {
                            Ok(val) => val,
                            Err(e) => serde_json::json!({ "error": e.to_string() })
                        }
                    }
                    _ => serde_json::json!({ "error": "Unknown function" }),
                };
                let _ = sender.send(WorkerEvent::ToolResult(name, result));
            });
        }
        crate::app::FunctionAction::ResumeGeneration(request) => {
            spawn_generation_worker(request.config, request.history, request.cancel_token, request.generation_id, sender.clone());
        }
    }
}

pub(crate) fn spawn_generation_worker(
    config: StoredConfig,
    history: Vec<crate::gemini::ChatMessage>,
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
                let _ = sender.send(WorkerEvent::StreamError(generation_id, "Stream cancelled".to_owned()));
                return;
            }
            let sender_c = sender_clone.clone();
            let cancel_c = cancel_clone.clone();
            let history_c = history.clone();
            let result = GeminiClient::new(config.clone()).generate_stream(&history_c, cancel_c, move |chunk| {
                let _ = sender_c.send(WorkerEvent::StreamChunk(generation_id, chunk));
                Ok(())
            });
            match result {
                Ok(_) => {
                    let _ = sender.send(WorkerEvent::StreamDone(generation_id));
                    return;
                }
                Err(error) => {
                    if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = sender.send(WorkerEvent::StreamError(generation_id, "Stream cancelled".to_owned()));
                        return;
                    }
                    retries += 1;
                    if retries < 3 {
                        let _ = sender.send(WorkerEvent::ResetStream(generation_id));
                        thread::sleep(Duration::from_millis(500));
                    } else {
                        let _ = sender.send(WorkerEvent::StreamError(generation_id, error.to_string()));
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
