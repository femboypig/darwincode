pub mod chat;
pub mod model;
pub mod permission;
pub mod session;
pub mod setup;

pub use chat::{ChatCommand, ChatState, CommandSuggestion, MessageLine};
pub use model::ModelPickerState;
pub use permission::PermissionPickerState;
pub use session::SessionPickerState;
pub use setup::{SetupField, SetupState};

use crate::api::{ChatMessage, GeminiResponse};
use crate::config::{PermissionLevel, StoredConfig};
use anyhow::Result;

#[derive(Clone, Debug)]
pub enum FunctionAction {
    Execute {
        name: String,
        args: serde_json::Value,
        config: StoredConfig,
    },
    ResumeGeneration(GenerationRequest),
}

#[derive(Clone, Debug)]
pub struct GenerationRequest {
    pub config: StoredConfig,
    pub history: Vec<ChatMessage>,
    pub cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub generation_id: usize,
}

#[derive(Clone, Debug)]
pub enum SubmitAction {
    Generate(GenerationRequest),
    LoadModels(StoredConfig),
    ExecuteFunction {
        name: String,
        args: serde_json::Value,
        config: StoredConfig,
    },
}

#[derive(Clone, Debug)]
pub enum PendingTask {
    Generating,
    LoadingModels,
    ConfirmFunction {
        name: String,
        args: serde_json::Value,
    },
    ExecutingFunction {
        name: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Setup,
    Chat,
    Models,
    Permissions,
    Sessions,
    AskUser,
}

#[derive(Clone, Debug)]
pub struct AskUserState {
    pub question: String,
    pub options: Vec<String>,
    pub selected_idx: usize,
    pub custom_input: String,
    pub is_custom: bool,
}

#[derive(Clone, Debug)]
pub struct FileBackup {
    pub path: String,
    pub original_content: Option<String>,
}

#[derive(Debug)]
pub struct App {
    pub screen: Screen,
    pub setup: SetupState,
    pub chat: ChatState,
    pub models: ModelPickerState,
    pub permissions: PermissionPickerState,
    pub sessions: SessionPickerState,
    pub status: String,
    pub pending: Option<PendingTask>,
    pub tick: usize,
    pub should_quit: bool,
    pub keybindings: crate::tui::keybindings::KeyBindings,
    pub cancel_token: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub last_file_backups: Vec<FileBackup>,
    pub generation_id: usize,
    pub confirm_scroll: u16,
    pub ask_user: AskUserState,
    pub sessions_cache: std::cell::RefCell<Option<Vec<crate::app::session::SessionMeta>>>,
}

impl App {
    pub fn new(config: Option<StoredConfig>) -> Self {
        let keybindings = crate::tui::keybindings::load_keybindings();
        match config {
            Some(config) => Self {
                screen: Screen::Chat,
                setup: SetupState::default(),
                chat: ChatState::new(config),
                models: ModelPickerState::default(),
                permissions: PermissionPickerState::default(),
                sessions: SessionPickerState::default(),
                status: "Ready".to_owned(),
                pending: None,
                tick: 0,
                should_quit: false,
                keybindings,
                cancel_token: None,
                last_file_backups: Vec::new(),
                generation_id: 0,
                confirm_scroll: 0,
                ask_user: AskUserState {
                    question: String::new(),
                    options: Vec::new(),
                    selected_idx: 0,
                    custom_input: String::new(),
                    is_custom: false,
                },
                sessions_cache: std::cell::RefCell::new(None),
            },
            None => Self {
                screen: Screen::Setup,
                setup: SetupState::default(),
                chat: ChatState::new(StoredConfig::default()),
                models: ModelPickerState::default(),
                permissions: PermissionPickerState::default(),
                sessions: SessionPickerState::default(),
                status: "Enter a Gemini API key. Use Tab to move, Enter to run an action."
                    .to_owned(),
                pending: None,
                tick: 0,
                should_quit: false,
                keybindings,
                cancel_token: None,
                last_file_backups: Vec::new(),
                generation_id: 0,
                confirm_scroll: 0,
                ask_user: AskUserState {
                    question: String::new(),
                    options: Vec::new(),
                    selected_idx: 0,
                    custom_input: String::new(),
                    is_custom: false,
                },
                sessions_cache: std::cell::RefCell::new(None),
            },
        }
    }

    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn save_setup(&mut self) -> Result<()> {
        if self.is_busy() {
            return Ok(());
        }

        let config = self.setup.to_config()?;
        config.save()?;
        self.chat.config = config;
        self.screen = Screen::Chat;
        self.status = "Settings saved".to_owned();
        Ok(())
    }

