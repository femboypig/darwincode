pub(crate) mod async_runtime;
pub(crate) mod event_loop;
pub(crate) mod events;
pub(crate) mod keybindings;
pub(crate) mod render;
pub(crate) mod syntax;
pub(crate) mod terminal;
pub(crate) mod theme;
pub(crate) mod tool_executor;

pub use event_loop::run;
pub use terminal::Tui;
pub(crate) use tool_executor::{
    handle_function_action, spawn_generation_worker, spawn_models_worker,
};

use anyhow::Result;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};

pub static RUNNING_PROCESS_PID: parking_lot::Mutex<Option<u32>> = parking_lot::Mutex::new(None);
pub static RUNNING_PROCESS_STDIN: parking_lot::Mutex<Option<tokio::process::ChildStdin>> =
    parking_lot::Mutex::new(None);
type AskUserChannel = (std::sync::mpsc::Sender<String>, String, Vec<String>);

pub static ASK_USER_CHANNEL: parking_lot::Mutex<Option<AskUserChannel>> =
    parking_lot::Mutex::new(None);

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

#[derive(Clone)]
pub(crate) struct PersistentSession {
    pub(crate) pid: u32,
    pub(crate) child: Arc<Mutex<std::process::Child>>,
    pub(crate) stdin: Arc<Mutex<std::process::ChildStdin>>,
    pub(crate) stdout_accumulator: Arc<Mutex<String>>,
    pub(crate) stderr_accumulator: Arc<Mutex<String>>,
}

pub(crate) static PERSISTENT_SESSIONS: OnceLock<Mutex<HashMap<String, PersistentSession>>> =
    OnceLock::new();
pub(crate) static ACTIVE_PERSISTENT_SESSION_ID: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug)]
pub(crate) enum WorkerEvent {
    StreamChunk(usize, crate::api::GeminiResponse),
    StreamDone(usize),
    StreamError(usize, String),
    Models(Result<Vec<String>, String>),
    ToolResult(String, serde_json::Value),
    ResetStream(usize),
    BashStdout(Option<u32>, String),
    BashStderr(Option<u32>, String),
}

pub(crate) fn register_background_process(
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
            {
                let mut child_guard = child_clone.lock();
                match child_guard.try_wait() {
                    Ok(Some(status)) => {
                        let mut status_guard = exit_status_clone.lock();
                        *status_guard = Some(status.code().unwrap_or(-1));
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        let mut status_guard = exit_status_clone.lock();
                        *status_guard = Some(-1);
                        break;
                    }
                }
            }
        }
    });

    {
        let mut map = registry.lock();
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

pub(crate) fn run_check_process(pid: u32) -> serde_json::Value {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock();
    if let Some(proc) = map.get_mut(&pid) {
        let mut exit_code_guard = proc.exit_status.lock();
        if exit_code_guard.is_none()
            && let Some(mut child_guard) = proc.child.try_lock()
            && let Some(status) = child_guard.try_wait().ok().flatten()
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
}

pub(crate) fn run_kill_process(pid: u32) -> serde_json::Value {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock();
    if let Some(proc) = map.remove(&pid) {
        if let Some(mut c) = proc.child.try_lock() {
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
}

pub(crate) fn run_get_logs(pid: u32, limit: Option<usize>) -> serde_json::Value {
    let registry = BACKGROUND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()));
    let map = registry.lock();
    if let Some(proc) = map.get(&pid) {
        let stdout = proc.stdout_accumulator.lock().clone();
        let stderr = proc.stderr_accumulator.lock().clone();

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

        let mut exit_code_guard = proc.exit_status.lock();
        if exit_code_guard.is_none()
            && let Some(mut child_guard) = proc.child.try_lock()
            && let Some(status) = child_guard.try_wait().ok().flatten()
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
}

#[allow(clippy::zombie_processes)]
pub(crate) fn run_persistent_bash(
    session_id: &str,
    cmd: &str,
    input: Option<&str>,
    sender: Sender<WorkerEvent>,
) -> Result<serde_json::Value, std::io::Error> {
    let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let entry = {
        let mut map = registry.lock();
        map.entry(session_id.to_owned())
            .or_insert_with(|| {
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
                    // SAFETY: setpgid(0,0) puts the child into its own process group
                    // so we can signal the entire group on Ctrl+C. This is safe because
                    // we are in the pre_exec hook (post-fork, pre-exec) and setpgid is
                    // async-signal-safe.
                    unsafe {
                        command.pre_exec(|| {
                            let _ = libc::setpgid(0, 0);
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
                        {
                            let mut guard = stdout_acc_clone.lock();
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
                        {
                            let mut guard = stderr_acc_clone.lock();
                            guard.push_str(&chunk);
                        }
                        let _ = sender_stderr.send(WorkerEvent::BashStderr(Some(pid), chunk));
                    }
                });

                PersistentSession {
                    pid,
                    child: child_arc,
                    stdin: Arc::new(Mutex::new(stdin)),
                    stdout_accumulator: stdout_acc,
                    stderr_accumulator: stderr_acc,
                }
            })
            .clone()
    };

    // Set the active persistent session ID for keystroke forwarding
    {
        let mut guard = ACTIVE_PERSISTENT_SESSION_ID.lock();
        *guard = Some(session_id.to_owned());
    }

    use std::io::Write;

    let start_stdout_len = entry.stdout_accumulator.lock().len();
    let start_stderr_len = entry.stderr_accumulator.lock().len();

    // Use a cryptographically random nonce for the sentinel to prevent
    // a malicious command from guessing/outputting it to fool the parser.
    let nonce = {
        let mut bytes = [0u8; 16];
        rand::fill(&mut bytes);
        bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>()
    };
    let sentinel = format!("__DCEND_{}__", nonce);

    {
        let mut stdin_guard = entry.stdin.lock();
        writeln!(stdin_guard, "{}", cmd)?;
        if let Some(inp) = input {
            writeln!(stdin_guard, "{}", inp)?;
        }
        writeln!(stdin_guard, "echo \"{}\"", sentinel)?;
        let _ = stdin_guard.flush();
    }

    let mut check_count = 0;
    let max_checks = 600; // 600 * 50ms = 30 seconds
    let mut found = false;
    let mut has_exited = false;
    let mut exit_status = None;

    while check_count < max_checks {
        std::thread::sleep(std::time::Duration::from_millis(50));

        if let Some(status) = entry.child.lock().try_wait().ok().flatten() {
            has_exited = true;
            exit_status = Some(status.code().unwrap_or(-1));
            break;
        }

        let stdout_guard = entry.stdout_accumulator.lock();
        if stdout_guard[start_stdout_len..].contains(&sentinel) {
            found = true;
            break;
        }
        check_count += 1;
    }

    if !found && !has_exited {
        let mut child_guard = entry.child.lock();
        let _ = child_guard.kill();
        drop(child_guard);

        let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        {
            let mut map = registry.lock();
            map.remove(session_id);
        }

        *ACTIVE_PERSISTENT_SESSION_ID.lock() = None;

        return Ok(serde_json::json!({
            "status": -1,
            "stdout": "",
            "stderr": "",
            "error": "Command timed out and shell process was terminated"
        }));
    }

    *ACTIVE_PERSISTENT_SESSION_ID.lock() = None;

    if has_exited {
        let registry = PERSISTENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        {
            let mut map = registry.lock();
            map.remove(session_id);
        }
    }

    let raw_stdout = entry.stdout_accumulator.lock();
    let raw_stderr = entry.stderr_accumulator.lock();

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
