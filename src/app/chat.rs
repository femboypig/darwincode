use crate::api::{ChatMessage, Part};
use crate::config::{PermissionLevel, StoredConfig, Theme};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct TodoItem {
    pub content: String,
    pub status: String,   // pending | in_progress | completed | cancelled
    pub priority: String, // high | medium | low
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
            focused_shell_session_id: None,
            focused_shell_pid: None,
            last_chunk_was_thought: false,
            messages_area: std::cell::Cell::new(None),
            mode_area: std::cell::Cell::new(None),
            model_area: std::cell::Cell::new(None),
            message_line_ranges: std::cell::RefCell::new(Vec::new()),
            todos: Vec::new(),
            suggestion_idx: 0,
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
    Shell(Option<String>),
    Help,
    Plan,
    Build,
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
            "/shell" => {
                let arg = parts.next().map(|s| s.to_owned());
                Self::Shell(arg)
            }
            "/clear" => Self::Clear,
            "/new" => Self::New,
            "/history" => Self::History,
            "/undo" => Self::Undo,
            "/help" => Self::Help,
            "/plan" => Self::Plan,
            "/build" => Self::Build,
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
                description: "List available models".to_owned(),
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
                name: "/shell".to_owned(),
                description: "List or focus active shell sessions".to_owned(),
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
                name: "/plan".to_owned(),
                description: "Switch to Plan mode (read-only for workspace files)".to_owned(),
            },
            CommandSuggestion {
                name: "/build".to_owned(),
                description: "Switch to Build mode (full tools access)".to_owned(),
            },
            CommandSuggestion {
                name: "/exit".to_owned(),
                description: "Quit darwincode".to_owned(),
            },
        ]
    }
}

pub fn extract_paths(text: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((idx, c)) = chars.next() {
        if c == '@' {
            // Check if character before is alphanumeric
            if idx > 0 {
                let prev_char = text[..idx].chars().next_back().unwrap();
                if prev_char.is_alphanumeric() {
                    continue;
                }
            }

            // Peek at the next character
            if let Some(&(_, next_c)) = chars.peek() {
                if next_c == '"' {
                    chars.next(); // consume '"'
                    let mut path = String::new();
                    let mut found_end = false;
                    for (_, qc) in chars.by_ref() {
                        if qc == '"' {
                            found_end = true;
                            break;
                        }
                        path.push(qc);
                    }
                    if found_end {
                        results.push((format!("@\"{}\"", path), path));
                    }
                    continue;
                } else if next_c == '\'' {
                    chars.next(); // consume '\''
                    let mut path = String::new();
                    let mut found_end = false;
                    for (_, qc) in chars.by_ref() {
                        if qc == '\'' {
                            found_end = true;
                            break;
                        }
                        path.push(qc);
                    }
                    if found_end {
                        results.push((format!("@'{}'", path), path));
                    }
                    continue;
                }
            }

            // Scan non-whitespace
            let mut raw_path = String::new();
            while let Some(&(_, wc)) = chars.peek() {
                if wc.is_whitespace() {
                    break;
                }
                raw_path.push(wc);
                chars.next();
            }

            if !raw_path.is_empty() {
                // We have a raw path. Let's find the best version by trimming trailing punctuation.
                let mut clean_path = raw_path.clone();
                while let Some(last_c) = clean_path.chars().next_back() {
                    if last_c == '.'
                        || last_c == ','
                        || last_c == ';'
                        || last_c == ':'
                        || last_c == '?'
                        || last_c == '!'
                        || last_c == ')'
                        || last_c == ']'
                        || last_c == '}'
                        || last_c == '>'
                    {
                        clean_path.pop();
                    } else {
                        break;
                    }
                }

                // Try to resolve both
                let match_raw = std::path::Path::new(&raw_path).exists()
                    || std::env::current_dir()
                        .map(|d| d.join(&raw_path).exists())
                        .unwrap_or(false);

                let match_clean = std::path::Path::new(&clean_path).exists()
                    || std::env::current_dir()
                        .map(|d| d.join(&clean_path).exists())
                        .unwrap_or(false);

                if match_clean && !match_raw {
                    results.push((format!("@{}", clean_path), clean_path));
                } else {
                    results.push((format!("@{}", raw_path), raw_path));
                }
            }
        }
    }
    results
}

