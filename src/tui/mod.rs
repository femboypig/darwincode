pub(crate) mod events;
pub(crate) mod render;
pub(crate) mod syntax;

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

pub(crate) enum WorkerEvent {
    StreamChunk(GeminiResponse),
    StreamDone,
    StreamError(String),
    Models(Result<Vec<String>, String>),
    ToolResult(String, serde_json::Value),
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
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(Into::into)
}

fn stop_terminal(terminal: &mut Tui) -> Result<()> {
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

        if app.screen == crate::app::Screen::Chat && !app.is_busy() && !app.chat.message_queue.is_empty() {
            if let Some(action) = app.pop_and_start_next_queue_item() {
                match action {
                    crate::app::SubmitAction::Generate(request) => {
                        spawn_generation_worker(request.config, request.history, sender.clone());
                    }
                    crate::app::SubmitAction::LoadModels(config) => {
                        spawn_models_worker(config, sender.clone());
                    }
                    crate::app::SubmitAction::ExecuteFunction { name, args } => {
                        handle_function_action(crate::app::FunctionAction::Execute { name, args }, sender);
                    }
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
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_worker_event(app: &mut App, event: WorkerEvent, sender: &Sender<WorkerEvent>) {
    match event {
        WorkerEvent::StreamChunk(chunk) => {
            app.handle_stream_chunk(chunk);
        }
        WorkerEvent::StreamDone => {
            if let Some(action) = app.complete_stream() {
                handle_function_action(action, sender);
            }
        }
        WorkerEvent::StreamError(err) => {
            app.handle_stream_error(err);
        }
        WorkerEvent::Models(result) => app.complete_load_models(result),
        WorkerEvent::ToolResult(name, response) => {
            if let Some(crate::app::FunctionAction::ResumeGeneration(request)) = app.complete_function_execution(name, response) {
                spawn_generation_worker(request.config, request.history, sender.clone());
            }
        }
    }
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
                        match std::process::Command::new("grep").arg("-rnI").arg(pattern).arg(path).output() {
                            Ok(output) => {
                                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
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
                            if let Some(parent) = std::path::Path::new(path).parent() {
                                if !parent.as_os_str().is_empty() {
                                    std::fs::create_dir_all(parent)?;
                                }
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
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_micros())
                            .unwrap_or(0);
                        let temp_out = std::env::temp_dir().join(format!("darwincode_cmd_{}.log", ts));
                        
                        let run_result = (|| -> Result<serde_json::Value, std::io::Error> {
                            let out_file = std::fs::File::create(&temp_out)?;
                            let err_file = out_file.try_clone()?;
                            
                            let mut child = std::process::Command::new("bash")
                                .arg("-c")
                                .arg(cmd)
                                .stdout(out_file)
                                .stderr(err_file)
                                .spawn()?;
                            
                            let status = child.wait()?;
                            let output_content = std::fs::read_to_string(&temp_out).unwrap_or_default();
                            let _ = std::fs::remove_file(&temp_out);
                            
                            Ok(serde_json::json!({
                                "status": status.code(),
                                "stdout": output_content,
                                "stderr": "",
                            }))
                        })();
                        
                        match run_result {
                            Ok(val) => val,
                            Err(e) => {
                                let _ = std::fs::remove_file(&temp_out);
                                serde_json::json!({ "error": e.to_string() })
                            }
                        }
                    }
                    _ => serde_json::json!({ "error": "Unknown function" }),
                };
                let _ = sender.send(WorkerEvent::ToolResult(name, result));
            });
        }
        crate::app::FunctionAction::ResumeGeneration(request) => {
            spawn_generation_worker(request.config, request.history, sender.clone());
        }
    }
}

pub(crate) fn spawn_generation_worker(
    config: StoredConfig,
    history: Vec<crate::gemini::ChatMessage>,
    sender: Sender<WorkerEvent>,
) {
    thread::spawn(move || {
        let sender_clone = sender.clone();
        let result = GeminiClient::new(config).generate_stream(&history, move |chunk| {
            let _ = sender_clone.send(WorkerEvent::StreamChunk(chunk));
            Ok(())
        });
        match result {
            Ok(_) => {
                let _ = sender.send(WorkerEvent::StreamDone);
            }
            Err(error) => {
                let _ = sender.send(WorkerEvent::StreamError(error.to_string()));
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
