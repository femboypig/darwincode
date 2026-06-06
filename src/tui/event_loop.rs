use anyhow::Result;
use crossterm::event::{self, Event};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::app::{App, Screen, SubmitAction};
use crate::tui::events::mouse::handle_mouse_event;
use crate::tui::events::mouse::update_selection_on_scroll;
use crate::tui::terminal::{start_terminal, stop_terminal};
use crate::tui::tool_executor::{
    handle_function_action, spawn_generation_worker, spawn_models_worker,
};
use crate::tui::{Tui, WorkerEvent};

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

fn run_loop(
    terminal: &mut Tui,
    app: &mut App,
    sender: &Sender<WorkerEvent>,
    receiver: &Receiver<WorkerEvent>,
) -> Result<()> {
    while !app.should_quit {
        let ask_user_req = crate::tui::ASK_USER_CHANNEL
            .lock()
            .as_ref()
            .map(|(_, q, opts)| (q.clone(), opts.clone()))
            .filter(|_| app.ui.screen != Screen::AskUser);

        if let Some((question, options)) = ask_user_req {
            app.ui.screen = Screen::AskUser;
            app.ui.ask_user.question = question;
            app.ui.ask_user.options = options;
            app.ui.ask_user.selected_idx = 0;
            app.ui.ask_user.custom_input.clear();
            app.ui.ask_user.is_custom = app.ui.ask_user.options.is_empty();
            app.status = "Answer the question. Enter to submit.".to_owned();
        }

        app.advance_tick();
        while let Ok(event) = receiver.try_recv() {
            handle_worker_event(app, event, sender);
        }

        if app.ui.screen == Screen::Chat
            && !app.is_busy()
            && !app.chat.message_queue.is_empty()
            && let Some(action) = app.pop_and_start_next_queue_item()
        {
            match action {
                SubmitAction::Generate(request) => {
                    spawn_generation_worker(
                        request.config,
                        request.history,
                        request.cancel_token,
                        request.generation_id,
                        request.dev_mode,
                        sender.clone(),
                    );
                }
                SubmitAction::LoadModels(config) => {
                    spawn_models_worker(config, sender.clone());
                }
                SubmitAction::ExecuteFunction { name, args, config } => {
                    handle_function_action(FunctionAction::Execute { name, args, config }, sender);
                }
            }
        }

        // Handle scrolling if dragging outside the viewport vertical boundaries
        if let Some((click_x, click_y)) = app.chat.last_mouse_drag_pos
            && let Some(rect) = app.chat.messages_area.get()
        {
            if click_y < rect.y {
                app.chat.scroll = app.chat.scroll.saturating_add(1);
                update_selection_on_scroll(app, click_x, click_y);
            } else if click_y >= rect.y + rect.height {
                app.chat.scroll = app.chat.scroll.saturating_sub(1);
                update_selection_on_scroll(app, click_x, click_y);
            }
        }

        let _ = terminal.draw(|frame| crate::tui::render::render(frame, app));

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    app.chat.selection = None;
                    if key.kind != event::KeyEventKind::Release {
                        crate::tui::events::handle_key(app, sender, key)?;
                    }
                }
                Event::Mouse(mouse_event) => {
                    handle_mouse_event(app, sender, mouse_event)?;
                }
                Event::Paste(text) => crate::tui::events::handle_paste(app, text),
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
            if id == app.proc.generation_id {
                app.handle_stream_chunk(chunk);
            }
        }
        WorkerEvent::StreamDone(id) if id == app.proc.generation_id => {
            if let Some(action) = app.complete_stream() {
                handle_function_action(action, sender);
            }
        }
        WorkerEvent::StreamDone(_) => {}
        WorkerEvent::StreamError(id, err) => {
            if id == app.proc.generation_id {
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
            if id == app.proc.generation_id {
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

// Re-expose FunctionAction so that it's in scope
use crate::app::FunctionAction;
