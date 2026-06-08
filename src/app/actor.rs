use crate::app::{App, Screen, SubmitAction};
use crate::tui::Tui;
use crate::tui::WorkerEvent;
use crossterm::event::{KeyEvent, MouseEvent};
use tokio::sync::mpsc;
use anyhow::Result;

#[derive(Debug)]
pub enum AppCommand {
    KeyEvent(KeyEvent),
    MouseEvent(MouseEvent),
    Paste(String),
    Resize,
    Worker(WorkerEvent),
    Tick,
}

pub struct AppActor {
    rx: mpsc::UnboundedReceiver<AppCommand>,
    tx: mpsc::UnboundedSender<AppCommand>,
    pub app: App,
}

impl AppActor {
    pub fn new(app: App) -> (Self, mpsc::UnboundedSender<AppCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { rx, tx: tx.clone(), app }, tx)
    }

    pub async fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let (worker_tx, worker_rx) = std::sync::mpsc::channel::<WorkerEvent>();
        let tx_clone = self.tx.clone();
        
        // Forward WorkerEvent from blocking worker threads to our async channel
        tokio::task::spawn_blocking(move || {
            while let Ok(event) = worker_rx.recv() {
                if tx_clone.send(AppCommand::Worker(event)).is_err() {
                    break;
                }
            }
        });

        while !self.app.should_quit {
            // Draw TUI
            let _ = terminal.draw(|frame| crate::tui::render::render(frame, &mut self.app));

            // Wait for next command
            if let Some(cmd) = self.rx.recv().await {
                self.handle_command(cmd, &worker_tx)?;
            }
        }
        Ok(())
    }

    fn handle_command(&mut self, cmd: AppCommand, worker_tx: &std::sync::mpsc::Sender<WorkerEvent>) -> Result<()> {
        // Auto-detect prompt requests
        let ask_user_req = crate::tui::ASK_USER_CHANNEL
            .lock()
            .as_ref()
            .map(|(_, q, opts)| (q.clone(), opts.clone()))
            .filter(|_| self.app.ui.screen != Screen::AskUser);

        if let Some((question, options)) = ask_user_req {
            self.app.ui.screen = Screen::AskUser;
            self.app.ui.ask_user.question = question;
            self.app.ui.ask_user.options = options;
            self.app.ui.ask_user.selected_idx = 0;
            self.app.ui.ask_user.custom_input.clear();
            self.app.ui.ask_user.is_custom = self.app.ui.ask_user.options.is_empty();
            self.app.status = "Answer the question. Enter to submit.".to_owned();
        }

        match cmd {
            AppCommand::KeyEvent(key) => {
                self.app.chat.selection = None;
                crate::tui::events::handle_key(&mut self.app, worker_tx, key)?;
            }
            AppCommand::MouseEvent(mouse_event) => {
                crate::tui::events::mouse::handle_mouse_event(&mut self.app, worker_tx, mouse_event)?;
            }
            AppCommand::Paste(text) => {
                crate::tui::events::handle_paste(&mut self.app, text);
            }
            AppCommand::Resize => {
                // Resize event
            }
            AppCommand::Worker(worker_event) => {
                self.handle_worker_event(worker_event, worker_tx);
            }
            AppCommand::Tick => {
                self.app.advance_tick();

                // Process background message queue if any
                if self.app.ui.screen == Screen::Chat
                    && !self.app.is_busy()
                    && !self.app.chat.message_queue.is_empty()
                    && let Some(action) = self.app.pop_and_start_next_queue_item()
                {
                    self.execute_submit_action(action, worker_tx);
                }

                // Handle scrolling if dragging outside viewport
                if !self.app.ui.show_trust_modal
                    && let Some((click_x, click_y)) = self.app.chat.last_mouse_drag_pos
                    && let Some(rect) = self.app.chat.messages_area.get()
                {
                    if click_y < rect.y {
                        self.app.chat.scroll.set(self.app.chat.scroll.get().saturating_add(1));
                        crate::tui::events::mouse::update_selection_on_scroll(&mut self.app, click_x, click_y);
                    } else if click_y >= rect.y + rect.height {
                        self.app.chat.scroll.set(self.app.chat.scroll.get().saturating_sub(1));
                        crate::tui::events::mouse::update_selection_on_scroll(&mut self.app, click_x, click_y);
                    }
                }
            }
        }
        Ok(())
    }

    fn execute_submit_action(&mut self, action: SubmitAction, worker_tx: &std::sync::mpsc::Sender<WorkerEvent>) {
        match action {
            SubmitAction::Generate(request) => {
                crate::tui::tool_executor::spawn_generation_worker(
                    request.config,
                    request.history,
                    request.cancel_token,
                    request.generation_id,
                    request.dev_mode,
                    worker_tx.clone(),
                );
            }
            SubmitAction::LoadModels(config) => {
                crate::tui::tool_executor::spawn_models_worker(config, worker_tx.clone());
            }
            SubmitAction::ExecuteFunction { name, args, config } => {
                crate::tui::tool_executor::handle_function_action(
                    crate::app::FunctionAction::Execute { name, args, config },
                    worker_tx,
                );
            }
        }
    }

    fn handle_worker_event(&mut self, event: WorkerEvent, worker_tx: &std::sync::mpsc::Sender<WorkerEvent>) {
        match event {
            WorkerEvent::StreamChunk(id, chunk) => {
                if id == self.app.proc.generation_id {
                    self.app.handle_stream_chunk(chunk);
                }
            }
            WorkerEvent::StreamDone(id) if id == self.app.proc.generation_id => {
                if let Some(action) = self.app.complete_stream() {
                    crate::tui::tool_executor::handle_function_action(action, worker_tx);
                }
            }
            WorkerEvent::StreamDone(_) => {}
            WorkerEvent::StreamError(id, err) => {
                if id == self.app.proc.generation_id {
                    self.app.handle_stream_error(err);
                }
            }
            WorkerEvent::Models(result) => self.app.complete_load_models(result),
            WorkerEvent::ToolResult(name, response) => {
                if let Some(crate::app::FunctionAction::ResumeGeneration(request)) =
                    self.app.complete_function_execution(name, response)
                {
                    crate::tui::tool_executor::spawn_generation_worker(
                        request.config,
                        request.history,
                        request.cancel_token,
                        request.generation_id,
                        request.dev_mode,
                        worker_tx.clone(),
                    );
                }
            }
            WorkerEvent::ResetStream(id) => {
                if id == self.app.proc.generation_id {
                    self.app.chat.streaming_parts.clear();
                }
            }
            WorkerEvent::BashStdout(pid, chunk) => {
                self.app.handle_bash_stdout(pid, chunk);
            }
            WorkerEvent::BashStderr(pid, chunk) => {
                self.app.handle_bash_stderr(pid, chunk);
            }
        }
    }
}
