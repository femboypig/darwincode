use crate::api::{ChatMessage, Part};
use crate::config::{StoredConfig, Theme};
use super::todos::TodoItem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSelection {
    pub msg_idx: usize,
    pub start_line: usize, // relative to the message's wrapped lines
    pub start_col: usize,  // relative to the text of the line (excluding margin)
    pub end_line: usize,
    pub end_col: usize,
}

impl MessageSelection {
    pub fn normalized(&self) -> (usize, usize, usize, usize) {
        if self.start_line < self.end_line {
            (self.start_line, self.start_col, self.end_line, self.end_col)
        } else if self.start_line > self.end_line {
            (self.end_line, self.end_col, self.start_line, self.start_col)
        } else {
            // Same line
            if self.start_col <= self.end_col {
                (self.start_line, self.start_col, self.end_line, self.end_col)
            } else {
                (self.start_line, self.end_col, self.end_line, self.start_col)
            }
        }
    }
}

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
    pub focused_shell_session_id: Option<String>,
    pub focused_shell_pid: Option<u32>,
    pub last_chunk_was_thought: bool,
    pub messages_area: std::cell::Cell<Option<ratatui::layout::Rect>>,
    pub mode_area: std::cell::Cell<Option<ratatui::layout::Rect>>,
    pub model_area: std::cell::Cell<Option<ratatui::layout::Rect>>,
    pub message_line_ranges: std::cell::RefCell<Vec<(usize, usize, usize)>>,
    pub todos: Vec<TodoItem>,
    pub suggestion_idx: usize,
    pub selection: Option<MessageSelection>,
    pub last_mouse_drag_pos: Option<(u16, u16)>,
}

impl ChatState {
    pub fn new(config: StoredConfig) -> Self {
        // Generate secure 128-bit random session identifier
        let mut rng_bytes = [0u8; 16];
        rand::fill(&mut rng_bytes);
        let session_id = format!(
            "session_{}",
            rng_bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
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
            focused_shell_session_id: None,
            focused_shell_pid: None,
            last_chunk_was_thought: false,
            messages_area: std::cell::Cell::new(None),
            mode_area: std::cell::Cell::new(None),
            model_area: std::cell::Cell::new(None),
            message_line_ranges: std::cell::RefCell::new(Vec::new()),
            todos: Vec::new(),
            suggestion_idx: 0,
            selection: None,
            last_mouse_drag_pos: None,
        }
    }

    pub fn get_user_prompts(&self) -> Vec<String> {
        self.history
            .iter()
            .filter(|m| m.role == "user")
            .filter_map(|m| {
                m.parts
                    .first()
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
            self.suggestion_idx = 0;
        }
    }

    pub fn redo(&mut self) {
        if let Some((next_input, next_cursor)) = self.redo_history.pop() {
            self.input_history.push((self.input.clone(), self.cursor));
            self.input = next_input;
            self.cursor = next_cursor;
            self.suggestion_idx = 0;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.save_history();
        let byte_idx = self
            .input
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len());
        self.input.insert(byte_idx, c);
        self.cursor += 1;
        self.suggestion_idx = 0;
    }

    pub fn insert_text(&mut self, text: &str) {
        self.save_history();
        let byte_idx = self
            .input
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len());
        self.input.insert_str(byte_idx, text);
        self.cursor += text.chars().count();
        self.suggestion_idx = 0;
    }

    pub fn remove_char(&mut self) {
        if self.cursor > 0 {
            self.save_history();
            self.cursor -= 1;
            let byte_idx = self
                .input
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap();
            self.input.remove(byte_idx);
            self.suggestion_idx = 0;
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor < self.input.chars().count() {
            self.save_history();
            let byte_idx = self
                .input
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap();
            self.input.remove(byte_idx);
            self.suggestion_idx = 0;
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
            if c == '\n' {
                lines.push(i + 1);
            }
        }

        let mut current_line = 0;
        for (i, &start) in lines.iter().enumerate() {
            if self.cursor >= start {
                current_line = i;
            }
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
            if c == '\n' {
                lines.push(i + 1);
            }
        }
        let end_idx = text.chars().count();
        lines.push(end_idx + 1);

        let mut current_line = 0;
        for (i, &start) in lines.iter().enumerate() {
            if self.cursor >= start
                && self.cursor < lines.get(i + 1).copied().unwrap_or(end_idx + 1)
            {
                current_line = i;
                break;
            }
        }

        if current_line + 1 < lines.len() - 1 {
            let col = self.cursor - lines[current_line];
            let next_line_start = lines[current_line + 1];
            let next_line_end = lines[current_line + 2] - 1;
            let next_line_len = next_line_end.saturating_sub(next_line_start);
            self.cursor = (next_line_start + col)
                .min(next_line_start + next_line_len)
                .min(end_idx);
        } else {
            self.cursor = end_idx;
        }
    }

    pub fn move_cursor_start(&mut self) {
        let text = &self.input;
        let mut start_of_line = 0;
        for (i, c) in text.chars().enumerate() {
            if i == self.cursor {
                break;
            }
            if c == '\n' {
                start_of_line = i + 1;
            }
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
    pub shell_pid: Option<u32>,
    pub shell_session_id: Option<String>,
    pub is_tool: bool,
    pub cached_wrapped:
        std::cell::RefCell<Option<(usize, Theme, Vec<ratatui::text::Line<'static>>)>>,
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
            shell_pid: None,
            shell_session_id: None,
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
            shell_pid: None,
            shell_session_id: None,
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
            shell_pid: None,
            shell_session_id: None,
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
            shell_pid: None,
            shell_session_id: None,
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
            shell_pid: None,
            shell_session_id: None,
            is_tool: true,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }

    pub fn shell(
        cmd: String,
        output: String,
        success: bool,
        shell_session_id: Option<String>,
    ) -> Self {
        Self {
            author: "Shell",
            text: output,
            pending: false,
            is_shell: true,
            shell_success: success,
            shell_cmd: cmd,
            shell_pid: None,
            shell_session_id,
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
            shell_pid: None,
            shell_session_id: None,
            is_tool: false,
            cached_wrapped: std::cell::RefCell::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_history_navigation() {
        let config = StoredConfig::default();
        let mut chat = ChatState::new(config);

        chat.history
            .push(crate::api::ChatMessage::user("first prompt".to_owned()));
        chat.history
            .push(crate::api::ChatMessage::user("second prompt".to_owned()));

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
}