    pub fn begin_load_chat_models(&mut self) -> Option<StoredConfig> {
        if self.is_busy() {
            return None;
        }

        self.pending = Some(PendingTask::LoadingModels);
        self.status = "Loading models".to_owned();
        Some(self.chat.config.clone())
    }

    pub fn complete_load_models(&mut self, result: Result<Vec<String>, String>) {
        self.pending = None;

        let models = match result {
            Ok(models) => models,
            Err(error) => {
                self.status = error;
                return;
            }
        };

        if self.screen == Screen::Chat {
            let count = models.len();
            self.models = ModelPickerState::new(models, &self.chat.config.model);
            self.screen = Screen::Models;
            self.status = format!("Loaded {count} models");
            return;
        }

        self.setup.models = models;
        self.setup.selected_model = 0;

        if let Some(model) = self.setup.models.first() {
            self.setup.model = model.trim_start_matches("models/").to_owned();
        }

        self.status = format!("Loaded {} models", self.setup.models.len());
    }

    pub fn submit_chat_input(&mut self) -> Option<SubmitAction> {
        let input = self.chat.input.trim().to_owned();
        if input.is_empty() {
            return None;
        }
        self.chat.sent_history_index = None;
        self.chat.input_draft.clear();

        // Special case: If user enters exit or permissions command, execute it immediately even if busy!
        if let Some(command) = ChatCommand::parse(&input)
            && matches!(command, ChatCommand::Exit | ChatCommand::Permissions(_))
        {
            self.chat.input.clear();
            self.chat.cursor = 0;
            self.chat.input_scroll = 0;
            self.chat.scroll = 0;
            return self.run_command(command);
        }

        if self.is_busy() {
            self.chat.input.clear();
            self.chat.cursor = 0;
            self.chat.input_scroll = 0;
            self.chat.scroll = 0;

            // Queue prompt in memory only (rendered in gray below the messages box)
            self.chat.message_queue.push(input);
            return None;
        }

        self.chat.input.clear();
        self.chat.cursor = 0;
        self.chat.input_scroll = 0;
        self.chat.scroll = 0;

        if let Some(command) = ChatCommand::parse(&input) {
            return self.run_command(command);
        }

        self.last_file_backups.clear();
        self.chat.history.push(ChatMessage::user(input.clone()));
        let _ = session::save_session(&self.chat);
        self.chat.messages.push(MessageLine::user(input));
        self.chat.messages.push(MessageLine::pending());
        self.pending = Some(PendingTask::Generating);
        self.status = "Working...".to_owned();

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancel_token = Some(cancel_token.clone());
        self.generation_id += 1;
        let generation_id = self.generation_id;

        Some(SubmitAction::Generate(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
            cancel_token,
            generation_id,
        }))
    }

