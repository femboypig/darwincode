use anyhow::Result;
use crate::api::{ChatMessage, GeminiResponse};
use crate::config::{PermissionLevel, StoredConfig};
use super::chat::{ChatCommand, CommandSuggestion, MessageLine, TodoItem};
use super::core::{App, Screen, PendingTask, SubmitAction, FunctionAction, GenerationRequest, FileBackup, DevelopMode};
use super::agent_picker::AgentPickerState;
use super::model::ModelPickerState;
use super::permission::PermissionPickerState;
use super::theme_picker::ThemePickerState;
use super::setup::SetupState;

impl App {
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

        self.proc.last_file_backups.clear();
        self.chat
            .history
            .push(super::chat::resolve_prompt_message(&input));
        self.save_session();
        let cleaned_input = super::chat::clean_prompt_images(&input);
        self.chat.messages.push(MessageLine::user(cleaned_input));
        self.chat.messages.push(MessageLine::pending());
        self.proc.pending = Some(PendingTask::Generating);
        self.status = "Working...".to_owned();

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.proc.cancel_token = Some(cancel_token.clone());
        self.proc.generation_id += 1;
        let generation_id = self.proc.generation_id;

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

        self.proc.last_file_backups.clear();
        self.chat
            .history
            .push(super::chat::resolve_prompt_message(&input));
        self.save_session();
        let cleaned_input = super::chat::clean_prompt_images(&input);
        self.chat.messages.push(MessageLine::user(cleaned_input));
        self.chat.messages.push(MessageLine::pending());
        self.proc.pending = Some(PendingTask::Generating);

        let queue_len = self.chat.message_queue.len();
        if queue_len > 0 {
            self.status = format!("Working... ({} in queue)", queue_len);
        } else {
            self.status = "Working...".to_owned();
        }

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.proc.cancel_token = Some(cancel_token.clone());
        self.proc.generation_id += 1;
        let generation_id = self.proc.generation_id;

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
        if !matches!(self.proc.pending, Some(PendingTask::Generating)) {
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

            let text = match part.get("text").and_then(|v| v.as_str()) {
                Some(t) if !t.is_empty() => t,
                _ => {
                    if is_thought {
                        self.chat.last_chunk_was_thought = true;
                    }
                    continue;
                }
            };

            if is_thought {
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
                } else if self
                    .chat
                    .messages
                    .last()
                    .is_none_or(|m| m.text != "Thinking...")
                {
                    self.chat
                        .messages
                        .push(MessageLine::assistant("Thinking...".to_owned()));
                }
                self.chat.last_chunk_was_thought = true;
                continue;
            }

