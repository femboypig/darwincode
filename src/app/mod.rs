pub mod chat;
pub mod model;
pub mod permission;
pub mod setup;
pub mod session;

pub use chat::{ChatState, MessageLine, ChatCommand, CommandSuggestion};
pub use model::ModelPickerState;
pub use permission::PermissionPickerState;
pub use setup::{SetupState, SetupField};
pub use session::SessionPickerState;

use anyhow::Result;
use crate::config::{PermissionLevel, StoredConfig};
use crate::gemini::{ChatMessage, GeminiResponse};

#[derive(Clone, Debug)]
pub enum FunctionAction {
    Execute {
        name: String,
        args: serde_json::Value,
    },
    ResumeGeneration(GenerationRequest),
}

#[derive(Clone, Debug)]
pub struct GenerationRequest {
    pub config: StoredConfig,
    pub history: Vec<ChatMessage>,
}

#[derive(Clone, Debug)]
pub enum SubmitAction {
    Generate(GenerationRequest),
    LoadModels(StoredConfig),
    ExecuteFunction {
        name: String,
        args: serde_json::Value,
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
}

fn detect_background_light() -> bool {
    // Check environment variables first (fast)
    if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
        if let Some(bg) = colorfgbg.split(';').last() {
            if let Ok(bg_num) = bg.parse::<i32>() {
                return bg_num == 7 || (bg_num >= 9 && bg_num <= 15);
            }
        }
    }

    // Try querying terminal background color via OSC 11
    use std::io::{Read, Write};
    
    // We only try this if stdin/stdout are terminals and raw mode is not enabled yet
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(b"\x1b]11;?\x07");
    let _ = stdout.flush();

    // Put stdin in raw mode temporarily to read the response immediately without waiting for enter key
    if crossterm::terminal::enable_raw_mode().is_ok() {
        let mut buf = [0u8; 100];
        let mut read_bytes = 0;
        // Poll stdin with a short timeout (e.g. 50ms)
        if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(50)) {
            // Read response
            let mut stdin = std::io::stdin();
            if let Ok(n) = stdin.read(&mut buf) {
                read_bytes = n;
            }
        }
        let _ = crossterm::terminal::disable_raw_mode();

        if read_bytes > 0 {
            let response = String::from_utf8_lossy(&buf[..read_bytes]);
            // Response is typically in the format: \x1b]11;rgb:XXXX/XXXX/XXXX\x07
            if let Some(rgb_idx) = response.find("rgb:") {
                let rgb_str = &response[rgb_idx + 4..];
                let parts: Vec<&str> = rgb_str.split(|c| c == '/' || c == '\x07' || c == '\x1b').collect();
                if parts.len() >= 3 {
                    let parse_hex = |s: &str| -> Option<u32> {
                        let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                        u32::from_str_radix(&clean, 16).ok()
                    };
                    if let (Some(r), Some(g), Some(b)) = (parse_hex(parts[0]), parse_hex(parts[1]), parse_hex(parts[2])) {
                        let max_val = if parts[0].len() > 2 { 65535.0 } else { 255.0 };
                        let r_norm = (r as f64) / max_val;
                        let g_norm = (g as f64) / max_val;
                        let b_norm = (b as f64) / max_val;
                        let luminance = 0.2126 * r_norm + 0.7152 * g_norm + 0.0722 * b_norm;
                        return luminance > 0.5; // Light background if luminance > 0.5
                    }
                }
            }
        }
    }

    false // Default to dark mode
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
    pub terminal_light: bool,
}