    pub fn pop_and_start_next_queue_item(&mut self) -> Option<SubmitAction> {
        if self.is_busy() || self.chat.message_queue.is_empty() {
            return None;
        }

        let input = self.chat.message_queue.remove(0);

        if let Some(command) = ChatCommand::parse(&input) {
            return self.run_command(command);
        }

        self.last_file_backups.clear();
        self.chat.history.push(ChatMessage::user(input.clone()));
        let _ = session::save_session(&self.chat);
        self.chat.messages.push(MessageLine::user(input));
        self.chat.messages.push(MessageLine::pending());
        self.pending = Some(PendingTask::Generating);

        let queue_len = self.chat.message_queue.len();
        if queue_len > 0 {
            self.status = format!("Working... ({} in queue)", queue_len);
        } else {
            self.status = "Working...".to_owned();
        }

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancel_token = Some(cancel_token.clone());
        self.generation_id += 1;
        let generation_id = self.generation_id;

        Some(SubmitAction::Generate(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
            cancel_token,
            generation_id,
        }))
    }

    pub fn handle_bash_stdout(&mut self, chunk: String) {
        if let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_shell) {
            if msg.text.ends_with("\nRunning...\n") {
                msg.text.truncate(msg.text.len() - 11);
            }
            msg.text.push_str(&chunk);
            *msg.cached_wrapped.borrow_mut() = None;
        }
    }

    pub fn handle_bash_stderr(&mut self, chunk: String) {
        if let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_shell) {
            if msg.text.ends_with("\nRunning...\n") {
                msg.text.truncate(msg.text.len() - 11);
            }
            msg.text.push_str(&chunk);
            *msg.cached_wrapped.borrow_mut() = None;
        }
    }

    pub fn handle_stream_chunk(&mut self, response: GeminiResponse) {
        if !matches!(self.pending, Some(PendingTask::Generating)) {
            return;
        }
        let GeminiResponse::Turn(parts) = response;
        let show_thoughts = self.chat.config.show_thoughts;

        // If the last message is pending, we remove it and start appending to a new message
        if self.chat.messages.last().is_some_and(|m| m.pending) {
            self.chat.messages.pop();
        }

        for part in parts {
            self.chat.streaming_parts.push(part.clone());

            let is_thought = part
                .get("thought")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || part.get("thought_signature").is_some();

            if is_thought {
                if let Some(text) = part.get("text").and_then(|v| v.as_str())
                    && !text.is_empty()
                {
                    if show_thoughts {
                        let last_is_thinking = self.chat.messages.last().is_some_and(|m| {
                            m.author == "Darwin"
                                && !m.pending
                                && !m.is_shell
                                && !m.is_tool
                                && m.text.starts_with("Thinking:")
                        });
                        if last_is_thinking {
                            self.append_to_chat_messages("Darwin", text.to_owned());
                        } else {
                            self.append_to_chat_messages("Darwin", format!("Thinking: {}", text));
                        }
                    } else {
                        if self
                            .chat
                            .messages
                            .last()
                            .is_none_or(|m| m.text != "Thinking...")
                        {
                            self.chat
                                .messages
                                .push(MessageLine::assistant("Thinking...".to_owned()));
                        }
                    }
                }
                self.chat.last_chunk_was_thought = true;
                continue;
            }

            if let Some(text) = part.get("text").and_then(|v| v.as_str())
                && !text.is_empty()
            {
                if self.chat.last_chunk_was_thought {
                    if show_thoughts {
                        let clean_text = text.trim_start_matches('\n').trim_start_matches('\r');
                        self.append_to_chat_messages("Darwin", format!("\n\n{}", clean_text));
                    } else {
                        if let Some(msg) = self.chat.messages.last_mut()
                            && msg.author == "Darwin"
                            && msg.text == "Thinking..."
                        {
                            msg.text = text
                                .trim_start_matches('\n')
                                .trim_start_matches('\r')
                                .to_owned();
                            *msg.cached_wrapped.borrow_mut() = None;
                        } else {
                            self.append_to_chat_messages("Darwin", text.to_owned());
                        }
                    }
                } else {
                    self.append_to_chat_messages("Darwin", text.to_owned());
                }
                self.chat.last_chunk_was_thought = false;
            }
        }

        self.chat.scroll = 0;
    }

    fn append_to_chat_messages(&mut self, author: &'static str, text: String) {
        if let Some(msg) = self.chat.messages.last_mut()
            && msg.author == author
            && !msg.pending
            && !msg.is_shell
            && !msg.is_tool
        {
            msg.text.push_str(&text);
            *msg.cached_wrapped.borrow_mut() = None;
            return;
        }

        self.chat.messages.push(MessageLine {
            author,
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        });
    }

    pub fn handle_stream_error(&mut self, error: String) {
        if let Some(m) = self.chat.messages.last_mut()
            && m.pending
        {
            self.chat.messages.pop();
        }
        self.chat.messages.push(MessageLine::error(error));
        self.chat.streaming_parts.clear();
        self.chat.scroll = 0;
        self.pending = None;
        self.status = "Ready".to_owned();
        let _ = session::save_session(&self.chat);
    }

    pub fn complete_stream(&mut self) -> Option<FunctionAction> {
        if !matches!(self.pending, Some(PendingTask::Generating)) {
            self.chat.streaming_parts.clear();
            return None;
        }
        let parts = std::mem::take(&mut self.chat.streaming_parts);
        if parts.is_empty() {
            self.pending = None;
            self.status = "Ready".to_owned();
            if self.chat.messages.last().is_some_and(|m| m.pending) {
                self.chat.messages.pop();
            }
            self.chat.messages.push(MessageLine::error(
                "Error: API returned an empty response".to_owned(),
            ));
            return None;
        }

        self.chat.history.push(ChatMessage {
            role: "model".to_owned(),
            parts: parts.clone(),
        });
        let _ = session::save_session(&self.chat);

        let mut first_call = None;
        for part in &parts {
            if let Some(call) = part.get("functionCall")
                && first_call.is_none()
                && let (Some(name), Some(args)) =
                    (call.get("name").and_then(|v| v.as_str()), call.get("args"))
            {
                first_call = Some((name.to_owned(), args.clone()));
            }
        }

        if let Some((name, args)) = first_call {
            let permission = self.chat.config.permission_level;
            let auto_allowed = permission == PermissionLevel::Chaos
                || (permission == PermissionLevel::Safe
                    && (name == "read_file"
                        || name == "list_directory"
                        || name == "search_files"
                        || name == "read_files"));

            if auto_allowed {
                self.backup_before_execution(&name, &args);
                self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                self.status = format!("Auto-executing {name}");
                self.start_function_execution(&name, &args);
                Some(FunctionAction::Execute {
                    name,
                    args,
                    config: self.chat.config.clone(),
                })
            } else if permission == PermissionLevel::Safe
                && (name == "run_bash_command"
                    || name == "edit_file"
                    || name == "write_file"
                    || name == "edit_files")
            {
                self.pending = Some(PendingTask::Generating);
                self.complete_function_execution(
                    name,
                    serde_json::json!({"error": "Permission denied: restricted mode"}),
                )
            } else {
                self.confirm_scroll = 0;
                self.pending = Some(PendingTask::ConfirmFunction { name, args });
                self.status = "Action required".to_owned();
                None
            }
        } else {
            self.pending = None;
            self.status = "Ready".to_owned();
            None
        }
    }

    pub fn open_setup(&mut self) {
        if self.is_busy() {
            return;
        }

        self.setup = SetupState::from_config(&self.chat.config);
        self.screen = Screen::Setup;
        self.status = "Edit settings. Press Enter to save or Esc to quit.".to_owned();
    }

    pub fn open_sessions(&mut self) {
        if self.is_busy() {
            return;
        }
        match crate::app::session::list_saved_sessions() {
            Ok(list) => {
                self.sessions.sessions = list;
                self.sessions.selected = 0;
                self.sessions.query = String::new();
                self.screen = Screen::Sessions;
                self.status =
                    "Select a session to resume. Enter to apply, Esc to cancel.".to_owned();
            }
            Err(e) => {
                self.chat.messages.push(MessageLine::error(format!(
                    "Failed to list sessions: {}",
                    e
                )));
            }
        }
    }

    pub fn cancel_setup(&mut self) {
        if self.is_busy() {
            return;
        }

        if self.chat.config.api_key.trim().is_empty() {
            self.should_quit = true;
            return;
        }

        self.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn busy_label(&self) -> Option<String> {
        let task = self.pending.as_ref()?;
        let frames = ["", ".", "..", "..."];
        let frame = frames[self.tick / 4 % frames.len()];
        let label = match task {
            PendingTask::Generating => "Working",
            PendingTask::LoadingModels => "Loading models",
            PendingTask::ConfirmFunction { .. } => "Awaiting confirmation",
            PendingTask::ExecutingFunction { name } => {
                return Some(format!("Running {name}{frame}"));
            }
        };

        Some(format!("{label}{frame}"))
    }

    pub fn command_suggestions(&self) -> Vec<CommandSuggestion> {
        let input = self.chat.input.trim_start();
        if !input.starts_with("/resume") {
            *self.sessions_cache.borrow_mut() = None;
        }

        if !input.starts_with('/') {
            return Vec::new();
        }

        if input.starts_with("/permissions ") || input == "/permissions" {
            let options = vec![
                ("/permissions safe", "Read-only access"),
                ("/permissions guardian", "Ask for every action"),
                ("/permissions chaos", "Auto-execute everything"),
            ];

            return options
                .into_iter()
                .filter(|(name, _)| name.starts_with(input))
                .map(|(name, desc)| CommandSuggestion {
                    name: name.to_owned(),
                    description: desc.to_owned(),
                })
                .collect();
        }

        if input.starts_with("/resume ") || input == "/resume" {
            let mut cache = self.sessions_cache.borrow_mut();
            if cache.is_none()
                && let Ok(sessions) = session::list_saved_sessions()
            {
                *cache = Some(sessions);
            }
            if let Some(sessions) = cache.as_ref() {
                return sessions
                    .iter()
                    .map(|meta| CommandSuggestion {
                        name: format!("/resume {}", meta.id),
                        description: meta.snippet.clone(),
                    })
                    .filter(|s| s.name.starts_with(input))
                    .collect();
            }
        }

        if input.contains(' ') {
            return Vec::new();
        }

        let typed = input.trim_start_matches('/');
        ChatCommand::suggestions()
            .into_iter()
            .filter(|suggestion| suggestion.name.trim_start_matches('/').starts_with(typed))
            .collect()
    }

    pub fn accept_command_suggestion(&mut self) {
        if let Some(suggestion) = self.command_suggestions().into_iter().next() {
            self.chat.input = format!("{} ", suggestion.name);
            self.chat.cursor = self.chat.input.chars().count();
        }
    }

    fn run_command(&mut self, command: ChatCommand) -> Option<SubmitAction> {
        match command {
            ChatCommand::Settings => {
                self.open_setup();
                None
            }
            ChatCommand::Exit => {
                self.should_quit = true;
                None
            }
            ChatCommand::Models => self.begin_load_chat_models().map(SubmitAction::LoadModels),
            ChatCommand::Permissions(level) => {
                if let Some(level) = level {
                    self.chat.config.permission_level = level;
                    let level_label = self.chat.config.permission_level.label();
                    if self.chat.messages.last().is_some_and(|m| m.pending) {
                        self.chat.messages.pop();
                    }
                    self.chat.messages.push(MessageLine::info(format!(
                        "Permission level set to **{level_label}**"
                    )));
                    let _ = self.chat.config.save();

                    if let Some(PendingTask::ConfirmFunction { name, args }) = self.pending.clone()
                    {
                        let auto_allowed = level == PermissionLevel::Chaos
                            || (level == PermissionLevel::Safe
                                && (name == "read_file"
                                    || name == "list_directory"
                                    || name == "search_files"
                                    || name == "read_files"));
                        if auto_allowed {
                            self.backup_before_execution(&name, &args);
                            self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                            self.status = format!("Auto-executing {name}");
                            return Some(SubmitAction::ExecuteFunction { name, args, config: self.chat.config.clone() });
                        } else if level == PermissionLevel::Safe && (name == "run_bash_command" || name == "edit_file" || name == "write_file" || name == "edit_files")
                            && let Some(crate::app::FunctionAction::ResumeGeneration(request)) = self.complete_function_execution(name, serde_json::json!({"error": "Permission denied: restricted mode"})) {
                                return Some(SubmitAction::Generate(request));
                            }
                    }
                    None
                } else {
                    self.screen = Screen::Permissions;
                    let current = self.chat.config.permission_level;
                    self.permissions.selected = PermissionPickerState::options()
                        .iter()
                        .position(|(_, _, l)| *l == current)
                        .unwrap_or(0);
                    self.status =
                        "Select permission level. Enter to apply, Esc to cancel.".to_owned();
                    None
                }
            }
            ChatCommand::Resume(session_id) => {
                if let Some(id) = session_id {
                    match session::load_session(&id) {
                        Ok(s) => {
                            self.chat.session_id = s.id;
                            self.chat.history = s.history;
                            self.chat.messages =
                                session::rebuild_messages_from_history(&self.chat.history);
                            self.status = format!("Resumed session: {}", id);
                        }
                        Err(e) => {
                            self.chat.messages.push(MessageLine::error(format!(
                                "Failed to load session '{}': {}",
                                id, e
                            )));
                        }
                    }
                } else {
                    self.open_sessions();
                }
                None
            }
            ChatCommand::Clear => {
                self.chat.history.clear();
                self.chat.messages.clear();
                self.chat.scroll = 0;
                self.chat.message_queue.clear();
                self.chat.sent_history_index = None;
                self.chat.input_draft.clear();
                let _ = session::save_session(&self.chat);
                self.status = "Chat history cleared".to_owned();
                None
            }
            ChatCommand::New => {
                self.chat.history.clear();
                self.chat.messages.clear();
                self.chat.scroll = 0;
                self.chat.message_queue.clear();
                self.chat.sent_history_index = None;
                self.chat.input_draft.clear();
                self.chat.session_id = format!(
                    "session_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                );
                let _ = session::save_session(&self.chat);
                self.status = "New chat started".to_owned();
                None
            }
            ChatCommand::History => {
                match session::list_saved_sessions() {
                    Ok(list) => {
                        if list.is_empty() {
                            self.chat
                                .messages
                                .push(MessageLine::info("No saved sessions found.".to_owned()));
                        } else {
                            let mut msg = "Saved sessions:\n".to_owned();
                            for meta in list {
                                msg.push_str(&format!("- **{}**: {}\n", meta.id, meta.snippet));
                            }
                            self.chat.messages.push(MessageLine::info(msg));
                        }
                    }
                    Err(e) => {
                        self.chat.messages.push(MessageLine::error(format!(
                            "Failed to list sessions: {}",
                            e
                        )));
                    }
                }
                None
            }
            ChatCommand::Undo => {
                if self.last_file_backups.is_empty() {
                    self.chat.messages.push(MessageLine::info(
                        "No changes to undo from the last prompt.".to_owned(),
                    ));
                } else {
                    let undone: Vec<String> = self
                        .last_file_backups
                        .iter()
                        .map(|b| {
                            if b.original_content.is_some() {
                                format!("reverted `{}`", b.path)
                            } else {
                                format!("deleted new file `{}`", b.path)
                            }
                        })
                        .collect();
                    self.rollback_transactions();
                    self.chat.messages.push(MessageLine::info(format!(
                        "Undo completed successfully: {}",
                        undone.join(", ")
                    )));
                }
                None
            }

            ChatCommand::Help => {
                let help_text = "Available commands:\n\
                                 - **/settings**: Open configuration settings\n\
                                 - **/models**: List and select Gemini/OpenAI models\n\
                                 - **/permissions [safe|guardian|chaos]**: View or set permission level\n\
                                 - **/resume [session_id]**: Load a saved session (or open selector)\n\
                                 - **/new**: Start a new chat session\n\
                                 - **/clear**: Clear the current chat history\n\
                                 - **/history**: Show all saved chat session IDs\n\
                                 - **/undo**: Revert all file changes made in the last prompt\n\
                                 - **/help**: Display this help card\n\
                                 - **/exit** / **/quit**: Exit the application\n\n\
                                 Hotkeys (in Chat):\n\
                                 - **Ctrl+S**: Open Setup screen\n\
                                 - **Ctrl+P**: Switch active Model instantly";
                self.chat
                    .messages
                    .push(MessageLine::info(help_text.to_owned()));
                None
            }

            ChatCommand::Unknown(command) => {
                self.chat.messages.push(MessageLine::info(format!(
                    "Unknown command: {command}\nTry /settings, /models, /permissions, or /exit."
                )));
                self.status = "Unknown command".to_owned();
                None
            }
        }
    }

    pub fn cancel_models(&mut self) {
        self.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn select_next_model(&mut self) {
        self.models.select_next();
    }

    pub fn select_previous_model(&mut self) {
        self.models.select_previous();
    }

    pub fn apply_selected_model(&mut self) {
        let Some(model) = self.models.selected_model() else {
            self.status = "No model selected".to_owned();
            return;
        };

        self.chat.config.model = model.trim_start_matches("models/").to_owned();
        match self.chat.config.save() {
            Ok(()) => {
                self.status = format!("Model set to {}", self.chat.config.model);
                self.screen = Screen::Chat;
            }
            Err(error) => {
                self.status = error.to_string();
            }
        }
    }

    pub fn apply_permission_level(&mut self) -> Option<SubmitAction> {
        let options = PermissionPickerState::options();
        let mut ret = None;
        if let Some((label, _, level)) = options.get(self.permissions.selected) {
            self.chat.config.permission_level = *level;
            if self.chat.messages.last().is_some_and(|m| m.pending) {
                self.chat.messages.pop();
            }
            self.chat.messages.push(MessageLine::info(format!(
                "Permission level set to **{label}**"
            )));
            let _ = self.chat.config.save();

            if let Some(PendingTask::ConfirmFunction { name, args }) = self.pending.clone() {
                let auto_allowed = *level == PermissionLevel::Chaos
                    || (*level == PermissionLevel::Safe
                        && (name == "read_file"
                            || name == "list_directory"
                            || name == "search_files"
                            || name == "read_files"));
                if auto_allowed {
                    self.backup_before_execution(&name, &args);
                    self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                    self.status = format!("Auto-executing {name}");
                    self.start_function_execution(&name, &args);
                    ret = Some(SubmitAction::ExecuteFunction {
                        name,
                        args,
                        config: self.chat.config.clone(),
                    });
                } else if *level == PermissionLevel::Safe
                    && (name == "run_bash_command"
                        || name == "edit_file"
                        || name == "write_file"
                        || name == "edit_files")
                    && let Some(crate::app::FunctionAction::ResumeGeneration(request)) = self
                        .complete_function_execution(
                            name,
                            serde_json::json!({"error": "Permission denied: restricted mode"}),
                        )
                {
                    ret = Some(SubmitAction::Generate(request));
                }
            }
        }
        self.screen = Screen::Chat;
        self.status = "Ready".to_owned();
        ret
    }

    pub fn cancel_permissions(&mut self) {
        self.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn cancel_sessions(&mut self) {
        self.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn apply_selected_session(&mut self) {
        if let Some(meta) = self.sessions.selected_session() {
            match session::load_session(&meta.id) {
                Ok(s) => {
                    self.chat.session_id = s.id;
                    self.chat.history = s.history;
                    self.chat.messages = session::rebuild_messages_from_history(&self.chat.history);
                    self.screen = Screen::Chat;
                    self.status = format!("Resumed session: {}", self.chat.session_id);
                }
                Err(e) => {
                    self.status = format!("Failed to load session: {e}");
                }
            }
        } else {
            self.screen = Screen::Chat;
            self.status = "Ready".to_owned();
        }
    }

    pub fn answer_function_confirmation(&mut self, allow: bool) -> Option<FunctionAction> {
        let PendingTask::ConfirmFunction { name, args } = self.pending.take()? else {
            return None;
        };

        if !allow {
            self.chat.messages.push(MessageLine::info(
                "Tool execution denied. Generation stopped.".to_owned(),
            ));
            self.chat.message_queue.clear();
            self.pending = None;
            self.status = "Ready".to_owned();
            if self.chat.messages.last().is_some_and(|m| m.pending) {
                self.chat.messages.pop();
            }
            let _ = crate::app::session::save_session(&self.chat);
            return None;
        }

        self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
        self.status = format!("Executing {name}");
        self.start_function_execution(&name, &args);
        Some(FunctionAction::Execute {
            name,
            args,
            config: self.chat.config.clone(),
        })
    }

    pub fn complete_function_execution(
        &mut self,
        name: String,
        response: serde_json::Value,
    ) -> Option<FunctionAction> {
        self.chat.scroll = 0;
        let args = if let Some(ChatMessage { parts, .. }) =
            self.chat.history.iter().rev().find(|m| m.role == "model")
        {
            parts
                .iter()
                .find_map(|p| {
                    if let Some(call) = p.get("functionCall")
                        && call.get("name").and_then(|v| v.as_str()) == Some(&name)
                    {
                        return call.get("args").cloned();
                    }
                    None
                })
                .unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        self.chat.history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![serde_json::json!({
                "functionResponse": {
                    "name": name.clone(),
                    "response": response.clone(),
                }
            })],
        });
        let _ = session::save_session(&self.chat);
        if name == "run_bash_command" {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let mut output = String::new();
            let mut success = true;

            let mut is_aborted = false;
            if let Some(status) = response.get("status").and_then(|v| v.as_i64()) {
                if status != 0 {
                    success = false;
                }
            } else {
                success = false;
                is_aborted = true;
            }

            let stdout = response
                .get("stdout")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let stderr = response
                .get("stderr")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !stdout.is_empty() {
                output.push_str(stdout);
            }
            if !stderr.is_empty() {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(stderr);
            }

            if is_aborted {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str("^C\n[Process terminated by user via Ctrl+C]");
            } else if output.is_empty() {
                output = "(empty output)".to_owned();
            }

            let body = format!("$ {}\n{}", cmd, output);

            if let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_shell) {
                msg.text = body;
                msg.shell_success = success;
                *msg.cached_wrapped.borrow_mut() = None;
            }
        } else {
            let summary = crate::app::session::format_tool_summary(&name, &args, &response);
            if let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_tool) {
                msg.text = summary;
                *msg.cached_wrapped.borrow_mut() = None;
            }
        }

        self.chat.messages.push(MessageLine::pending());
        self.pending = Some(PendingTask::Generating);
        self.status = "Working...".to_owned();

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancel_token = Some(cancel_token.clone());
        self.generation_id += 1;
        let generation_id = self.generation_id;

        Some(FunctionAction::ResumeGeneration(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
            cancel_token,
            generation_id,
        }))
    }

    pub fn cancel_generation(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            token.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.pending = None;
        self.status = "Generation stopped".to_owned();
        if self.chat.messages.last().is_some_and(|m| m.pending) {
            self.chat.messages.pop();
        }
        self.chat.streaming_parts.clear();
        self.chat.message_queue.clear();
        self.chat.last_chunk_was_thought = false;
        let _ = crate::app::session::save_session(&self.chat);
    }

    pub fn rollback_transactions(&mut self) {
        if !self.last_file_backups.is_empty() {
            for backup in &self.last_file_backups {
                match &backup.original_content {
                    Some(content) => {
                        let _ = std::fs::write(&backup.path, content);
                    }
                    None => {
                        let _ = std::fs::remove_file(&backup.path);
                    }
                }
            }
            self.last_file_backups.clear();
        }
    }

    pub fn backup_before_execution(&mut self, name: &str, args: &serde_json::Value) {
        let mut paths_to_backup = Vec::new();
        match name {
            "edit_file" | "write_file" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    paths_to_backup.push(path.to_owned());
                }
            }
            "edit_files" => {
                if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                    for edit_val in edits {
                        if let Some(path) = edit_val.get("path").and_then(|v| v.as_str()) {
                            paths_to_backup.push(path.to_owned());
                        }
                    }
                }
            }
            _ => {}
        }

        for path in paths_to_backup {
            if self.last_file_backups.iter().any(|b| b.path == path) {
                continue;
            }
            let original_content = if std::path::Path::new(&path).exists() {
                std::fs::read_to_string(&path).ok()
            } else {
                None
            };
            self.last_file_backups.push(FileBackup {
                path,
                original_content,
            });
        }
    }

    pub fn start_function_execution(&mut self, name: &str, args: &serde_json::Value) {
        if name == "run_bash_command" {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let body = format!("$ {}\nRunning...\n", cmd);
            self.chat
                .messages
                .push(MessageLine::shell(cmd.to_owned(), body, false));
        } else {
            let summary = format!("**{}** executing...", name);
            self.chat.messages.push(MessageLine::tool(summary));
        }
        self.chat.scroll = 0;
    }
}