            if self.chat.last_chunk_was_thought {
                if show_thoughts {
                    let clean_text = text.trim_start_matches('\n').trim_start_matches('\r');
                    self.append_to_chat_messages("Darwin", format!("\n\n{}", clean_text));
                } else if let Some(msg) = self.chat.messages.last_mut()
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
            } else {
                self.append_to_chat_messages("Darwin", text.to_owned());
            }
            self.chat.last_chunk_was_thought = false;
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
        self.proc.pending = None;
        self.status = "Ready".to_owned();
        self.save_session();
    }

    pub fn complete_stream(&mut self) -> Option<FunctionAction> {
        if !matches!(self.proc.pending, Some(PendingTask::Generating)) {
            self.chat.streaming_parts.clear();
            return None;
        }
        let parts = std::mem::take(&mut self.chat.streaming_parts);
        if parts.is_empty() {
            self.proc.pending = None;
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

            let is_blocked_by_plan = self.core.dev_mode == DevelopMode::Plan
                && (name == "sh"
                    || name == "write"
                    || name == "edit"
                    || name == "patch"
                    || name == "kill");

            let is_allowed_plan_file = if self.core.dev_mode == DevelopMode::Plan {
                let mut allowed = false;
                if name == "write" || name == "edit" || name == "patch" {
                    if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
                        allowed = Self::is_plan_file_allowed(path_str);
                    } else if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                        allowed = edits.iter().all(|e| {
                            e.get("path")
                                .and_then(|v| v.as_str())
                                .map(Self::is_plan_file_allowed)
                                .unwrap_or(false)
                        });
                    }
                }
                allowed
            } else {
                false
            };

            let auto_allowed = !is_blocked_by_plan
                && (permission == PermissionLevel::Chaos
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
                self.proc.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
                self.status = format!("Auto-executing {name}");
                self.start_function_execution(&name, &args);
                Some(FunctionAction::Execute {
                    name,
                    args,
                    config: self.chat.config.clone(),
                })
            } else if (permission == PermissionLevel::Safe
                && !is_allowed_plan_file
                && (name == "sh"
                    || name == "write"
                    || name == "edit"
                    || name == "patch"
                    || name == "kill"))
                || (self.core.dev_mode == DevelopMode::Plan
                    && is_blocked_by_plan
                    && !is_allowed_plan_file)
            {
                self.proc.pending = Some(PendingTask::Generating);
                let err_msg = if self.core.dev_mode == DevelopMode::Plan {
                    "Permission denied: Plan mode only allows editing .darwincode/plans/*.md"
                } else {
                    "Permission denied: restricted mode"
                };
                self.complete_function_execution(name, serde_json::json!({"error": err_msg}))
            } else {
                self.ui.confirm_scroll.set(0);
                self.proc.pending = Some(PendingTask::ConfirmFunction { name, args });
                self.status = "Action required".to_owned();
                None
            }
        } else {
            self.proc.pending = None;
            self.status = "Ready".to_owned();
            None
        }
    }

    pub fn open_setup(&mut self) {
        if self.is_busy() {
            return;
        }

        self.ui.setup = SetupState::from_config(&self.chat.config);
        self.ui.screen = Screen::Setup;
        self.status = "Edit settings. Press Enter to save or Esc to quit.".to_owned();
    }

    pub fn open_sessions(&mut self) {
        if self.is_busy() {
            return;
        }
        let cached = if let Ok(cache) = self.core.sessions_cache.lock() {
            cache.clone()
        } else {
            None
        };

        if let Some(list) = cached {
            self.ui.sessions.sessions = list;
            self.ui.sessions.selected = 0;
            self.ui.sessions.query = String::new();
            self.ui.screen = Screen::Sessions;
            self.status = "Select a session to resume. Enter to apply, Esc to cancel.".to_owned();
        } else {
            match super::session::list_saved_sessions() {
                Ok(list) => {
                    if let Ok(mut cache) = self.core.sessions_cache.lock() {
                        *cache = Some(list.clone());
                    }
                    self.ui.sessions.sessions = list;
                    self.ui.sessions.selected = 0;
                    self.ui.sessions.query = String::new();
                    self.ui.screen = Screen::Sessions;
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

        self.ui.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn busy_label(&self) -> Option<String> {
        let task = self.proc.pending.as_ref()?;
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
            super::chat::get_at_word_at_cursor(&self.chat.input, self.chat.cursor)
        {
            return super::chat::get_path_suggestions(&path_prefix);
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

        if (input.starts_with("/resume ") || input.starts_with("/resum"))
            && let Ok(cache) = self.core.sessions_cache.lock()
            && let Some(sessions) = cache.as_ref()
        {
            return sessions
                .iter()
                .map(|meta| CommandSuggestion {
                    name: format!("/resume {}", meta.id),
                    description: meta.snippet.clone(),
                })
                .filter(|s| s.name.starts_with(input))
                .collect();
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
        let active_idx = self
            .chat
            .suggestion_idx
            .min(suggestions.len().saturating_sub(1));
        if let Some(suggestion) = suggestions.into_iter().nth(active_idx) {
            if let Some((start_idx, _)) =
                super::chat::get_at_word_at_cursor(&self.chat.input, self.chat.cursor)
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

    pub fn cancel_models(&mut self) {
        self.ui.model_picker_open = false;
        self.ui.models.clear_query();
        self.status = "Ready".to_owned();
    }

    pub fn select_next_model(&mut self) {
        self.ui.models.select_next();
    }

    pub fn select_previous_model(&mut self) {
        self.ui.models.select_previous();
    }

    pub fn apply_selected_model(&mut self) {
        let Some(model) = self.ui.models.selected_model() else {
            self.status = "No model selected".to_owned();
            return;
        };

        self.chat.config.model = model.trim_start_matches("models/").to_owned();
        match self.chat.config.save() {
            Ok(()) => {
                self.status = format!("Model set to {}", self.chat.config.model);
                self.ui.model_picker_open = false;
                self.ui.models.clear_query();
            }
            Err(error) => {
                self.status = error.to_string();
            }
        }
    }

    pub fn cancel_themes(&mut self) {
        self.ui.theme_picker_open = false;
        self.ui.theme_picker.clear_query();
        self.status = "Ready".to_owned();
    }

    pub fn select_next_theme(&mut self) {
        self.ui.theme_picker.select_next();
    }

    pub fn select_previous_theme(&mut self) {
        self.ui.theme_picker.select_previous();
    }

    pub fn apply_selected_theme(&mut self) {
        let Some(theme) = self.ui.theme_picker.selected_theme() else {
            self.status = "No theme selected".to_owned();
            return;
        };

        self.chat.config.theme = theme;
        match self.chat.config.save() {
            Ok(()) => {
                self.status = format!("Theme set to {}", self.chat.config.theme.label());
                self.ui.theme_picker_open = false;
                self.ui.theme_picker.clear_query();
            }
            Err(error) => {
                self.status = error.to_string();
            }
        }
    }

    pub fn cancel_agents(&mut self) {
        self.ui.agent_picker_open = false;
        self.ui.agent_picker.clear_query();
        self.status = "Ready".to_owned();
    }

    pub fn select_next_agent(&mut self) {
        self.ui.agent_picker.select_next();
    }

    pub fn select_previous_agent(&mut self) {
        self.ui.agent_picker.select_previous();
    }

    pub fn apply_selected_agent(&mut self) {
        let Some((agent_id, display_name)) = self.ui.agent_picker.selected_agent() else {
            self.status = "No agent selected".to_owned();
            return;
        };

        self.core.active_agent = agent_id.clone();
        self.chat.config.active_agent = agent_id.clone();

        self.status = if agent_id.is_some() {
            format!("Agent set to {}", display_name)
        } else {
            "Agent cleared (Standard Agent)".to_owned()
        };

        if self.chat.messages.last().is_some_and(|m| m.pending) {
            self.chat.messages.pop();
        }
        self.chat.messages.push(MessageLine::info(format!(
            "Active agent set to: **{}**",
            display_name
        )));

        self.ui.agent_picker_open = false;
        self.ui.agent_picker.clear_query();
    }

    pub fn apply_permission_level(&mut self) -> Option<SubmitAction> {
        let options = PermissionPickerState::options();
        let mut ret = None;
        if let Some((label, _, level)) = options.get(self.ui.permissions.selected) {
            self.chat.config.permission_level = *level;
            if self.chat.messages.last().is_some_and(|m| m.pending) {
                self.chat.messages.pop();
            }
            self.chat.messages.push(MessageLine::info(format!(
                "Permission level set to **{label}**"
            )));
            let _ = self.chat.config.save();

            if let Some(PendingTask::ConfirmFunction { name, args }) = self.proc.pending.clone() {
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
                    self.proc.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
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
        self.ui.screen = Screen::Chat;
        self.status = "Ready".to_owned();
        ret
    }

    pub fn cancel_permissions(&mut self) {
        self.ui.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn cancel_sessions(&mut self) {
        self.ui.screen = Screen::Chat;
        self.status = "Ready".to_owned();
    }

    pub fn resume_session(&mut self, id: &str) -> Result<()> {
        let s = super::session::load_session(id)?;
        self.chat.session_id = s.id;
        self.chat.history = s.history;
        self.chat.messages = super::session::rebuild_messages_from_history(
            &self.chat.history,
            self.chat.config.show_thoughts,
        );
        self.chat.todos = super::session::rebuild_todos_from_history(&self.chat.history);
        self.status = format!("Resumed session: {}", id);
        Ok(())
    }

    pub fn save_session(&mut self) {
        if self.chat.session_id.starts_with("test_mock") {
            return;
        }
        let _ = super::session::save_session(&self.chat);

        if let Ok(mut cache) = self.core.sessions_cache.lock()
            && let Some(ref mut list) = *cache
        {
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

    pub fn apply_selected_session(&mut self) {
        if let Some(meta) = self.ui.sessions.selected_session() {
            match self.resume_session(&meta.id) {
                Ok(_) => {
                    self.ui.screen = Screen::Chat;
                }
                Err(e) => {
                    self.status = format!("Failed to load session: {e}");
                }
            }
        } else {
            self.ui.screen = Screen::Chat;
            self.status = "Ready".to_owned();
        }
    }

    pub fn answer_function_confirmation(&mut self, allow: bool) -> Option<FunctionAction> {
        let PendingTask::ConfirmFunction { name, args } = self.proc.pending.take()? else {
            return None;
        };

        if !allow {
            self.chat.messages.push(MessageLine::info(
                "Tool execution denied. Generation stopped.".to_owned(),
            ));
            self.chat.message_queue.clear();
            self.proc.pending = None;
            self.status = "Ready".to_owned();
            if self.chat.messages.last().is_some_and(|m| m.pending) {
                self.chat.messages.pop();
            }
            self.save_session();
            return None;
        }

        let is_blocked_by_plan = self.core.dev_mode == DevelopMode::Plan
            && (name == "sh"
                || name == "write"
                || name == "edit"
                || name == "patch"
                || name == "kill");

        let is_allowed_plan_file = if self.core.dev_mode == DevelopMode::Plan {
            let mut allowed = false;
            if name == "write" || name == "edit" || name == "patch" {
                if let Some(path_str) = args.get("path").and_then(|v| v.as_str()) {
                    allowed = Self::is_plan_file_allowed(path_str);
                } else if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                    allowed = edits.iter().all(|e| {
                        e.get("path")
                            .and_then(|v| v.as_str())
                            .map(Self::is_plan_file_allowed)
                            .unwrap_or(false)
                    });
                }
            }
            allowed
        } else {
            false
        };

        if is_blocked_by_plan && !is_allowed_plan_file {
            self.chat.messages.push(MessageLine::error(
                "Permission denied: Plan mode only allows editing .darwincode/plans/*.md"
                    .to_owned(),
            ));
            self.chat.message_queue.clear();
            self.proc.pending = None;
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

        self.proc.pending = Some(PendingTask::ExecutingFunction { name: name.clone() });
        self.status = format!("Executing {name}");
        self.start_function_execution(&name, &args);
        Some(FunctionAction::Execute {
            name,
            args,
            config: self.chat.config.clone(),
        })
    }

    pub fn is_plan_file_allowed(path_str: &str) -> bool {
        let path = std::path::Path::new(path_str);
        let is_md = path.extension().map(|ext| ext == "md").unwrap_or(false);
        if !is_md {
            return false;
        }
        if path_str.starts_with(".darwincode/plans/")
            || path_str.starts_with("./.darwincode/plans/")
        {
            return true;
        }
        if let Ok(current_dir) = std::env::current_dir() {
            let abs_allowed = current_dir.join(".darwincode/plans");
            if let Ok(abs_path) = std::path::Path::new(path_str).canonicalize()
                && let Ok(abs_allowed_canonical) = abs_allowed.canonicalize()
            {
                return abs_path.starts_with(abs_allowed_canonical);
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
                serde_json::from_value::<Vec<super::chat::TodoItem>>(todos_val.clone())
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

            let mut output = crate::app::file_ops::format_shell_output(
                stdout,
                stderr,
                error_field,
                false,
                is_aborted,
                is_running,
                None,
            );
            if output.is_empty() && !is_running {
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
            let summary = super::session::format_tool_summary(&name, &args, &response);
            if let Some(msg) = self.chat.messages.iter_mut().rev().find(|m| m.is_tool) {
                msg.text = summary;
                *msg.cached_wrapped.borrow_mut() = None;
            }
        }

        self.chat.messages.push(MessageLine::pending());
        self.proc.pending = Some(PendingTask::Generating);
        self.status = "Working...".to_owned();

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.proc.cancel_token = Some(cancel_token.clone());
        self.proc.generation_id += 1;
        let generation_id = self.proc.generation_id;

        Some(FunctionAction::ResumeGeneration(GenerationRequest {
            config: self.chat.config.clone(),
            history: self.chat.history.clone(),
            cancel_token,
            generation_id,
            dev_mode: self.dev_mode_label().to_owned(),
        }))
    }

    pub fn cancel_generation(&mut self) {
        if let Some(token) = self.proc.cancel_token.take() {
            token.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.proc.pending = None;
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
        if !self.proc.last_file_backups.is_empty() {
            for backup in &self.proc.last_file_backups {
                match &backup.original_content {
                    Some(content) => {
                        let _ = std::fs::write(&backup.path, content);
                    }
                    None => {
                        let _ = std::fs::remove_file(&backup.path);
                    }
                }
            }
            self.proc.last_file_backups.clear();
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
            if self.proc.last_file_backups.iter().any(|b| b.path == path) {
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
            self.proc.last_file_backups.push(FileBackup {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    #[test]
    fn test_app_dev_mode_toggle() {
        let mut app = App::new(Some(StoredConfig::default()));
        assert_eq!(app.dev_mode_label(), "Build");
        app.toggle_dev_mode();
        assert_eq!(app.dev_mode_label(), "Plan");
        app.toggle_dev_mode();
        assert_eq!(app.dev_mode_label(), "Build");
    }

    #[test]
    fn test_app_theme_picker_state() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.theme_picker_open = true;
        app.ui.theme_picker = ThemePickerState::new(&Theme::Auto);
        
        // select next theme
        app.select_next_theme();
        app.select_previous_theme();
        app.cancel_themes();
        assert!(!app.ui.theme_picker_open);
        assert_eq!(app.status, "Ready");
    }

    #[test]
    fn test_app_model_picker_state() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.model_picker_open = true;
        app.ui.models = ModelPickerState::new(vec!["models/gemini-2.0-flash".to_owned()], "models/gemini-2.0-flash");
        
        app.select_next_model();
        app.select_previous_model();
        app.cancel_models();
        assert!(!app.ui.model_picker_open);
        assert_eq!(app.status, "Ready");
    }

    #[test]
    fn test_app_agent_picker_state() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.agent_picker_open = true;
        app.ui.agent_picker = AgentPickerState::new(&None);
        
        app.select_next_agent();
        app.select_previous_agent();
        app.cancel_agents();
        assert!(!app.ui.agent_picker_open);
        assert_eq!(app.status, "Ready");
    }

    #[test]
    fn test_app_cancel_permissions() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.screen = Screen::Permissions;
        app.cancel_permissions();
        assert_eq!(app.ui.screen, Screen::Chat);
        assert_eq!(app.status, "Ready");
    }

    #[test]
    fn test_app_submit_empty_input() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.chat.input = "   ".to_owned();
        let action = app.submit_chat_input();
        assert!(action.is_none());
    }

    #[test]
    fn test_app_submit_command_help() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.chat.input = "/help".to_owned();
        let _action = app.submit_chat_input();
        // /help command clears the input buffer
        assert!(app.chat.input.is_empty());
    }

    #[test]
    fn test_app_queue_message_when_busy() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.proc.pending = Some(PendingTask::Generating);
        app.chat.input = "busy prompt".to_owned();
        let action = app.submit_chat_input();
        assert!(action.is_none());
        assert_eq!(app.chat.message_queue.len(), 1);
        assert_eq!(app.chat.message_queue[0], "busy prompt");
        assert!(app.chat.input.is_empty());
    }

    #[test]
    fn test_app_apply_theme() {
        let temp_dir = std::env::temp_dir().join(format!("darwin_test_{}", std::time::Instant::now().elapsed().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let mut config = StoredConfig::default();
        config.api_key = "dummy_key".to_owned();
        let mut app = App::new(Some(config));

        app.ui.theme_picker = ThemePickerState::new(&Theme::Auto);
        app.ui.theme_picker.selected = 1; // Dark
        app.ui.theme_picker_open = true;
        app.apply_selected_theme();
        assert_eq!(app.chat.config.theme, Theme::Dark);
        assert!(!app.ui.theme_picker_open);

        let _ = std::fs::remove_dir_all(&temp_dir);
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
    }

    #[test]
    fn test_app_apply_model() {
        let temp_dir = std::env::temp_dir().join(format!("darwin_test_{}", std::time::Instant::now().elapsed().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let mut config = StoredConfig::default();
        config.api_key = "dummy_key".to_owned();
        let mut app = App::new(Some(config));

        app.ui.models = ModelPickerState::new(vec!["models/gemini-2.0-flash".to_owned()], "models/gemini-2.0-flash");
        app.ui.models.selected = 0;
        app.ui.model_picker_open = true;
        app.apply_selected_model();
        assert_eq!(app.chat.config.model, "gemini-2.0-flash");
        assert!(!app.ui.model_picker_open);

        let _ = std::fs::remove_dir_all(&temp_dir);
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
    }

    #[test]
    fn test_app_apply_agent() {
        let mut app = App::new(Some(StoredConfig::default()));
        app.ui.agent_picker = AgentPickerState::new(&None);
        app.ui.agent_picker_open = true;
        app.apply_selected_agent();
        assert_eq!(app.chat.config.active_agent, None);
        assert!(!app.ui.agent_picker_open);
    }

    #[test]
    fn test_app_apply_permission_level() {
        let temp_dir = std::env::temp_dir().join(format!("darwin_test_{}", std::time::Instant::now().elapsed().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let mut config = StoredConfig::default();
        config.api_key = "dummy_key".to_owned();
        let mut app = App::new(Some(config));

        app.ui.permissions.selected = 2; // Chaos
        app.apply_permission_level();
        assert_eq!(app.chat.config.permission_level, PermissionLevel::Chaos);

        let _ = std::fs::remove_dir_all(&temp_dir);
        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
    }
}