impl App {
    pub fn new(config: Option<StoredConfig>) -> Self {
        let terminal_light = detect_background_light();
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
                terminal_light,
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
                terminal_light,
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

        // Special case: If user enters exit or permissions command, execute it immediately even if busy!
        if let Some(command) = ChatCommand::parse(&input) {
            if matches!(command, ChatCommand::Exit | ChatCommand::Permissions(_)) {
                self.chat.input.clear();
                self.chat.cursor = 0;
                self.chat.input_scroll = 0;
                self.chat.scroll = 0;
                return self.run_command(command);
            }
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

        self.chat.history.push(ChatMessage::user(input.clone()));
        let _ = session::save_session(&self.chat);
        self.chat.messages.push(MessageLine::user(input));
        self.chat.messages.push(MessageLine::pending());
        self.pending = Some(PendingTask::Generating);
        self.status = "Working...".to_owned();

        Some(SubmitAction::Generate(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
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

        Some(SubmitAction::Generate(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
        }))
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

            let is_thought = part.get("thought").and_then(|v| v.as_bool()).unwrap_or(false)
                || part.get("thought_signature").is_some();

            if is_thought {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        if show_thoughts {
                            self.append_to_chat_messages("Darwin", format!("░ Thinking: {text}"));
                        } else {
                            if !self.chat.messages.last().is_some_and(|m| m.text == "░ Thinking...") {
                                self.chat.messages.push(MessageLine::assistant("░ Thinking...".to_owned()));
                            }
                        }
                    }
                }
                continue;
            }

            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    self.append_to_chat_messages("Darwin", text.to_owned());
                }
            }
        }
        
        self.chat.scroll = 0;
    }

    fn append_to_chat_messages(&mut self, author: &'static str, text: String) {
        if let Some(msg) = self.chat.messages.last_mut() {
            if msg.author == author && !msg.pending && !msg.is_shell && !msg.is_tool {
                msg.text.push_str(&text);
                return;
            }
        }
        
        self.chat.messages.push(MessageLine {
            author,
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
        });
    }

    pub fn handle_stream_error(&mut self, error: String) {
        if let Some(m) = self.chat.messages.last_mut() {
            if m.pending {
                self.chat.messages.pop();
            }
        }
        self.chat.messages.push(MessageLine::error(error));
        self.chat.streaming_parts.clear();
        self.chat.scroll = 0;
        self.pending = None;
        self.status = "Ready".to_owned();
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
            return None;
        }

        self.chat.history.push(ChatMessage {
            role: "model".to_owned(),
            parts: parts.clone(),
        });
        let _ = session::save_session(&self.chat);

        let mut first_call = None;
        for part in &parts {
            if let Some(call) = part.get("functionCall") {
                if first_call.is_none() {
                    if let (Some(name), Some(args)) = (call.get("name").and_then(|v| v.as_str()), call.get("args")) {
                        first_call = Some((name.to_owned(), args.clone()));
                    }
                }
            }
        }

        if let Some((name, args)) = first_call {
            let permission = self.chat.config.permission_level;
            let auto_allowed = permission == PermissionLevel::Chaos || (permission == PermissionLevel::Safe && (name == "read_file" || name == "list_directory" || name == "search_files"));
            
            if auto_allowed {
                self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                self.status = format!("Auto-executing {name}");
                Some(FunctionAction::Execute { name, args })
            } else if permission == PermissionLevel::Safe && (name == "run_bash_command" || name == "edit_file" || name == "write_file") {
                self.pending = Some(PendingTask::Generating);
                self.complete_function_execution(name, serde_json::json!({"error": "Permission denied: restricted mode"}))
            } else {
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
            PendingTask::ExecutingFunction { name } => return Some(format!("Running {name}{frame}")),
        };

        Some(format!("{label}{frame}"))
    }

    pub fn command_suggestions(&self) -> Vec<CommandSuggestion> {
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

            return options.into_iter()
                .filter(|(name, _)| name.starts_with(input))
                .map(|(name, desc)| CommandSuggestion { 
                    name: name.to_owned(), 
                    description: desc.to_owned() 
                })
                .collect();
        }

        if input.starts_with("/resume ") || input == "/resume" {
            if let Ok(sessions) = session::list_saved_sessions() {
                return sessions.into_iter()
                    .map(|meta| {
                        CommandSuggestion { 
                            name: format!("/resume {}", meta.id), 
                            description: meta.snippet 
                        }
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
                    self.chat.messages.push(MessageLine::assistant(format!("Permission level set to **{level_label}**")));
                    let _ = self.chat.config.save();
                    
                    if let Some(PendingTask::ConfirmFunction { name, args }) = self.pending.clone() {
                        let auto_allowed = level == PermissionLevel::Chaos || (level == PermissionLevel::Safe && (name == "read_file" || name == "list_directory" || name == "search_files"));
                        if auto_allowed {
                            self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                            self.status = format!("Auto-executing {name}");
                            return Some(SubmitAction::ExecuteFunction { name, args });
                        } else if level == PermissionLevel::Safe && (name == "run_bash_command" || name == "edit_file" || name == "write_file") {
                            if let Some(crate::app::FunctionAction::ResumeGeneration(request)) = self.complete_function_execution(name, serde_json::json!({"error": "Permission denied: restricted mode"})) {
                                return Some(SubmitAction::Generate(request));
                            }
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
                    self.status = "Select permission level. Enter to apply, Esc to cancel.".to_owned();
                    None
                }
            }
            ChatCommand::Resume(session_id) => {
                if let Some(id) = session_id {
                    match session::load_session(&id) {
                        Ok(s) => {
                            self.chat.session_id = s.id;
                            self.chat.history = s.history;
                            self.chat.messages = session::rebuild_messages_from_history(&self.chat.history);
                            self.status = format!("Resumed session: {}", id);
                        }
                        Err(e) => {
                            self.chat.messages.push(MessageLine::error(format!("Failed to load session '{}': {}", id, e)));
                        }
                    }
                } else {
                    match session::list_saved_sessions() {
                        Ok(list) => {
                            self.sessions.sessions = list;
                            self.sessions.selected = 0;
                            self.screen = Screen::Sessions;
                            self.status = "Select a session to resume. Enter to apply, Esc to cancel.".to_owned();
                        }
                        Err(e) => {
                            self.chat.messages.push(MessageLine::error(format!("Failed to list sessions: {}", e)));
                        }
                    }
                }
                None
            }
            ChatCommand::Clear => {
                self.chat.history.clear();
                self.chat.messages.clear();
                self.chat.scroll = 0;
                let _ = session::save_session(&self.chat);
                self.status = "Chat history cleared".to_owned();
                None
            }
            ChatCommand::New => {
                self.chat.history.clear();
                self.chat.messages.clear();
                self.chat.scroll = 0;
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
                            self.chat.messages.push(MessageLine::assistant("No saved sessions found.".to_owned()));
                        } else {
                            let mut msg = "Saved sessions:\n".to_owned();
                            for meta in list {
                                msg.push_str(&format!("- **{}**: {}\n", meta.id, meta.snippet));
                            }
                            self.chat.messages.push(MessageLine::assistant(msg));
                        }
                    }
                    Err(e) => {
                        self.chat.messages.push(MessageLine::error(format!("Failed to list sessions: {}", e)));
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
                                 - **/help**: Display this help card\n\
                                 - **/exit** / **/quit**: Exit the application\n\n\
                                 Hotkeys (in Chat):\n\
                                 - **Ctrl+S**: Open Setup screen\n\
                                 - **Ctrl+P**: Switch active Model instantly";
                self.chat.messages.push(MessageLine::assistant(help_text.to_owned()));
                None
            }

            ChatCommand::Unknown(command) => {
                self.chat.messages.push(MessageLine::assistant(format!(
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
            self.chat.messages.push(MessageLine::assistant(format!("Permission level set to **{label}**")));
            let _ = self.chat.config.save();
            
            if let Some(PendingTask::ConfirmFunction { name, args }) = self.pending.clone() {
                let auto_allowed = *level == PermissionLevel::Chaos || (*level == PermissionLevel::Safe && (name == "read_file" || name == "list_directory" || name == "search_files"));
                if auto_allowed {
                    self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                    self.status = format!("Auto-executing {name}");
                    ret = Some(SubmitAction::ExecuteFunction { name, args });
                } else if *level == PermissionLevel::Safe && (name == "run_bash_command" || name == "edit_file" || name == "write_file") {
                    if let Some(crate::app::FunctionAction::ResumeGeneration(request)) = self.complete_function_execution(name, serde_json::json!({"error": "Permission denied: restricted mode"})) {
                        ret = Some(SubmitAction::Generate(request));
                    }
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
            self.chat.history.push(ChatMessage {
                role: "function".to_owned(),
                parts: vec![serde_json::json!({
                    "functionResponse": {
                        "name": name.clone(),
                        "response": { "error": "User denied the tool call" }
                    }
                })],
            });
            let _ = session::save_session(&self.chat);
            self.chat.messages.push(MessageLine::tool(format!("**{name}** → Cancelled")));
            self.chat.messages.push(MessageLine::pending());
            self.pending = Some(PendingTask::Generating);
            self.status = "Working...".to_owned();

            return Some(FunctionAction::ResumeGeneration(GenerationRequest {
                config: self.chat.config.clone(),
                history: self.chat.history.clone(),
            }));
        }

        self.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
        self.status = format!("Executing {name}");
        Some(FunctionAction::Execute { name, args })
    }

    pub fn complete_function_execution(&mut self, name: String, response: serde_json::Value) -> Option<FunctionAction> {
        self.chat.scroll = 0;
        let args = if let Some(ChatMessage { parts, .. }) = self.chat.history.iter().rev().find(|m| m.role == "model") {
            parts.iter().find_map(|p| {
                if let Some(call) = p.get("functionCall") {
                    if call.get("name").and_then(|v| v.as_str()) == Some(&name) {
                        return call.get("args").cloned();
                    }
                }
                None
            }).unwrap_or(serde_json::Value::Null)
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
        
        let tool_label = {
            let mut label = String::new();
            let mut next_cap = true;
            for c in name.chars() {
                if c == '_' { next_cap = true; }
                else if next_cap {
                    label.push(c.to_ascii_uppercase());
                    next_cap = false;
                } else {
                    label.push(c);
                }
            }
            label
        };

        if name == "run_bash_command" {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let mut output = String::new();
            let mut icon = "✓";
            
            if let Some(status) = response.get("status").and_then(|v| v.as_i64()) {
                if status != 0 { icon = "✗"; }
            } else if response.get("error").is_some() {
                icon = "✗";
            }
            
            let stdout = response.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
            let stderr = response.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
            if !stdout.is_empty() { output.push_str(stdout); }
            if !stderr.is_empty() { 
                if !output.is_empty() { output.push('\n'); }
                output.push_str(stderr);
            }
            
            self.chat.messages.push(MessageLine::shell(cmd.to_owned(), output, icon == "✓"));
        } else {
            let mut summary = format!("**{tool_label}** ");
            let mut res_parts = Vec::new();
            
            if let Some(err) = response.get("error").and_then(|v| v.as_str()) {
                res_parts.push(format!("Error: {err}"));
            } else {
                match name.as_str() {
                    "edit_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                        res_parts.push(format!("`{path}` updated"));
                        if let Some(diff) = response.get("diff").and_then(|v| v.as_str()) {
                            res_parts.push(format!("\n{diff}"));
                        }
                    }
                    "read_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                        res_parts.push(format!("`{path}` read successfully"));
                    }
                    "write_file" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                        res_parts.push(format!("`{path}` written successfully"));
                    }
                    "list_directory" => {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                        if let Some(files) = response.get("files").and_then(|v| v.as_array()) {
                            res_parts.push(format!("`{path}` → {} items", files.len()));
                        }
                    }
                    "search_files" => {
                        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
                        if let Some(matches) = response.get("matches").and_then(|v| v.as_str()) {
                            res_parts.push(format!("`{pattern}` → {} matches", matches.lines().count()));
                        }
                    }
                    _ => {
                        if let Some(obj) = response.as_object() {
                            for (k, v) in obj {
                                if k == "content" || k == "matches" || k == "diff" {
                                    res_parts.push(format!("{k}=..."));
                                } else if let Some(arr) = v.as_array() {
                                    res_parts.push(format!("{k}={} items", arr.len()));
                                } else {
                                    res_parts.push(format!("{k}={v}"));
                                }
                            }
                        }
                    }
                }
            }
            
            if !res_parts.is_empty() {
                summary.push_str("→ ");
                summary.push_str(&res_parts.join(", "));
            }
            self.chat.messages.push(MessageLine::tool(summary));
        }

        self.chat.messages.push(MessageLine::pending());
        self.pending = Some(PendingTask::Generating);
        self.status = "Working...".to_owned();

        Some(FunctionAction::ResumeGeneration(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
        }))
    }
}
