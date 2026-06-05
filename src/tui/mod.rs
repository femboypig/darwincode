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

pub(crate) struct BackgroundProcess {
    pub(crate) _command: String,
    pub(crate) child: Arc<Mutex<std::process::Child>>,
    pub(crate) stdin: Option<std::process::ChildStdin>,
    pub(crate) stdout_accumulator: Arc<Mutex<String>>,
    pub(crate) stderr_accumulator: Arc<Mutex<String>>,
    pub(crate) exit_status: Arc<Mutex<Option<i32>>>,
}

pub(crate) static BACKGROUND_PROCESSES: OnceLock<Mutex<HashMap<u32, BackgroundProcess>>> =
    OnceLock::new();

pub(crate) struct PersistentSession {
    pub(crate) pid: u32,
    pub(crate) child: Arc<Mutex<std::process::Child>>,
    pub(crate) stdin: std::process::ChildStdin,
    pub(crate) stdout_accumulator: Arc<Mutex<String>>,
    pub(crate) stderr_accumulator: Arc<Mutex<String>>,
}

pub(crate) static PERSISTENT_SESSIONS: OnceLock<Mutex<HashMap<String, PersistentSession>>> =
    OnceLock::new();
pub(crate) static ACTIVE_PERSISTENT_SESSION_ID: Mutex<Option<String>> = Mutex::new(None);

fn register_background_process(
    pid: u32,
    command: String,
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    stdout_acc: Arc<Mutex<String>>,
    stderr_acc: Arc<Mutex<String>>,
) {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    let child_arc = Arc::new(Mutex::new(child));
    let exit_status = Arc::new(Mutex::new(None));

    let child_clone = child_arc.clone();
    let exit_status_clone = exit_status.clone();

    // Spawn non-blocking monitor thread to poll process exit status
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(mut child_guard) = child_clone.lock() {
                match child_guard.try_wait() {
                    Ok(Some(status)) => {
                        let mut status_guard = exit_status_clone.lock().unwrap();
                        *status_guard = Some(status.code().unwrap_or(-1));
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        let mut status_guard = exit_status_clone.lock().unwrap();
                        *status_guard = Some(-1);
                        break;
                    }
                }
            } else {
                break;
            }
        }
    });

    if let Ok(mut map) = registry.lock() {
        map.insert(
            pid,
            BackgroundProcess {
                _command: command,
                child: child_arc,
                stdin,
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
            if exit_code_guard.is_none()
                && let Ok(mut child_guard) = proc.child.try_lock()
                && let Ok(Some(status)) = child_guard.try_wait()
            {
                *exit_code_guard = Some(status.code().unwrap_or(-1));
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
            if let Ok(mut c) = proc.child.try_lock() {
                let _ = c.kill();
            }
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &format!("-{}", pid)])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
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
            if exit_code_guard.is_none()
                && let Ok(mut child_guard) = proc.child.try_lock()
                && let Ok(Some(status)) = child_guard.try_wait()
            {
                *exit_code_guard = Some(status.code().unwrap_or(-1));
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

#[allow(clippy::zombie_processes)]
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
        let pid = child.id();
        let child_arc = Arc::new(Mutex::new(child));

        let sender_stdout = sender.clone();
        let stdout_acc = Arc::new(Mutex::new(String::new()));
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
                let _ = sender_stdout.send(WorkerEvent::BashStdout(Some(pid), chunk));
            }
        });

        let sender_stderr = sender.clone();
        let stderr_acc = Arc::new(Mutex::new(String::new()));
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
                let _ = sender_stderr.send(WorkerEvent::BashStderr(Some(pid), chunk));
            }
        });

        PersistentSession {
            pid,
            child: child_arc,
            stdin,
            stdout_accumulator: stdout_acc,
            stderr_accumulator: stderr_acc,
        }
    });

    // Set the active persistent session ID for keystroke forwarding
    if let Ok(mut guard) = ACTIVE_PERSISTENT_SESSION_ID.lock() {
        *guard = Some(session_id.to_owned());
    }

    use std::io::Write;

    let start_stdout_len = entry.stdout_accumulator.lock().unwrap().len();
    let start_stderr_len = entry.stderr_accumulator.lock().unwrap().len();

    let nonce = format!(
        "CMD_DONE_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
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
    let mut has_exited = false;
    let mut exit_status = None;

    while check_count < max_checks {
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Check if bash process has exited early
        if let Ok(mut child_guard) = entry.child.lock()
            && let Ok(Some(status)) = child_guard.try_wait()
        {
            has_exited = true;
            exit_status = Some(status.code().unwrap_or(-1));
            break;
        }

        let stdout_guard = entry.stdout_accumulator.lock().unwrap();
        if stdout_guard[start_stdout_len..].contains(&sentinel) {
            found = true;
            break;
        }
        check_count += 1;
    }

    // Clear active persistent session ID
    if let Ok(mut guard) = ACTIVE_PERSISTENT_SESSION_ID.lock() {
        *guard = None;
    }

    // Clean up registry entry if process has exited
    if has_exited {
        let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut map) = registry.lock() {
            map.remove(session_id);
        }
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
        "status": if found {
            serde_json::json!(0)
        } else if has_exited {
            serde_json::json!(exit_status.unwrap_or(-1))
        } else {
            serde_json::Value::Null
        },
        "stdout": clean_stdout,
        "stderr": stderr_diff,
        "pid": entry.pid,
        "error": if found {
            serde_json::Value::Null
        } else if has_exited {
            serde_json::json!("Shell process exited")
        } else {
            serde_json::json!("Command timed out / is still running")
        }
    }))
}

