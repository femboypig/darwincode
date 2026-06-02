use crate::config::{PermissionLevel, StoredConfig, Theme};
use crate::gemini::{ChatMessage, Part};

#[derive(Debug)]
pub struct ChatState {
    pub config: StoredConfig,
    pub history: Vec<ChatMessage>,
    pub messages: Vec<MessageLine>,
    pub input: String,
    pub cursor: usize,
    pub scroll: u16,
    pub input_scroll: u16,
    pub streaming_parts: Vec<Part>,
    pub session_id: String,
    pub message_queue: Vec<String>,
    pub input_history: Vec<(String, usize)>,
    pub redo_history: Vec<(String, usize)>,
    pub sent_history_index: Option<usize>,
    pub input_draft: String,
    pub shell_focused: bool,
}

impl ChatState {
    pub fn new(config: StoredConfig) -> Self {
        let session_id = format!(
            "session_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        Self {
            config,
            history: Vec::new(),
            messages: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll: 0,
            input_scroll: 0,
            streaming_parts: Vec::new(),
            session_id,
            message_queue: Vec::new(),
            input_history: Vec::new(),
            redo_history: Vec::new(),
            sent_history_index: None,
            input_draft: String::new(),
            shell_focused: false,
        }
    }

    pub fn get_user_prompts(&self) -> Vec<String> {
        self.history.iter()
            .filter(|m| m.role == "user")
            .filter_map(|m| {
                m.parts.first()
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_owned())
            })
            .collect()
    }

    pub fn navigate_history_up(&mut self) {
        let prompts = self.get_user_prompts();
        if prompts.is_empty() {
            return;
        }

        let next_index = match self.sent_history_index {
            None => {
                self.input_draft = self.input.clone();
                prompts.len().saturating_sub(1)
            }
            Some(idx) => idx.saturating_sub(1),
        };

        if next_index < prompts.len() {
            self.input = prompts[next_index].clone();
            self.cursor = self.input.chars().count();
            self.sent_history_index = Some(next_index);
        }
    }

    pub fn navigate_history_down(&mut self) {
        let prompts = self.get_user_prompts();
        if prompts.is_empty() {
            return;
        }

        if let Some(idx) = self.sent_history_index {
            let next_index = idx + 1;
            if next_index >= prompts.len() {
                self.input = self.input_draft.clone();
                self.cursor = self.input.chars().count();
                self.sent_history_index = None;
            } else {
                self.input = prompts[next_index].clone();
                self.cursor = self.input.chars().count();
                self.sent_history_index = Some(next_index);
            }
        }
    }

    pub fn save_history(&mut self) {
        let current = (self.input.clone(), self.cursor);
        if self.input_history.last() != Some(&current) {
            self.input_history.push(current);
            if self.input_history.len() > 100 {
                self.input_history.remove(0);
            }
        }
        self.redo_history.clear();
    }

    pub fn undo(&mut self) {
        if let Some((prev_input, prev_cursor)) = self.input_history.pop() {
            self.redo_history.push((self.input.clone(), self.cursor));
            self.input = prev_input;
            self.cursor = prev_cursor;
        }
    }

    pub fn redo(&mut self) {
        if let Some((next_input, next_cursor)) = self.redo_history.pop() {
            self.input_history.push((self.input.clone(), self.cursor));
            self.input = next_input;
            self.cursor = next_cursor;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.save_history();
        let byte_idx = self.input.char_indices().nth(self.cursor).map(|(i, _)| i).unwrap_or(self.input.len());
        self.input.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn insert_text(&mut self, text: &str) {
        self.save_history();
        let byte_idx = self.input.char_indices().nth(self.cursor).map(|(i, _)| i).unwrap_or(self.input.len());
        self.input.insert_str(byte_idx, text);
        self.cursor += text.chars().count();
    }

    pub fn remove_char(&mut self) {
        if self.cursor > 0 {
            self.save_history();
            self.cursor -= 1;
            let byte_idx = self.input.char_indices().nth(self.cursor).map(|(i, _)| i).unwrap();
            self.input.remove(byte_idx);
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor < self.input.chars().count() {
            self.save_history();
            let byte_idx = self.input.char_indices().nth(self.cursor).map(|(i, _)| i).unwrap();
            self.input.remove(byte_idx);
        }
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.input.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn move_cursor_up(&mut self) {
        let text = &self.input;
        let mut lines = vec![0];
        for (i, c) in text.chars().enumerate() {
            if c == '\n' { lines.push(i + 1); }
        }
        
        let mut current_line = 0;
        for (i, &start) in lines.iter().enumerate() {
            if self.cursor >= start { current_line = i; }
        }
        
        if current_line > 0 {
            let col = self.cursor - lines[current_line];
            let prev_line_start = lines[current_line - 1];
            let prev_line_end = lines[current_line] - 1;
            let prev_line_len = prev_line_end - prev_line_start;
            self.cursor = prev_line_start + col.min(prev_line_len);
        } else {
            self.cursor = 0;
        }
    }

    pub fn move_cursor_down(&mut self) {
        let text = &self.input;
        let mut lines = vec![0];
        for (i, c) in text.chars().enumerate() {
            if c == '\n' { lines.push(i + 1); }
        }
        let end_idx = text.chars().count();
        lines.push(end_idx + 1);
        
        let mut current_line = 0;
        for (i, &start) in lines.iter().enumerate() {
            if self.cursor >= start && self.cursor < lines.get(i+1).copied().unwrap_or(end_idx + 1) {
                current_line = i;
                break;
            }
        }
        
        if current_line + 1 < lines.len() - 1 {
            let col = self.cursor - lines[current_line];
            let next_line_start = lines[current_line + 1];
            let next_line_end = lines[current_line + 2] - 1;
            let next_line_len = next_line_end.saturating_sub(next_line_start);
            self.cursor = (next_line_start + col).min(next_line_start + next_line_len).min(end_idx);
        } else {
            self.cursor = end_idx;
        }
    }

    pub fn move_cursor_start(&mut self) {
        let text = &self.input;
        let mut start_of_line = 0;
        for (i, c) in text.chars().enumerate() {
            if i == self.cursor { break; }
            if c == '\n' { start_of_line = i + 1; }
        }
        self.cursor = start_of_line;
    }

    pub fn move_cursor_end(&mut self) {
        let text = &self.input;
        let mut end_of_line = text.chars().count();
        for (i, c) in text.chars().enumerate().skip(self.cursor) {
            if c == '\n' {
                end_of_line = i;
                break;
            }
        }
        self.cursor = end_of_line;
    }
}

#[derive(Debug)]
pub struct MessageLine {
    pub author: &'static str,
    pub text: String,
    pub pending: bool,
    pub is_shell: bool,
    pub shell_success: bool,
    pub shell_cmd: String,
    pub is_tool: bool,
    pub cached_wrapped: std::cell::RefCell<Option<(usize, Theme, Vec<ratatui::text::Line<'static>>)>>,
}

impl MessageLine {
    pub fn error(text: String) -> Self {
        Self {
            author: "System",
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn user(text: String) -> Self {
        Self {
            author: "You",
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn assistant(text: String) -> Self {
        Self {
            author: "Darwin",
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn info(text: String) -> Self {
        Self {
            author: "Info",
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn tool(text: String) -> Self {
        Self {
            author: "Darwin",
            text,
            pending: false,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: true,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn shell(cmd: String, output: String, success: bool) -> Self {
        Self {
            author: "Shell",
            text: output,
            pending: false,
            is_shell: true,
            shell_success: success,
            shell_cmd: cmd,
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn pending() -> Self {
        Self {
            author: "Darwin",
            text: String::new(),
            pending: true,
            is_shell: false,
            shell_success: false,
            shell_cmd: String::new(),
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommandSuggestion {
    pub name: String,
    pub description: String,
}

pub enum ChatCommand {
    Settings,
    Exit,
    Models,
    Permissions(Option<PermissionLevel>),
    Resume(Option<String>),
    Clear,
    New,
    History,
    Undo,
    Help,
    Unknown(String),
}

impl ChatCommand {
    pub fn parse(input: &str) -> Option<Self> {
        let mut parts = input.split_whitespace();
        let command = parts.next()?;
        if !command.starts_with('/') {
            return None;
        }

        Some(match command {
            "/settings" => Self::Settings,
            "/exit" | "/quit" => Self::Exit,
            "/models" => Self::Models,
            "/permissions" => {
                let arg = parts.next().map(|s| s.to_lowercase());
                let level = match arg.as_deref() {
                    Some("safe") => Some(PermissionLevel::Safe),
                    Some("guardian") => Some(PermissionLevel::Guardian),
                    Some("chaos") => Some(PermissionLevel::Chaos),
                    _ => None,
                };
                Self::Permissions(level)
            }
            "/resume" => {
                let arg = parts.next().map(|s| s.to_owned());
                Self::Resume(arg)
            }
            "/clear" => Self::Clear,
            "/new" => Self::New,
            "/history" => Self::History,
            "/undo" => Self::Undo,
            "/help" => Self::Help,
            value => Self::Unknown(value.to_owned()),
        })
    }

    pub fn suggestions() -> Vec<CommandSuggestion> {
        vec![
            CommandSuggestion {
                name: "/settings".to_owned(),
                description: "Open settings".to_owned(),
            },
            CommandSuggestion {
                name: "/models".to_owned(),
                description: "List Gemini models".to_owned(),
            },
            CommandSuggestion {
                name: "/permissions".to_owned(),
                description: "Cycle permission levels".to_owned(),
            },
            CommandSuggestion {
                name: "/resume".to_owned(),
                description: "Resume saved chat sessions".to_owned(),
            },
            CommandSuggestion {
                name: "/new".to_owned(),
                description: "Start a new chat session".to_owned(),
            },
            CommandSuggestion {
                name: "/clear".to_owned(),
                description: "Clear current chat history".to_owned(),
            },
            CommandSuggestion {
                name: "/history".to_owned(),
                description: "List history in chat".to_owned(),
            },
            CommandSuggestion {
                name: "/undo".to_owned(),
                description: "Revert all file changes made in the last prompt".to_owned(),
            },
            CommandSuggestion {
                name: "/help".to_owned(),
                description: "Show available commands".to_owned(),
            },
            CommandSuggestion {
                name: "/exit".to_owned(),
                description: "Quit darwincode".to_owned(),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_history_navigation() {
        let config = StoredConfig::default();
        let mut chat = ChatState::new(config);
        
        chat.history.push(ChatMessage::user("first prompt".to_owned()));
        chat.history.push(ChatMessage::user("second prompt".to_owned()));
        
        chat.input = "current draft".to_owned();
        
        // Navigate up
        chat.navigate_history_up();
        assert_eq!(chat.input, "second prompt");
        assert_eq!(chat.sent_history_index, Some(1));
        assert_eq!(chat.input_draft, "current draft");
        
        // Navigate up again
        chat.navigate_history_up();
        assert_eq!(chat.input, "first prompt");
        assert_eq!(chat.sent_history_index, Some(0));
        
        // Navigate up at top (should stay at first prompt)
        chat.navigate_history_up();
        assert_eq!(chat.input, "first prompt");
        assert_eq!(chat.sent_history_index, Some(0));
        
        // Navigate down
        chat.navigate_history_down();
        assert_eq!(chat.input, "second prompt");
        assert_eq!(chat.sent_history_index, Some(1));
        
        // Navigate down to restore draft
        chat.navigate_history_down();
        assert_eq!(chat.input, "current draft");
        assert_eq!(chat.sent_history_index, None);
    }

    #[test]
    fn test_undo_command_parsing() {
        let parsed = ChatCommand::parse("/undo");
        assert!(matches!(parsed, Some(ChatCommand::Undo)));

        let suggestions = ChatCommand::suggestions();
        assert!(suggestions.iter().any(|s| s.name == "/undo"));
    }
}