pub fn resolve_home(path_str: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(path_str);
    if let Ok(striped) = path.strip_prefix("~")
        && let Some(home) = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
    {
        return home.join(striped);
    }
    path.to_path_buf()
}

pub fn resolve_path(path_str: &str) -> std::path::PathBuf {
    let resolved_home = resolve_home(path_str);
    if resolved_home.is_absolute() {
        resolved_home
    } else {
        let workspace_path = std::env::current_dir()
            .unwrap_or_default()
            .join(&resolved_home);
        if workspace_path.exists() {
            workspace_path
        } else if let Ok(pasted_dir) = crate::tui::events::common::pasted_images_dir() {
            let pasted_path = pasted_dir.join(path_str);
            if pasted_path.exists() {
                pasted_path
            } else {
                workspace_path
            }
        } else {
            workspace_path
        }
    }
}

pub fn list_directory_contents(dir_path: &std::path::Path) -> String {
    let mut result = String::new();
    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(dir_path).unwrap_or(&path);
            if path.is_dir() {
                result.push_str(&format!("  [Dir]  {}\n", rel.display()));
            } else {
                result.push_str(&format!("  [File] {}\n", rel.display()));
            }
        }
    }
    result
}

pub fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < data.len() {
        let chunk = &data[i..std::cmp::min(i + 3, data.len())];
        let val = match chunk.len() {
            3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
            2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
            1 => (chunk[0] as u32) << 16,
            _ => 0,
        };
        result.push(CHARSET[((val >> 18) & 63) as usize] as char);
        result.push(CHARSET[((val >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[((val >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(val & 63) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

pub fn clean_prompt_images(input: &str) -> String {
    let mut cleaned = input.to_owned();
    let refs = extract_paths(input);
    for (raw_ref, path_str) in refs {
        let resolved = resolve_path(&path_str);
        if resolved.exists() && resolved.is_file() {
            let ext = resolved
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if (ext == "png"
                || ext == "jpg"
                || ext == "jpeg"
                || ext == "webp"
                || ext == "gif"
                || ext == "bmp")
                && let Some(filename) = resolved.file_name().and_then(|n| n.to_str())
            {
                cleaned = cleaned.replace(&raw_ref, &format!("[Image: {}]", filename));
            }
        }
    }
    cleaned
}

pub fn resolve_prompt_message(input: &str) -> ChatMessage {
    let refs = extract_paths(input);
    let cleaned_input = clean_prompt_images(input);
    let mut text_parts = vec![cleaned_input];
    let mut parts = Vec::new();

    for (_, path_str) in refs {
        let resolved = resolve_path(&path_str);
        if resolved.exists() {
            if resolved.is_file() {
                let ext = resolved
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ext == "png"
                    || ext == "jpg"
                    || ext == "jpeg"
                    || ext == "webp"
                    || ext == "gif"
                    || ext == "bmp"
                {
                    if let Ok(bytes) = std::fs::read(&resolved) {
                        let base64_data = base64_encode(&bytes);
                        let mime_type = match ext.as_str() {
                            "png" => "image/png",
                            "jpg" | "jpeg" => "image/jpeg",
                            "webp" => "image/webp",
                            "gif" => "image/gif",
                            "bmp" => "image/bmp",
                            _ => "image/png",
                        };
                        parts.push(serde_json::json!({
                            "inlineData": {
                                "mimeType": mime_type,
                                "data": base64_data
                            }
                        }));
                    }
                } else {
                    if let Ok(content) = std::fs::read_to_string(&resolved) {
                        text_parts.push(format!(
                            "\n\n--- File: {} ---\n{}\n-----------------",
                            path_str, content
                        ));
                    }
                }
            } else if resolved.is_dir() {
                let contents = list_directory_contents(&resolved);
                text_parts.push(format!(
                    "\n\n--- Directory: {} ---\n{}\n----------------------",
                    path_str, contents
                ));
            }
        }
    }

    let combined_text = text_parts.join("");
    let mut final_parts = vec![serde_json::json!({ "text": combined_text })];
    final_parts.extend(parts);

    ChatMessage {
        role: "user".to_owned(),
        parts: final_parts,
    }
}

pub fn get_at_word_at_cursor(input: &str, cursor_char_idx: usize) -> Option<(usize, String)> {
    let char_indices: Vec<(usize, char)> = input.char_indices().collect();
    if cursor_char_idx > char_indices.len() {
        return None;
    }

    let mut start_idx = cursor_char_idx;
    while start_idx > 0 {
        let prev_char = char_indices[start_idx - 1].1;
        if prev_char.is_whitespace() {
            break;
        }
        start_idx -= 1;
    }

    if start_idx < char_indices.len() {
        let word_chars: Vec<char> = char_indices[start_idx..cursor_char_idx]
            .iter()
            .map(|&(_, c)| c)
            .collect();
        if !word_chars.is_empty() && word_chars[0] == '@' {
            let path_prefix: String = word_chars[1..].iter().collect();
            return Some((start_idx, path_prefix));
        }
    }
    None
}

pub fn get_path_suggestions(path_prefix: &str) -> Vec<CommandSuggestion> {
    let mut parent_dir_str = ".";
    let mut file_prefix = path_prefix;

    if let Some(pos) = path_prefix.rfind('/') {
        parent_dir_str = &path_prefix[..=pos];
        file_prefix = &path_prefix[pos + 1..];
    } else if let Some(pos) = path_prefix.rfind('\\') {
        parent_dir_str = &path_prefix[..=pos];
        file_prefix = &path_prefix[pos + 1..];
    }

    let resolved_parent = if parent_dir_str == "." {
        std::env::current_dir().unwrap_or_default()
    } else {
        let trimmed = parent_dir_str.trim_end_matches('/').trim_end_matches('\\');
        resolve_path(trimmed)
    };

    let mut suggestions = Vec::new();
    if resolved_parent.is_dir()
        && let Ok(entries) = std::fs::read_dir(&resolved_parent)
    {
        let mut entries_vec: Vec<_> = entries.flatten().collect();
        entries_vec.sort_by_key(|e| e.file_name());

        for entry in entries_vec {
            if let Some(name) = entry.file_name().to_str()
                && name.to_lowercase().starts_with(&file_prefix.to_lowercase())
            {
                if name.starts_with('.') && !file_prefix.starts_with('.') {
                    continue;
                }

                let is_dir = entry.path().is_dir();
                let path_name = if parent_dir_str == "." {
                    if is_dir {
                        format!("{}/", name)
                    } else {
                        name.to_owned()
                    }
                } else {
                    if is_dir {
                        format!("{}{}/", parent_dir_str, name)
                    } else {
                        format!("{}{}", parent_dir_str, name)
                    }
                };

                let desc = if is_dir {
                    "Directory".to_owned()
                } else {
                    let ext = entry
                        .path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if ext == "png"
                        || ext == "jpg"
                        || ext == "jpeg"
                        || ext == "webp"
                        || ext == "gif"
                        || ext == "bmp"
                    {
                        "Image File".to_owned()
                    } else {
                        "File".to_owned()
                    }
                };

                suggestions.push(CommandSuggestion {
                    name: format!("@{}", path_name),
                    description: desc,
                });
            }
        }
    }

    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_history_navigation() {
        let config = StoredConfig::default();
        let mut chat = ChatState::new(config);

        chat.history
            .push(ChatMessage::user("first prompt".to_owned()));
        chat.history
            .push(ChatMessage::user("second prompt".to_owned()));

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

        let parsed_plan = ChatCommand::parse("/plan");
        assert!(matches!(parsed_plan, Some(ChatCommand::Plan)));

        let parsed_build = ChatCommand::parse("/build");
        assert!(matches!(parsed_build, Some(ChatCommand::Build)));

        let suggestions = ChatCommand::suggestions();
        assert!(suggestions.iter().any(|s| s.name == "/undo"));
        assert!(suggestions.iter().any(|s| s.name == "/plan"));
        assert!(suggestions.iter().any(|s| s.name == "/build"));
    }

    #[test]
    fn test_extract_paths() {
        let text = "Please read @src/main.rs and check @\"assets/logo.png\" or @'some file.txt' or user@host.com";
        let paths = extract_paths(text);
        assert_eq!(paths.len(), 3);
        assert_eq!(
            paths[0],
            ("@src/main.rs".to_owned(), "src/main.rs".to_owned())
        );
        assert_eq!(
            paths[1],
            (
                "@\"assets/logo.png\"".to_owned(),
                "assets/logo.png".to_owned()
            )
        );
        assert_eq!(
            paths[2],
            ("@'some file.txt'".to_owned(), "some file.txt".to_owned())
        );
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn test_resolve_prompt_message() {
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_test_{}",
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("test.txt");
        std::fs::write(&file_path, "Hello from test file!").unwrap();

        let path_str = file_path.to_str().unwrap();
        let prompt = format!("Check this file: @{}", path_str);

        let resolved = resolve_prompt_message(&prompt);
        assert_eq!(resolved.parts.len(), 1);
        let text_part = resolved.parts[0].get("text").unwrap().as_str().unwrap();
        assert!(text_part.contains("Check this file:"));
        assert!(text_part.contains("--- File:"));
        assert!(text_part.contains("Hello from test file!"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_clean_prompt_images() {
        let temp_dir = std::env::temp_dir().join(format!(
            "darwin_test_img_{}",
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let img_path = temp_dir.join("test_image.png");
        std::fs::write(&img_path, b"dummy png bytes").unwrap();

        let path_str = img_path.to_str().unwrap();
        let prompt = format!("Here is the image: @{} and some text.", path_str);

        let cleaned = clean_prompt_images(&prompt);
        assert_eq!(
            cleaned,
            "Here is the image: [Image: test_image.png] and some text."
        );

        let resolved = resolve_prompt_message(&prompt);
        assert_eq!(resolved.parts.len(), 2);

        let text_part = resolved.parts[0].get("text").unwrap().as_str().unwrap();
        assert_eq!(
            text_part,
            "Here is the image: [Image: test_image.png] and some text."
        );

        let inline_data = resolved.parts[1].get("inlineData").unwrap();
        assert_eq!(
            inline_data.get("mimeType").unwrap().as_str().unwrap(),
            "image/png"
        );
        assert_eq!(
            inline_data.get("data").unwrap().as_str().unwrap(),
            &base64_encode(b"dummy png bytes")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_at_word_at_cursor() {
        assert_eq!(
            get_at_word_at_cursor("hello @src/m", 12),
            Some((6, "src/m".to_owned()))
        );
        assert_eq!(get_at_word_at_cursor("hello @src/m", 5), None);
        assert_eq!(
            get_at_word_at_cursor("hello @", 7),
            Some((6, "".to_owned()))
        );
    }

    #[test]
    fn test_todo_lifecycle_validation() {
        use crate::app::App;

        // Create app with default config
        let mut app = App::new(Some(StoredConfig::default()));
        app.chat.session_id = "test_mock_todo_validation".to_owned();

        // Case 1: Initial list (one pending, one in_progress)
        app.chat.history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "todo",
                    "args": {
                        "todos": [
                            { "content": "Task 1", "status": "pending", "priority": "high" },
                            { "content": "Task 2", "status": "in_progress", "priority": "medium" }
                        ]
                    }
                }
            })],
        });

        app.complete_function_execution("todo".to_string(), serde_json::json!({ "success": true }));
        assert_eq!(app.chat.todos.len(), 2);
        assert_eq!(app.chat.todos[0].content, "Task 1");
        assert_eq!(app.chat.todos[0].status, "pending");
        assert_eq!(app.chat.todos[1].content, "Task 2");
        assert_eq!(app.chat.todos[1].status, "in_progress");

        // Case 2: Transition pending -> completed directly (validation is relaxed, should succeed)
        app.chat.history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "todo",
                    "args": {
                        "todos": [
                            { "content": "Task 1", "status": "completed", "priority": "high" },
                            { "content": "Task 2", "status": "in_progress", "priority": "medium" }
                        ]
                    }
                }
            })],
        });

        app.complete_function_execution("todo".to_string(), serde_json::json!({ "success": true }));
        assert_eq!(app.chat.todos[0].status, "completed");
    }
}