pub(crate) enum WorkerEvent {
    StreamChunk(usize, GeminiResponse),
    StreamDone(usize),
    StreamError(usize, String),
    Models(Result<Vec<String>, String>),
    ToolResult(String, serde_json::Value),
    ResetStream(usize),
    BashStdout(Option<u32>, String),
    BashStderr(Option<u32>, String),
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
        event::EnableBracketedPaste,
        event::EnableMouseCapture
    )?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(Into::into)
}

fn stop_terminal(terminal: &mut Tui) -> Result<()> {
    let _ = execute!(
        io::stdout(),
        event::DisableFocusChange,
        event::DisableBracketedPaste,
        event::DisableMouseCapture
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
                        request.dev_mode,
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
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    event::MouseEventKind::ScrollUp => {
                        if matches!(
                            app.pending,
                            Some(crate::app::PendingTask::ConfirmFunction { .. })
                        ) {
                            app.confirm_scroll
                                .set(app.confirm_scroll.get().saturating_sub(1));
                        } else {
                            app.chat.scroll = app.chat.scroll.saturating_add(1);
                        }
                    }
                    event::MouseEventKind::ScrollDown => {
                        if matches!(
                            app.pending,
                            Some(crate::app::PendingTask::ConfirmFunction { .. })
                        ) {
                            app.confirm_scroll
                                .set(app.confirm_scroll.get().saturating_add(1));
                        } else {
                            app.chat.scroll = app.chat.scroll.saturating_sub(1);
                        }
                    }
                    event::MouseEventKind::Down(event::MouseButton::Left) => {
                        let click_x = mouse_event.column;
                        let click_y = mouse_event.row;

                        if let Some(rect) = app.chat.mode_area.get()
                            && click_x >= rect.x
                            && click_x < rect.x + rect.width
                            && click_y == rect.y
                        {
                            app.toggle_dev_mode();
                            continue;
                        }

                        if let Some(rect) = app.chat.model_area.get()
                            && click_x >= rect.x
                            && click_x < rect.x + rect.width
                            && click_y == rect.y
                        {
                            if app.model_picker_open {
                                app.cancel_models();
                            } else if let Some(config) = app.begin_load_chat_models() {
                                spawn_models_worker(config, sender.clone());
                            }
                            continue;
                        }

                        if let Some(rect) = app.chat.messages_area.get()
                            && click_x >= rect.x
                            && click_x < rect.x + rect.width
                            && click_y >= rect.y
                            && click_y < rect.y + rect.height
                        {
                            let total_lines = app
                                .chat
                                .message_line_ranges
                                .borrow()
                                .last()
                                .map(|(_, _, end)| *end)
                                .unwrap_or(0);
                            let viewport_height = rect.height as usize;
                            let max_scroll = total_lines.saturating_sub(viewport_height);
                            let scroll_offset = (app.chat.scroll as usize).min(max_scroll);
                            let scroll_y = max_scroll.saturating_sub(scroll_offset);

                            let clicked_line = scroll_y + (click_y - rect.y) as usize;

                            let mut clicked_msg_idx = None;
                            for &(msg_idx, start_line, end_line) in
                                app.chat.message_line_ranges.borrow().iter()
                            {
                                if clicked_line >= start_line && clicked_line < end_line {
                                    clicked_msg_idx = Some(msg_idx);
                                    break;
                                }
                            }

                            if let Some(msg_idx) = clicked_msg_idx
                                && app.chat.messages[msg_idx].is_shell
                            {
                                if let Some(session_id) =
                                    app.chat.messages[msg_idx].shell_session_id.clone()
                                {
                                    if let Ok(mut guard) = ACTIVE_PERSISTENT_SESSION_ID.lock() {
                                        let opt: &mut Option<String> = &mut guard;
                                        *opt = Some(session_id.clone());
                                    }
                                    app.chat.focused_shell_session_id = Some(session_id.clone());
                                    app.chat.focused_shell_pid = None;
                                    app.chat.shell_focused = true;

                                    for m in &mut app.chat.messages {
                                        if m.is_shell {
                                            *m.cached_wrapped.borrow_mut() = None;
                                        }
                                    }

                                    let mut scrolled = false;
                                    if let Some(&(_, start_line, end_line)) = app
                                        .chat
                                        .message_line_ranges
                                        .borrow()
                                        .iter()
                                        .find(|&&(idx, _, _)| idx == msg_idx)
                                    {
                                        let total_lines = app
                                            .chat
                                            .message_line_ranges
                                            .borrow()
                                            .last()
                                            .map(|(_, _, end)| *end)
                                            .unwrap_or(0);
                                        let viewport_height = rect.height as usize;
                                        let max_scroll =
                                            total_lines.saturating_sub(viewport_height);
                                        let msg_height = end_line.saturating_sub(start_line);
                                        let mid_line = start_line + msg_height / 2;
                                        let target_scroll_y =
                                            mid_line.saturating_sub(viewport_height / 2);
                                        let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                                        app.chat.scroll = scroll_val as u16;
                                        scrolled = true;
                                    }
                                    if !scrolled {
                                        app.chat.scroll = 0;
                                    }

                                    *app.chat.message_line_ranges.borrow_mut() = Vec::new();
                                    app.status = "Ready".to_owned();
                                } else {
                                    let clicked_pid = app.chat.messages[msg_idx].shell_pid;
                                    let fg_pid = RUNNING_PROCESS_PID.lock().ok().and_then(|g| *g);
                                    let is_bg_alive = if let Some(pid) = clicked_pid {
                                        let bg_registry = BACKGROUND_PROCESSES.get();
                                        if let Some(registry_mutex) = bg_registry
                                            && let Ok(registry_guard) = registry_mutex.lock()
                                            && let Some(proc) = registry_guard.get(&pid)
                                        {
                                            proc.exit_status.lock().unwrap().is_none()
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };

                                    if let Some(pid) = fg_pid
                                        && Some(pid) == clicked_pid
                                    {
                                        if let Ok(mut guard) = ACTIVE_PERSISTENT_SESSION_ID.lock() {
                                            *guard = None;
                                        }
                                        app.chat.focused_shell_session_id = None;
                                        app.chat.focused_shell_pid = Some(pid);
                                        app.chat.shell_focused = true;

                                        for m in &mut app.chat.messages {
                                            if m.is_shell {
                                                *m.cached_wrapped.borrow_mut() = None;
                                            }
                                        }

                                        let mut scrolled = false;
                                        if let Some(&(_, start_line, end_line)) = app
                                            .chat
                                            .message_line_ranges
                                            .borrow()
                                            .iter()
                                            .find(|&&(idx, _, _)| idx == msg_idx)
                                        {
                                            let total_lines = app
                                                .chat
                                                .message_line_ranges
                                                .borrow()
                                                .last()
                                                .map(|(_, _, end)| *end)
                                                .unwrap_or(0);
                                            let viewport_height = rect.height as usize;
                                            let max_scroll =
                                                total_lines.saturating_sub(viewport_height);
                                            let msg_height = end_line.saturating_sub(start_line);
                                            let mid_line = start_line + msg_height / 2;
                                            let target_scroll_y =
                                                mid_line.saturating_sub(viewport_height / 2);
                                            let scroll_val =
                                                max_scroll.saturating_sub(target_scroll_y);
                                            app.chat.scroll = scroll_val as u16;
                                            scrolled = true;
                                        }
                                        if !scrolled {
                                            app.chat.scroll = 0;
                                        }

                                        *app.chat.message_line_ranges.borrow_mut() = Vec::new();
                                        app.status = "Ready".to_owned();
                                    } else if is_bg_alive && let Some(pid) = clicked_pid {
                                        if let Ok(mut guard) = ACTIVE_PERSISTENT_SESSION_ID.lock() {
                                            *guard = None;
                                        }
                                        app.chat.focused_shell_session_id = None;
                                        app.chat.focused_shell_pid = Some(pid);
                                        app.chat.shell_focused = true;

                                        for m in &mut app.chat.messages {
                                            if m.is_shell {
                                                *m.cached_wrapped.borrow_mut() = None;
                                            }
                                        }

                                        let mut scrolled = false;
                                        if let Some(&(_, start_line, end_line)) = app
                                            .chat
                                            .message_line_ranges
                                            .borrow()
                                            .iter()
                                            .find(|&&(idx, _, _)| idx == msg_idx)
                                        {
                                            let total_lines = app
                                                .chat
                                                .message_line_ranges
                                                .borrow()
                                                .last()
                                                .map(|(_, _, end)| *end)
                                                .unwrap_or(0);
                                            let viewport_height = rect.height as usize;
                                            let max_scroll =
                                                total_lines.saturating_sub(viewport_height);
                                            let msg_height = end_line.saturating_sub(start_line);
                                            let mid_line = start_line + msg_height / 2;
                                            let target_scroll_y =
                                                mid_line.saturating_sub(viewport_height / 2);
                                            let scroll_val =
                                                max_scroll.saturating_sub(target_scroll_y);
                                            app.chat.scroll = scroll_val as u16;
                                            scrolled = true;
                                        }
                                        if !scrolled {
                                            app.chat.scroll = 0;
                                        }

                                        *app.chat.message_line_ranges.borrow_mut() = Vec::new();
                                        app.status = "Ready".to_owned();
                                    } else {
                                        app.chat
                                                .messages
                                                .push(crate::app::MessageLine::error(
                                                    "This shell process has already terminated or does not support input.".to_owned()
                                                ));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
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
                    request.dev_mode,
                    sender.clone(),
                );
            }
        }
        WorkerEvent::ResetStream(id) => {
            if id == app.generation_id {
                app.chat.streaming_parts.clear();
            }
        }
        WorkerEvent::BashStdout(pid, chunk) => {
            app.handle_bash_stdout(pid, chunk);
        }
        WorkerEvent::BashStderr(pid, chunk) => {
            app.handle_bash_stderr(pid, chunk);
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

fn matches_wildcard(name: &str, pattern: &str) -> bool {
    let mut pattern_parts = pattern.split('*');
    if let Some(first) = pattern_parts.next() {
        if !name.starts_with(first) {
            return false;
        }
        let mut last_idx = first.len();
        for part in pattern_parts {
            if part.is_empty() {
                return true;
            }
            if let Some(idx) = name[last_idx..].find(part) {
                last_idx += idx + part.len();
            } else {
                return false;
            }
        }
        last_idx == name.len() || pattern.ends_with('*')
    } else {
        name == pattern
    }
}

fn matches_pattern(path: &std::path::Path, base_dir: &std::path::Path, pattern: &str) -> bool {
    if pattern.contains('/') {
        if let Ok(rel_path) = path.strip_prefix(base_dir) {
            let rel_str = rel_path.to_string_lossy();
            matches_wildcard(&rel_str, pattern)
        } else {
            false
        }
    } else if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        matches_wildcard(file_name, pattern)
    } else {
        false
    }
}

fn recursive_glob(
    dir: &std::path::Path,
    base_dir: &std::path::Path,
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
                let _ = recursive_glob(&path, base_dir, pattern, rules, matches);
            } else if path.is_file() && matches_pattern(&path, base_dir, pattern) {
                matches.push(path.display().to_string());
                if matches.len() >= 1000 {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
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
                    "read" => {
                        let path = args.get("path").and_then(|v| v.as_str());
                        let paths = args.get("paths").and_then(|v| v.as_array());

                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };

                        if let Some(paths) = paths {
                            let mut results = serde_json::Map::new();
                            for path_val in paths {
                                if let Some(p_str) = path_val.as_str() {
                                    if config.respect_gitignore
                                        && should_ignore(std::path::Path::new(p_str), &rules)
                                    {
                                        results.insert(p_str.to_owned(), serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", p_str) }));
                                    } else {
                                        match std::fs::read_to_string(p_str) {
                                            Ok(content) => {
                                                results.insert(
                                                    p_str.to_owned(),
                                                    serde_json::json!({ "content": content }),
                                                );
                                            }
                                            Err(e) => {
                                                results.insert(
                                                    p_str.to_owned(),
                                                    serde_json::json!({ "error": e.to_string() }),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            serde_json::json!({ "files": results })
                        } else {
                            let target_path = path.unwrap_or(".");
                            let p = std::path::Path::new(target_path);
                            if config.respect_gitignore && should_ignore(p, &rules) {
                                serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", target_path) })
                            } else if p.is_dir() {
                                match std::fs::read_dir(p) {
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
                            } else if p.is_file() {
                                match std::fs::read_to_string(p) {
                                    Ok(content) => serde_json::json!({ "content": content }),
                                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                                }
                            } else {
                                serde_json::json!({ "error": format!("Path `{}` does not exist or is not readable", target_path) })
                            }
                        }
                    }
                    "grep" => {
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
                    "glob" => {
                        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

                        let mut matches = Vec::new();
                        let search_path = std::path::Path::new(path);
                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };

                        match recursive_glob(
                            search_path,
                            search_path,
                            pattern,
                            &rules,
                            &mut matches,
                        ) {
                            Ok(_) => {
                                let stdout = matches.join("\n");
                                serde_json::json!({ "matches": stdout })
                            }
                            Err(e) => serde_json::json!({ "error": e.to_string() }),
                        }
                    }
                    "edit" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let edits = args.get("edits").and_then(|v| v.as_array());

                        let rules = if config.respect_gitignore {
                            load_gitignore_rules()
                        } else {
                            Vec::new()
                        };

                        if let Some(edits) = edits {
                            let mut parsed_edits = Vec::new();
                            let mut validation_errors = Vec::new();

                            for (idx, edit_val) in edits.iter().enumerate() {
                                let edit_path =
                                    edit_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                                let old_string = edit_val
                                    .get("old_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let new_string = edit_val
                                    .get("new_string")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                if edit_path.is_empty() {
                                    validation_errors
                                        .push(format!("Edit at index {} is missing 'path'", idx));
                                    continue;
                                }
                                if config.respect_gitignore
                                    && should_ignore(std::path::Path::new(edit_path), &rules)
                                {
                                    validation_errors.push(format!(
                                        "Access denied: `{}` is ignored by .gitignore",
                                        edit_path
                                    ));
                                    continue;
                                }
                                parsed_edits.push((
                                    edit_path.to_owned(),
                                    old_string.to_owned(),
                                    new_string.to_owned(),
                                ));
                            }

                            if !validation_errors.is_empty() {
                                serde_json::json!({ "error": validation_errors.join("; ") })
                            } else {
                                let mut original_contents = std::collections::HashMap::new();
                                let mut apply_errors = Vec::new();

                                for (edit_path, _, _) in &parsed_edits {
                                    if !original_contents.contains_key(edit_path) {
                                        match std::fs::read_to_string(edit_path) {
                                            Ok(content) => {
                                                original_contents
                                                    .insert(edit_path.clone(), content);
                                            }
                                            Err(e) => {
                                                apply_errors.push(format!(
                                                    "Failed to read `{}`: {}",
                                                    edit_path, e
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

                                    for (edit_path, old_string, new_string) in &parsed_edits {
                                        let current_content =
                                            working_contents.get_mut(edit_path).unwrap();
                                        if current_content.contains(old_string) {
                                            let mut diff =
                                                format!("--- {}\n+++ {}\n", edit_path, edit_path);
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
                                                edit_path
                                            ));
                                            break;
                                        }
                                    }

                                    if !apply_errors.is_empty() {
                                        serde_json::json!({ "error": apply_errors.join("; ") })
                                    } else {
                                        let mut written_files = Vec::new();
                                        let mut write_error = None;

                                        for (edit_path, new_content) in &working_contents {
                                            match std::fs::write(edit_path, new_content) {
                                                Ok(_) => {
                                                    written_files.push(edit_path.clone());
                                                }
                                                Err(e) => {
                                                    write_error = Some(format!(
                                                        "Failed to write `{}`: {}",
                                                        edit_path, e
                                                    ));
                                                    break;
                                                }
                                            }
                                        }

                                        if let Some(err) = write_error {
                                            for edit_path in written_files {
                                                let orig =
                                                    original_contents.get(&edit_path).unwrap();
                                                let _ = std::fs::write(&edit_path, orig);
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
                        } else if args.get("start_line").is_some() {
                            if config.respect_gitignore
                                && should_ignore(std::path::Path::new(path), &rules)
                            {
                                serde_json::json!({ "error": format!("Access denied: `{}` is ignored by .gitignore", path) })
                            } else {
                                let start_line =
                                    args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1)
                                        as usize;
                                let end_line =
                                    args.get("end_line").and_then(|v| v.as_u64()).unwrap_or(1)
                                        as usize;
                                let new_content = args
                                    .get("new_content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");

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
                                                for line in
                                                    lines.iter().take(end_idx).skip(start_line - 1)
                                                {
                                                    diff.push_str("- ");
                                                    diff.push_str(line);
                                                    diff.push('\n');
                                                }
                                                for line in new_content.lines() {
                                                    diff.push_str("+ ");
                                                    diff.push_str(line);
                                                    diff.push('\n');
                                                }
                                                diff.push_str("```");

                                                let mut new_lines = Vec::new();
                                                for line in lines.iter().take(start_line - 1) {
                                                    new_lines.push(*line);
                                                }
                                                for line in new_content.lines() {
                                                    new_lines.push(line);
                                                }
                                                for line in lines.iter().skip(end_idx) {
                                                    new_lines.push(*line);
                                                }
                                                let mut new_content_str = new_lines.join("\n");
                                                if content.ends_with('\n')
                                                    && !new_content_str.is_empty()
                                                {
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
                        } else {
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
                    }
                    "write" => {
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
                    "sh" => {
                        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                        let background = args
                            .get("background")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let input = args.get("input").and_then(|v| v.as_str());
                        let persistent_session_id =
                            args.get("persistent_session_id").and_then(|v| v.as_str());

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

                                let mut child_stdin = child.stdin.take();
                                if let Some(ref mut stdin) = child_stdin.as_mut()
                                    && let Some(inp) = input
                                {
                                    use std::io::Write;
                                    let _ = stdin.write_all(inp.as_bytes());
                                    let _ = stdin.flush();
                                }
                                if !background
                                    && let Ok(mut guard) = crate::tui::RUNNING_PROCESS_STDIN.lock()
                                {
                                    *guard = child_stdin.take();
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
                                        let chunk =
                                            String::from_utf8_lossy(&buffer[..n]).into_owned();
                                        if let Ok(mut guard) = stdout_acc_clone.lock() {
                                            guard.push_str(&chunk);
                                        }
                                        let _ = sender_stdout
                                            .send(WorkerEvent::BashStdout(Some(pid), chunk));
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
                                        let chunk =
                                            String::from_utf8_lossy(&buffer[..n]).into_owned();
                                        if let Ok(mut guard) = stderr_acc_clone.lock() {
                                            guard.push_str(&chunk);
                                        }
                                        let _ = sender_stderr
                                            .send(WorkerEvent::BashStderr(Some(pid), chunk));
                                    }
                                });

                                if background {
                                    register_background_process(
                                        pid,
                                        cmd.to_owned(),
                                        child,
                                        child_stdin,
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
                                    if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_STDIN.lock()
                                    {
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
                                        err_val = serde_json::json!(
                                            "Process terminated by user via Ctrl+C"
                                        );
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
                    "ps" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        run_check_process(pid)
                    }
                    "kill" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        run_kill_process(pid)
                    }
                    "logs" => {
                        let pid = args.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let limit = args
                            .get("limit")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as usize);
                        run_get_logs(pid, limit)
                    }
                    "patch" => {
                        let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
                        let run_res = (|| -> Result<serde_json::Value, String> {
                            let file_path = patch
                                .lines()
                                .find_map(|line| {
                                    if let Some(stripped) = line.strip_prefix("--- a/") {
                                        Some(stripped)
                                    } else if let Some(stripped) = line.strip_prefix("+++ b/") {
                                        Some(stripped)
                                    } else {
                                        None
                                    }
                                })
                                .and_then(|rest| rest.split_whitespace().next());

                            let start_dir = if let Some(fp) = file_path {
                                let p = std::path::Path::new(fp);
                                let parent = p.parent().unwrap_or(std::path::Path::new(""));
                                if parent.as_os_str().is_empty() {
                                    std::env::current_dir()
                                        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                                } else {
                                    std::env::current_dir()
                                        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                                        .join(parent)
                                }
                            } else {
                                std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                            };

                            let mut cwd = start_dir;
                            let mut git_root = None;
                            loop {
                                if cwd.join(".git").exists() {
                                    git_root = Some(cwd.clone());
                                    break;
                                }
                                if let Some(parent) = cwd.parent() {
                                    cwd = parent.to_path_buf();
                                } else {
                                    break;
                                }
                            }

                            if git_root.is_none() {
                                let original_cwd = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("/"));
                                if let Ok(entries) = std::fs::read_dir(&original_cwd) {
                                    let mut candidates = Vec::new();
                                    for entry in entries.flatten() {
                                        let path = entry.path();
                                        if path.is_dir() && path.join(".git").exists() {
                                            candidates.push(path);
                                        }
                                    }

                                    if !candidates.is_empty() {
                                        let found = if let Some(fp) = file_path {
                                            candidates.iter().find(|cand| cand.join(fp).exists())
                                        } else {
                                            None
                                        };

                                        git_root = Some(match found {
                                            Some(cand) => cand.clone(),
                                            None => candidates[0].clone(),
                                        });
                                    }
                                }
                            }

                            let git_root = match git_root {
                                Some(root) => root,
                                None => return Err("Not a git repository".to_owned()),
                            };

                            let git_bin = if std::path::Path::new("/usr/bin/git").exists() {
                                "/usr/bin/git"
                            } else {
                                "git"
                            };

                            let random_val = {
                                let mut bytes = [0u8; 8];
                                rand::fill(&mut bytes);
                                bytes
                                    .iter()
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<String>()
                            };
                            let temp_path = std::path::PathBuf::from(format!(
                                "/tmp/_apply_patch_{}.diff",
                                random_val
                            ));
                            std::fs::write(&temp_path, patch).map_err(|e| {
                                format!("Failed to write temporary patch file: {}", e)
                            })?;

                            let cleanup = |path: &std::path::Path| {
                                let _ = std::fs::remove_file(path);
                            };

                            let mut cmd = std::process::Command::new(git_bin);
                            cmd.current_dir(&git_root);
                            cmd.args(["apply", &temp_path.to_string_lossy()]);
                            cmd.stdout(std::process::Stdio::piped());
                            cmd.stderr(std::process::Stdio::piped());

                            let output = cmd.output().map_err(|e| {
                                cleanup(&temp_path);
                                format!("Failed waiting for git apply: {}", e)
                            })?;

                            if !output.status.success() {
                                cleanup(&temp_path);
                                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                                return Err(format!("git apply failed:\n{}", stderr));
                            }

                            cleanup(&temp_path);

                            let diff_out = std::process::Command::new(git_bin)
                                .current_dir(&git_root)
                                .args(["diff", "--cached"])
                                .output();
                            let mut diff_str = match diff_out {
                                Ok(out) if out.status.success() => {
                                    String::from_utf8_lossy(&out.stdout).into_owned()
                                }
                                _ => String::new(),
                            };
                            if diff_str.is_empty() {
                                let diff_uncached = std::process::Command::new(git_bin)
                                    .current_dir(&git_root)
                                    .arg("diff")
                                    .output();
                                if let Ok(out) = diff_uncached {
                                    let success = out.status.success();
                                    if success {
                                        diff_str =
                                            String::from_utf8_lossy(&out.stdout).into_owned();
                                    }
                                }
                            }

                            Ok(serde_json::json!({
                                "success": true,
                                "diff": diff_str
                            }))
                        })();
                        match run_res {
                            Ok(val) => val,
                            Err(e) => serde_json::json!({ "error": e }),
                        }
                    }
                    "websearch" => {
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
                    "ask" => {
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
                    "todo" => {
                        serde_json::json!({ "success": true })
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
                request.dev_mode,
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
    dev_mode: String,
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
                &dev_mode,
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

fn html_to_plain_text(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    let mut text_parts = Vec::new();
    for node in document.tree.nodes() {
        use scraper::node::Node;
        if let Node::Text(text) = node.value() {
            let mut has_ignored_ancestor = false;
            let mut parent = node.parent();
            while let Some(p) = parent {
                if let Node::Element(elem) = p.value() {
                    let name = elem.name();
                    if name == "script" || name == "style" {
                        has_ignored_ancestor = true;
                        break;
                    }
                }
                parent = p.parent();
            }
            if !has_ignored_ancestor {
                text_parts.push(text.to_string());
            }
        }
    }

    let combined = text_parts.join(" ");
    let mut result = String::new();
    for word in combined.split_whitespace() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

fn parse_ddg_html(html: &str) -> Vec<serde_json::Value> {
    let document = scraper::Html::parse_document(html);
    let result_selector = scraper::Selector::parse(".result").unwrap();
    let a_selector = scraper::Selector::parse(".result__a").unwrap();
    let snippet_selector = scraper::Selector::parse(".result__snippet").unwrap();

    let mut results = Vec::new();
    for element in document.select(&result_selector) {
        if results.len() >= 6 {
            break;
        }

        let href = match element
            .select(&a_selector)
            .next()
            .and_then(|a| a.value().attr("href"))
        {
            Some(raw_url) => {
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
            }
            None => continue,
        };

        let title = element
            .select(&a_selector)
            .next()
            .map(|a| a.text().collect::<Vec<_>>().join(""))
            .unwrap_or_else(|| "Untitled".to_owned());

        let snippet = element
            .select(&snippet_selector)
            .next()
            .map(|s| s.text().collect::<Vec<_>>().join(""))
            .unwrap_or_default();

        results.push(serde_json::json!({
            "title": title.trim().to_owned(),
            "url": href,
            "snippet": snippet.trim().to_owned()
        }));
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_plain_text() {
        let html = "<html><head><title>Test</title><style>body { color: red; }</style></head>\
                    <body><h1>Hello World</h1><script>console.log('test');</script>\
                    <p>This is a <b>test</b> of the scraper.</p></body></html>";
        let text = html_to_plain_text(html);
        assert_eq!(text, "Test Hello World This is a test of the scraper.");
    }

    #[test]
    fn test_parse_ddg_html() {
        let html = r#"
            <div class="result">
                <a class="result__a" href="https://example.com/uddg=https%3A%2F%2Fexample.com%2Fpage%26amp%3Bfoo">Example Title</a>
                <span class="result__snippet">This is a snippet.</span>
            </div>
        "#;
        let results = parse_ddg_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "Example Title");
        assert_eq!(results[0]["url"], "https://example.com/page");
        assert_eq!(results[0]["snippet"], "This is a snippet.");
    }
}
