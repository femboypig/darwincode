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
    pub dev_mode: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DevelopMode {
    Plan,
    Build,
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
    pub confirm_scroll: std::cell::Cell<u16>,
    pub ask_user: AskUserState,
    pub sessions_cache: std::sync::Arc<std::sync::Mutex<Option<Vec<crate::app::session::SessionMeta>>>>,
    pub dev_mode: DevelopMode,
    pub model_picker_open: bool,
}

impl App {
    pub fn new(config: Option<StoredConfig>) -> Self {
        let keybindings = crate::tui::keybindings::load_keybindings();
        let sessions_cache = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cache_clone = sessions_cache.clone();
        std::thread::spawn(move || {
            if let Ok(sessions) = crate::app::session::list_saved_sessions() {
                if let Ok(mut guard) = cache_clone.lock() {
                    *guard = Some(sessions);
                }
            }
        });

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
                confirm_scroll: std::cell::Cell::new(0),
                ask_user: AskUserState {
                    question: String::new(),
                    options: Vec::new(),
                    selected_idx: 0,
                    custom_input: String::new(),
                    is_custom: false,
                },
                sessions_cache: sessions_cache.clone(),
                dev_mode: DevelopMode::Build,
                model_picker_open: false,
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
                confirm_scroll: std::cell::Cell::new(0),
                ask_user: AskUserState {
                    question: String::new(),
                    options: Vec::new(),
                    selected_idx: 0,
                    custom_input: String::new(),
                    is_custom: false,
                },
                sessions_cache,
                dev_mode: DevelopMode::Build,
                model_picker_open: false,
            },
        }
    }

    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn dev_mode_label(&self) -> &'static str {
        match self.dev_mode {
            DevelopMode::Plan => "Plan",
            DevelopMode::Build => "Build",
        }
    }

    pub fn model_label(&self) -> &str {
        self.chat.config.model.trim_start_matches("models/")
    }

    pub fn toggle_dev_mode(&mut self) {
        self.dev_mode = match self.dev_mode {
            DevelopMode::Plan => DevelopMode::Build,
            DevelopMode::Build => DevelopMode::Plan,
        };
        self.status = format!("Switched to {} mode", self.dev_mode_label());
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
            self.model_picker_open = true;
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

        // Special case: If user enters exit, permissions, or shell command, execute it immediately even if busy!
        if let Some(command) = ChatCommand::parse(&input)
            && matches!(
                command,
                ChatCommand::Exit | ChatCommand::Permissions(_) | ChatCommand::Shell(_)
            )
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
        self.chat
            .history
            .push(self::chat::resolve_prompt_message(&input));
        self.save_session();
        let cleaned_input = self::chat::clean_prompt_images(&input);
        self.chat.messages.push(MessageLine::user(cleaned_input));
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
            dev_mode: self.dev_mode_label().to_owned(),
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
        self.chat
            .history
            .push(self::chat::resolve_prompt_message(&input));
        self.save_session();
        let cleaned_input = self::chat::clean_prompt_images(&input);
        self.chat.messages.push(MessageLine::user(cleaned_input));
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
            dev_mode: self.dev_mode_label().to_owned(),
        }))
    }

    pub fn handle_bash_stdout(&mut self, pid: Option<u32>, chunk: String) {
        let mut found = false;
        if let Some(p) = pid
            && let Some(msg) = self
                .chat
                .messages
                .iter_mut()
                .rev()
                .find(|m| m.is_shell && m.shell_pid == Some(p))
        {
            if msg.text.ends_with("\nRunning...\n") {
                msg.text.truncate(msg.text.len() - 11);
            }
            msg.text.push_str(&chunk);
            *msg.cached_wrapped.borrow_mut() = None;
            found = true;
        }
        if !found && let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_shell) {
            if msg.text.ends_with("\nRunning...\n") {
                msg.text.truncate(msg.text.len() - 11);
            }
            msg.text.push_str(&chunk);
            *msg.cached_wrapped.borrow_mut() = None;
        }
    }

    pub fn handle_bash_stderr(&mut self, pid: Option<u32>, chunk: String) {
        let mut found = false;
        if let Some(p) = pid
            && let Some(msg) = self
                .chat
                .messages
                .iter_mut()
                .rev()
                .find(|m| m.is_shell && m.shell_pid == Some(p))
        {
            if msg.text.ends_with("\nRunning...\n") {
                msg.text.truncate(msg.text.len() - 11);
            }
            msg.text.push_str(&chunk);
            *msg.cached_wrapped.borrow_mut() = None;
            found = true;
        }
        if !found && let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_shell) {
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
            shell_pid: None,
            shell_session_id: None,
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
        self.save_session();
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
        self.save_session();

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
            
            let is_blocked_by_plan = self.dev_mode == DevelopMode::Plan
                && (name == "sh"
                    || name == "write"
                    || name == "edit"
                    || name == "patch"
                    || name == "kill");

            let is_allowed_plan_file = if self.dev_mode == DevelopMode::Plan {
                let mut allowed = false;
                if name == "write" || name == "edit" || name == "patch" {
                    if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
                        allowed = Self::is_plan_file_allowed(path_str);
                    } else if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                        allowed = edits.iter().all(|e| {
                            e.get("path").and_then(|v| v.as_str()).map(Self::is_plan_file_allowed).unwrap_or(false)
                        });
                    }
                }
                allowed
            } else {
                false
            };

            let auto_allowed = !is_blocked_by_plan && (permission == PermissionLevel::Chaos
                || (permission == PermissionLevel::Safe
                    && (name == "read"
                        || name == "grep"
                        || name == "glob"
                        || name == "websearch"
                        || name == "ask"
                        || name == "todo"
                        || name == "ps"
                        || name == "logs")));

            if auto_allowed || is_allowed_plan_file {
                self.backup_before_execution(&name, &args);
                self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                self.status = format!("Auto-executing {name}");
                self.start_function_execution(&name, &args);
                Some(FunctionAction::Execute {
                    name,
                    args,
                    config: self.chat.config.clone(),
                })
            } else if (permission == PermissionLevel::Safe && !is_allowed_plan_file
                && (name == "sh"
                    || name == "write"
                    || name == "edit"
                    || name == "patch"
                    || name == "kill"))
                || (self.dev_mode == DevelopMode::Plan && is_blocked_by_plan && !is_allowed_plan_file)
            {
                self.pending = Some(PendingTask::Generating);
                let err_msg = if self.dev_mode == DevelopMode::Plan {
                    "Permission denied: Plan mode only allows editing .darwincode/plans/*.md"
                } else {
                    "Permission denied: restricted mode"
                };
                self.complete_function_execution(
                    name,
                    serde_json::json!({"error": err_msg}),
                )
            } else {
                self.confirm_scroll.set(0);
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
        let cached = if let Ok(cache) = self.sessions_cache.lock() {
            cache.clone()
        } else {
            None
        };

        if let Some(list) = cached {
            self.sessions.sessions = list;
            self.sessions.selected = 0;
            self.sessions.query = String::new();
            self.screen = Screen::Sessions;
            self.status =
                "Select a session to resume. Enter to apply, Esc to cancel.".to_owned();
        } else {
            match crate::app::session::list_saved_sessions() {
                Ok(list) => {
                    if let Ok(mut cache) = self.sessions_cache.lock() {
                        *cache = Some(list.clone());
                    }
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
        if let Some((_, path_prefix)) =
            self::chat::get_at_word_at_cursor(&self.chat.input, self.chat.cursor)
        {
            return self::chat::get_path_suggestions(&path_prefix);
        }

        let input = self.chat.input.trim_start();
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

        if input.starts_with("/resume ") || input.starts_with("/resum") {
            if let Ok(cache) = self.sessions_cache.lock() {
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
        let suggestions = self.command_suggestions();
        if suggestions.is_empty() {
            return;
        }
        let active_idx = self.chat.suggestion_idx.min(suggestions.len().saturating_sub(1));
        if let Some(suggestion) = suggestions.into_iter().nth(active_idx) {
            if let Some((start_idx, _)) =
                self::chat::get_at_word_at_cursor(&self.chat.input, self.chat.cursor)
            {
                let char_indices: Vec<(usize, char)> = self.chat.input.char_indices().collect();
                let prefix: String = char_indices[..start_idx].iter().map(|&(_, c)| c).collect();
                let suffix: String = char_indices[self.chat.cursor..]
                    .iter()
                    .map(|&(_, c)| c)
                    .collect();

                self.chat.input = format!("{}{}{}", prefix, suggestion.name, suffix);

                let inserted_len = suggestion.name.chars().count();
                self.chat.cursor = start_idx + inserted_len;
                self.chat.suggestion_idx = 0;
                return;
            }

            self.chat.input = format!("{} ", suggestion.name);
            self.chat.cursor = self.chat.input.chars().count();
            self.chat.suggestion_idx = 0;
        }
    }

    fn run_command(&mut self, command: ChatCommand) -> Option<SubmitAction> {
        match command {
            ChatCommand::Plan => {
                self.dev_mode = DevelopMode::Plan;
                self.status = "Switched to Plan mode".to_owned();
                self.chat.messages.push(MessageLine::info(
                    "Switched to **Plan** mode (read-only for workspace files)".to_owned(),
                ));
                None
            }
            ChatCommand::Build => {
                self.dev_mode = DevelopMode::Build;
                self.status = "Switched to Build mode".to_owned();
                self.chat.messages.push(MessageLine::info(
                    "Switched to **Build** mode (full tools access)".to_owned(),
                ));
                None
            }
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
                                && (name == "read"
                                    || name == "grep"
                                    || name == "glob"
                                    || name == "websearch"
                                    || name == "ask"
                                    || name == "todo"
                                    || name == "ps"
                                    || name == "logs"));
                        if auto_allowed {
                            self.backup_before_execution(&name, &args);
                            self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                            self.status = format!("Auto-executing {name}");
                            return Some(SubmitAction::ExecuteFunction { name, args, config: self.chat.config.clone() });
                        } else if level == PermissionLevel::Safe && (name == "sh" || name == "write" || name == "edit" || name == "patch" || name == "kill")
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
                    if let Err(e) = self.resume_session(&id) {
                        self.chat.messages.push(MessageLine::error(format!(
                            "Failed to load session '{}': {}",
                            id, e
                        )));
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
                self.save_session();
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
                self.save_session();
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

            ChatCommand::Shell(session_arg) => {
                if let Some(session_id) = session_arg {
                    // Switch/focus to a specific session or active process
                    let mut found = false;
                    let registry = crate::tui::PERSISTENT_SESSIONS
                        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
                    let has_session = {
                        let map = registry.lock().unwrap();
                        map.contains_key(session_id.as_str())
                    };

                    let is_bg_process = {
                        let bg_registry = crate::tui::BACKGROUND_PROCESSES.get_or_init(|| {
                            std::sync::Mutex::new(std::collections::HashMap::new())
                        });
                        let map = bg_registry.lock().unwrap();
                        map.keys().any(|k| k.to_string() == session_id)
                    };

                    if has_session {
                        if let Ok(mut guard) = crate::tui::ACTIVE_PERSISTENT_SESSION_ID.lock() {
                            let opt: &mut Option<String> = &mut guard;
                            *opt = Some(session_id.clone());
                        }
                        self.chat.focused_shell_session_id = Some(session_id.clone());
                        self.chat.focused_shell_pid = None;
                        self.chat.shell_focused = true;

                        for m in &mut self.chat.messages {
                            if m.is_shell {
                                *m.cached_wrapped.borrow_mut() = None;
                            }
                        }

                        let mut scrolled = false;
                        let target_msg_idx = self
                            .chat
                            .messages
                            .iter()
                            .enumerate()
                            .rev()
                            .find(|(_, m)| {
                                m.is_shell && m.shell_session_id.as_ref() == Some(&session_id)
                            })
                            .map(|(idx, _)| idx);
                        if let Some(msg_idx) = target_msg_idx
                            && let Some(&(_, start_line, end_line)) = self
                                .chat
                                .message_line_ranges
                                .borrow()
                                .iter()
                                .find(|&&(idx, _, _)| idx == msg_idx)
                        {
                            let total_lines = self
                                .chat
                                .message_line_ranges
                                .borrow()
                                .last()
                                .map(|(_, _, end)| *end)
                                .unwrap_or(0);
                            let viewport_height = self
                                .chat
                                .messages_area
                                .get()
                                .map(|r| r.height as usize)
                                .unwrap_or(20);
                            let max_scroll = total_lines.saturating_sub(viewport_height);
                            let msg_height = end_line.saturating_sub(start_line);
                            let mid_line = start_line + msg_height / 2;
                            let target_scroll_y = mid_line.saturating_sub(viewport_height / 2);
                            let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                            self.chat.scroll = scroll_val as u16;
                            scrolled = true;
                        }
                        if !scrolled {
                            self.chat.scroll = 0;
                        }

                        *self.chat.message_line_ranges.borrow_mut() = Vec::new();
                        self.status = "Ready".to_owned();
                        found = true;
                    } else if let Ok(guard) = crate::tui::RUNNING_PROCESS_PID.lock()
                        && let Some(pid) = *guard
                        && pid.to_string() == session_id
                    {
                        if let Ok(mut active_guard) =
                            crate::tui::ACTIVE_PERSISTENT_SESSION_ID.lock()
                        {
                            *active_guard = None;
                        }
                        self.chat.focused_shell_session_id = None;
                        self.chat.focused_shell_pid = Some(pid);
                        self.chat.shell_focused = true;

                        for m in &mut self.chat.messages {
                            if m.is_shell {
                                *m.cached_wrapped.borrow_mut() = None;
                            }
                        }

                        let mut scrolled = false;
                        let target_msg_idx = self
                            .chat
                            .messages
                            .iter()
                            .enumerate()
                            .rev()
                            .find(|(_, m)| m.is_shell && m.shell_pid == Some(pid))
                            .map(|(idx, _)| idx);
                        if let Some(msg_idx) = target_msg_idx
                            && let Some(&(_, start_line, end_line)) = self
                                .chat
                                .message_line_ranges
                                .borrow()
                                .iter()
                                .find(|&&(idx, _, _)| idx == msg_idx)
                        {
                            let total_lines = self
                                .chat
                                .message_line_ranges
                                .borrow()
                                .last()
                                .map(|(_, _, end)| *end)
                                .unwrap_or(0);
                            let viewport_height = self
                                .chat
                                .messages_area
                                .get()
                                .map(|r| r.height as usize)
                                .unwrap_or(20);
                            let max_scroll = total_lines.saturating_sub(viewport_height);
                            let msg_height = end_line.saturating_sub(start_line);
                            let mid_line = start_line + msg_height / 2;
                            let target_scroll_y = mid_line.saturating_sub(viewport_height / 2);
                            let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                            self.chat.scroll = scroll_val as u16;
                            scrolled = true;
                        }
                        if !scrolled {
                            self.chat.scroll = 0;
                        }

                        *self.chat.message_line_ranges.borrow_mut() = Vec::new();
                        self.status = "Ready".to_owned();
                        found = true;
                    } else if is_bg_process {
                        let pid = session_id.parse::<u32>().unwrap();
                        if let Ok(mut active_guard) =
                            crate::tui::ACTIVE_PERSISTENT_SESSION_ID.lock()
                        {
                            *active_guard = None;
                        }
                        self.chat.focused_shell_session_id = None;
                        self.chat.focused_shell_pid = Some(pid);
                        self.chat.shell_focused = true;

                        for m in &mut self.chat.messages {
                            if m.is_shell {
                                *m.cached_wrapped.borrow_mut() = None;
                            }
                        }

                        let mut scrolled = false;
                        let target_msg_idx = self
                            .chat
                            .messages
                            .iter()
                            .enumerate()
                            .rev()
                            .find(|(_, m)| m.is_shell && m.shell_pid == Some(pid))
                            .map(|(idx, _)| idx);
                        if let Some(msg_idx) = target_msg_idx
                            && let Some(&(_, start_line, end_line)) = self
                                .chat
                                .message_line_ranges
                                .borrow()
                                .iter()
                                .find(|&&(idx, _, _)| idx == msg_idx)
                        {
                            let total_lines = self
                                .chat
                                .message_line_ranges
                                .borrow()
                                .last()
                                .map(|(_, _, end)| *end)
                                .unwrap_or(0);
                            let viewport_height = self
                                .chat
                                .messages_area
                                .get()
                                .map(|r| r.height as usize)
                                .unwrap_or(20);
                            let max_scroll = total_lines.saturating_sub(viewport_height);
                            let msg_height = end_line.saturating_sub(start_line);
                            let mid_line = start_line + msg_height / 2;
                            let target_scroll_y = mid_line.saturating_sub(viewport_height / 2);
                            let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                            self.chat.scroll = scroll_val as u16;
                            scrolled = true;
                        }
                        if !scrolled {
                            self.chat.scroll = 0;
                        }

                        *self.chat.message_line_ranges.borrow_mut() = Vec::new();
                        self.status = "Ready".to_owned();
                        found = true;
                    }
                    if !found {
                        self.chat.messages.push(MessageLine::error(format!(
                            "Shell session or active process '{}' not found or cannot be focused.",
                            session_id
                        )));
                    }
                } else {
                    // List all active sessions
                    let registry = crate::tui::PERSISTENT_SESSIONS
                        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));

                    let mut session_infos = Vec::new();

                    // 1. Persistent Sessions
                    {
                        let map = registry.lock().unwrap();
                        for (id, session) in map.iter() {
                            let is_running =
                                matches!(session.child.lock().unwrap().try_wait(), Ok(None));
                            if is_running {
                                let active_str = if self.chat.shell_focused
                                    && self.chat.focused_shell_session_id.as_ref() == Some(id)
                                {
                                    " (focused)"
                                } else {
                                    ""
                                };
                                session_infos.push(format!(
                                    "- **Persistent Session: {}** (PID: {}) [active]{}",
                                    id, session.pid, active_str
                                ));
                            }
                        }
                    }

                    // 2. Non-persistent Background Processes
                    let bg_registry = crate::tui::BACKGROUND_PROCESSES
                        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
                    {
                        let map = bg_registry.lock().unwrap();
                        for (pid, proc) in map.iter() {
                            let is_running = proc.exit_status.lock().unwrap().is_none();
                            if is_running {
                                session_infos.push(format!(
                                    "- **Background Process: {}** (PID: {}) [active]",
                                    proc._command, pid
                                ));
                            }
                        }
                    }

                    // 3. Foreground Process
                    if let Ok(guard) = crate::tui::RUNNING_PROCESS_PID.lock()
                        && let Some(pid) = *guard
                    {
                        let is_focused =
                            self.chat.shell_focused && self.chat.focused_shell_pid == Some(pid);
                        let active_str = if is_focused { " (focused)" } else { "" };
                        session_infos.push(format!(
                            "- **Foreground Process** (PID: {}) [active]{}",
                            pid, active_str
                        ));
                    }

                    if session_infos.is_empty() {
                        self.chat.messages.push(MessageLine::info(
                            "No active shell sessions at this time.".to_owned(),
                        ));
                    } else {
                        session_infos.sort();
                        let info_text = format!(
                            "Active shell sessions:\n{}\nUse `/shell [session_id_or_pid]` to focus a session.",
                            session_infos.join("\n")
                        );
                        self.chat.messages.push(MessageLine::info(info_text));
                    }
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
                                 - **/shell [session_id_or_pid]**: List or focus active shell sessions\n\
                                 - **/plan**: Switch to Plan mode (read-only for workspace files)\n\
                                 - **/build**: Switch to Build mode (full tools access)\n\
                                 - **/help**: Display this help card\n\
                                 - **/exit** / **/quit**: Exit the application\n\n\
                                 Hotkeys (in Chat):\n\
                                 - **Ctrl+S**: Open Setup screen\n\
                                 - **Ctrl+P**: Switch active Model instantly\n\
                                 - **Ctrl+T**: Toggle between Plan and Build modes";
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
        self.model_picker_open = false;
        self.models.clear_query();
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
                self.model_picker_open = false;
                self.models.clear_query();
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
                        && (name == "read"
                            || name == "grep"
                            || name == "glob"
                            || name == "websearch"
                            || name == "ask"
                            || name == "todo"
                            || name == "ps"
                            || name == "logs"));
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
                    && (name == "sh"
                        || name == "write"
                        || name == "edit"
                        || name == "patch"
                        || name == "kill")
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

    pub fn resume_session(&mut self, id: &str) -> Result<()> {
        let s = session::load_session(id)?;
        self.chat.session_id = s.id;
        self.chat.history = s.history;
        self.chat.messages = session::rebuild_messages_from_history(
            &self.chat.history,
            self.chat.config.show_thoughts,
        );
        self.chat.todos = session::rebuild_todos_from_history(&self.chat.history);
        self.status = format!("Resumed session: {}", id);
        Ok(())
    }

    pub fn save_session(&mut self) {
        if self.chat.session_id.starts_with("test_mock") {
            return;
        }
        let _ = session::save_session(&self.chat);

        if let Ok(mut cache) = self.sessions_cache.lock() {
            if let Some(ref mut list) = *cache {
                let id = self.chat.session_id.clone();
                let snippet = if let Some(first_msg) = self.chat.history.first()
                    && let Some(first_part) = first_msg.parts.first()
                    && let Some(text) = first_part.get("text").and_then(|v| v.as_str())
                {
                    text.chars().take(40).collect::<String>()
                } else {
                    "Empty chat".to_owned()
                };

                list.retain(|s| s.id != id);
                list.insert(0, crate::app::session::SessionMeta { id, snippet });
            }
        }
    }

    pub fn apply_selected_session(&mut self) {
        if let Some(meta) = self.sessions.selected_session() {
            match self.resume_session(&meta.id) {
                Ok(_) => {
                    self.screen = Screen::Chat;
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
            self.save_session();
            return None;
        }

        let is_blocked_by_plan = self.dev_mode == DevelopMode::Plan
            && (name == "sh"
                || name == "write"
                || name == "edit"
                || name == "patch"
                || name == "kill");

        let is_allowed_plan_file = if self.dev_mode == DevelopMode::Plan {
            let mut allowed = false;
            if name == "write" || name == "edit" || name == "patch" {
                if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
                    allowed = Self::is_plan_file_allowed(path_str);
                } else if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                    allowed = edits.iter().all(|e| {
                        e.get("path").and_then(|v| v.as_str()).map(Self::is_plan_file_allowed).unwrap_or(false)
                    });
                }
            }
            allowed
        } else {
            false
        };

        if is_blocked_by_plan && !is_allowed_plan_file {
            self.chat.messages.push(MessageLine::error(
                "Permission denied: Plan mode only allows editing .darwincode/plans/*.md".to_owned(),
            ));
            self.chat.message_queue.clear();
            self.pending = None;
            self.status = "Ready".to_owned();
            if self.chat.messages.last().is_some_and(|m| m.pending) {
                self.chat.messages.pop();
            }
            self.complete_function_execution(
                name,
                serde_json::json!({"error": "Permission denied: Plan mode only allows editing .darwincode/plans/*.md"}),
            );
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

fn is_plan_file_allowed(path_str: &str) -> bool {
    let path = std::path::Path::new(path_str);
    let is_md = path.extension().map(|ext| ext == "md").unwrap_or(false);
    if !is_md {
        return false;
    }
    if path_str.starts_with(".darwincode/plans/") || path_str.starts_with("./.darwincode/plans/") {
        return true;
    }
    if let Ok(current_dir) = std::env::current_dir() {
        let abs_allowed = current_dir.join(".darwincode/plans");
        if let Ok(abs_path) = std::path::Path::new(path_str).canonicalize() {
            if let Ok(abs_allowed_canonical) = abs_allowed.canonicalize() {
                return abs_path.starts_with(abs_allowed_canonical);
            }
        }
        let abs_allowed_str = abs_allowed.to_string_lossy();
        if path_str.starts_with(&*abs_allowed_str) {
            return true;
        }
    }
    false
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

        let mut response = response;
        if name == "todo"
            && let Some(todos_val) = args.get("todos")
        {
            if let Ok(new_todos) =
                serde_json::from_value::<Vec<crate::app::chat::TodoItem>>(todos_val.clone())
            {
                self.chat.todos = new_todos;
                response = serde_json::json!({ "success": true });
            } else {
                response =
                    serde_json::json!({ "success": false, "error": "Invalid todos format." });
            }
        }

        self.chat.history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![serde_json::json!({
                "functionResponse": {
                    "name": name.clone(),
                    "response": response.clone(),
                }
            })],
        });
        self.save_session();
        if name == "sh" {
            let _cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let mut output = String::new();
            let mut success = true;

            let mut is_aborted = false;
            let mut is_running = false;

            if let Some(status) = response.get("status").and_then(|v| v.as_i64()) {
                if status != 0 {
                    success = false;
                }
            } else if let Some(status_str) = response.get("status").and_then(|v| v.as_str()) {
                if status_str == "running" {
                    is_running = true;
                } else {
                    success = false;
                }
            } else if response.get("status").is_some() && response.get("status").unwrap().is_null()
            {
                success = false;
            } else {
                success = false;
                is_aborted = true;
            }

            if let Some(err_str) = response.get("error").and_then(|v| v.as_str())
                && err_str.contains("terminated by user via Ctrl+C")
            {
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
            let error_field = response.get("error").and_then(|v| v.as_str()).unwrap_or("");

            if !stdout.is_empty() {
                output.push_str(stdout);
            }
            if !stderr.is_empty() {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(stderr);
            }
            if !error_field.is_empty() && error_field != "null" {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(error_field);
            }

            if is_aborted {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str("^C\n[Process terminated by user via Ctrl+C]");
            } else if output.is_empty() && !is_running {
                output = "(empty output)".to_owned();
            }

            let pid = response
                .get("pid")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let persistent_session_id = args
                .get("persistent_session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned());
            if let Some(msg) = self
                .chat
                .messages
                .iter_mut()
                .rev()
                .find(|m| m.is_shell && m.shell_pid.is_none())
            {
                msg.text = output;
                msg.shell_success = success;
                msg.shell_pid = pid;
                msg.shell_session_id = persistent_session_id;
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
            dev_mode: self.dev_mode_label().to_owned(),
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
        self.save_session();
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
        let git_root = (|| -> Option<std::path::PathBuf> {
            let git_bin = if std::path::Path::new("/usr/bin/git").exists() {
                "/usr/bin/git"
            } else {
                "git"
            };
            let out = std::process::Command::new(git_bin)
                .args(["rev-parse", "--show-toplevel"])
                .output()
                .ok()?;
            if out.status.success() {
                let path_str = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                Some(std::path::PathBuf::from(path_str))
            } else {
                None
            }
        })();

        let mut paths_to_backup = Vec::new();
        match name {
            "write" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    paths_to_backup.push(path.to_owned());
                }
            }
            "edit" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    paths_to_backup.push(path.to_owned());
                }
                if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                    for edit_val in edits {
                        if let Some(path) = edit_val.get("path").and_then(|v| v.as_str()) {
                            paths_to_backup.push(path.to_owned());
                        }
                    }
                }
            }
            "patch" => {
                if let Some(patch) = args.get("patch").and_then(|v| v.as_str()) {
                    for line in patch.lines() {
                        if line.starts_with("+++ b/") || line.starts_with("+++ ") {
                            let path = if let Some(stripped) = line.strip_prefix("+++ b/") {
                                stripped
                            } else {
                                line.strip_prefix("+++ ").unwrap_or(line)
                            };
                            let path = path.split_whitespace().next().unwrap_or(path);
                            if path != "/dev/null" {
                                paths_to_backup.push(path.to_owned());
                            }
                        }
                        if line.starts_with("--- a/") || line.starts_with("--- ") {
                            let path = if let Some(stripped) = line.strip_prefix("--- a/") {
                                stripped
                            } else {
                                line.strip_prefix("--- ").unwrap_or(line)
                            };
                            let path = path.split_whitespace().next().unwrap_or(path);
                            if path != "/dev/null" && !paths_to_backup.contains(&path.to_owned()) {
                                paths_to_backup.push(path.to_owned());
                            }
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
            let resolved_path = if let Some(ref root) = git_root
                && !std::path::Path::new(&path).is_absolute()
            {
                root.join(&path)
            } else {
                std::path::PathBuf::from(&path)
            };

            let original_content = if resolved_path.exists() {
                std::fs::read_to_string(&resolved_path).ok()
            } else {
                None
            };
            self.last_file_backups.push(FileBackup {
                path: resolved_path.to_string_lossy().into_owned(),
                original_content,
            });
        }
    }

    pub fn start_function_execution(&mut self, name: &str, args: &serde_json::Value) {
        if name == "sh" {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let body = format!("$ {}\nRunning...\n", cmd);
            let persistent_session_id = args
                .get("persistent_session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned());
            self.chat.messages.push(MessageLine::shell(
                cmd.to_owned(),
                body,
                false,
                persistent_session_id,
            ));
        } else {
            let summary = format!("**{}** executing...", name);
            self.chat.messages.push(MessageLine::tool(summary));
        }
        self.chat.scroll = 0;
    }
}
