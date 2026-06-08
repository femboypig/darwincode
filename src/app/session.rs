use crate::api::ChatMessage;
use crate::app::{ChatState, MessageLine};
use anyhow::Result;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub timestamp: u64,
    pub model: String,
    pub history: Vec<ChatMessage>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub snippet: String,
}

#[derive(Debug, Default)]
pub struct SessionPickerState {
    pub sessions: Vec<SessionMeta>,
    pub selected: usize,
    pub query: String,
}

impl SessionPickerState {
    pub fn filtered_sessions(&self) -> Vec<SessionMeta> {
        if self.query.is_empty() {
            return self.sessions.clone();
        }
        let q = self.query.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| s.id.to_lowercase().contains(&q) || s.snippet.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn select_next(&mut self) {
        let len = self.filtered_sessions().len();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    pub fn select_previous(&mut self) {
        let len = self.filtered_sessions().len();
        if len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(len - 1);
        }
    }

    pub fn selected_session(&self) -> Option<SessionMeta> {
        self.filtered_sessions().get(self.selected).cloned()
    }

    pub fn update_query(&mut self, query: String) {
        self.query = query;
        let len = self.filtered_sessions().len();
        if self.selected >= len {
            self.selected = if len > 0 { len - 1 } else { 0 };
        }
    }
}
pub fn format_tool_summary(
    name: &str,
    args: &serde_json::Value,
    response: &serde_json::Value,
) -> String {
    let tool_label = match name {
        "read" => "read".to_owned(),
        "grep" => "grep".to_owned(),
        "glob" => "glob".to_owned(),
        "edit" => "edit".to_owned(),
        "write" => "write".to_owned(),
        "patch" => "patch".to_owned(),
        "sh" => "sh".to_owned(),
        "ps" => "ps".to_owned(),
        "kill" => "kill".to_owned(),
        "logs" => "logs".to_owned(),
        "websearch" => "websearch".to_owned(),
        "ask" => "ask".to_owned(),
        "todo" => "todo".to_owned(),
        _ => {
            let mut label = String::new();
            for c in name.chars() {
                if c == '_' {
                    label.push(' ');
                } else {
                    label.push(c.to_ascii_lowercase());
                }
            }
            label
        }
    };

    let mut summary = format!("**{tool_label}** ");
    let mut res_parts = Vec::new();

    if let Some(err) = response.get("error").and_then(|v| v.as_str()) {
        res_parts.push(format!("Error: {err}"));
    } else {
        match name {
            "todo" => {
                if let Some(todos) = args.get("todos").and_then(|v| v.as_array()) {
                    let mut completed = 0;
                    let mut in_progress = 0;
                    let mut pending = 0;
                    let mut cancelled = 0;
                    for t in todos {
                        if let Some(status) = t.get("status").and_then(|v| v.as_str()) {
                            match status {
                                "completed" => completed += 1,
                                "in_progress" => in_progress += 1,
                                "pending" => pending += 1,
                                "cancelled" => cancelled += 1,
                                _ => {}
                            }
                        }
                    }
                    res_parts.push(format!(
                        "updated task list ({} completed, {} in progress, {} pending, {} cancelled)",
                        completed, in_progress, pending, cancelled
                    ));
                } else {
                    res_parts.push("updated task list".to_owned());
                }
            }
            "edit" => {
                if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                    let edits_len = edits.len();
                    let suffix = if edits_len == 1 { "file" } else { "files" };
                    res_parts.push(format!("atomically edited {edits_len} {suffix}"));
                } else if args.get("start_line").is_some() {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    let start = args.get("start_line").and_then(|v| v.as_i64()).unwrap_or(0);
                    let end = args.get("end_line").and_then(|v| v.as_i64()).unwrap_or(0);
                    res_parts.push(format!("`{path}` lines {start}-{end} updated"));
                } else {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    res_parts.push(format!("`{path}` updated"));
                }
                if let Some(diff) = response.get("diff").and_then(|v| v.as_str()) {
                    res_parts.push(format!("\n{diff}"));
                }
            }
            "patch" => {
                res_parts.push("Patch applied successfully".to_owned());
            }
            "ps" => {
                let pid = args.get("pid").and_then(|v| v.as_i64()).unwrap_or(0);
                let alive = response
                    .get("alive")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let status = if alive { "running" } else { "terminated" };
                res_parts.push(format!("Process {pid} is {status}"));
            }
            "kill" => {
                let pid = args.get("pid").and_then(|v| v.as_i64()).unwrap_or(0);
                res_parts.push(format!("Process {pid} terminated"));
            }
            "logs" => {
                let pid = args.get("pid").and_then(|v| v.as_i64()).unwrap_or(0);
                res_parts.push(format!("Logs for process {pid} retrieved"));
            }
            "read" => {
                if let Some(paths) = args.get("paths").and_then(|v| v.as_array()) {
                    let paths_len = paths.len();
                    let suffix = if paths_len == 1 { "file" } else { "files" };
                    res_parts.push(format!("read {paths_len} {suffix} successfully"));
                } else {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    if let Some(files) = response.get("files").and_then(|v| v.as_array()) {
                        res_parts.push(format!("`{path}` → {} items", files.len()));
                    } else if let Some(content) = response.get("content").and_then(|v| v.as_str()) {
                        let count = content.lines().count();
                        res_parts.push(format!("`{path}` read successfully ({count} lines)"));
                    } else {
                        res_parts.push(format!("`{path}` read successfully"));
                    }
                }
            }
            "write" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                res_parts.push(format!("`{path}` written successfully"));
                if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
                    let mut diff = "```diff\n".to_owned();
                    let lines: Vec<&str> = content.lines().collect();
                    let limit = 30;
                    for line in lines.iter().take(limit) {
                        diff.push_str("+ ");
                        diff.push_str(line);
                        diff.push('\n');
                    }
                    if lines.len() > limit {
                        diff.push_str(&format!("+ ... and {} more lines\n", lines.len() - limit));
                    }
                    diff.push_str("```");
                    res_parts.push(format!("\n{diff}"));
                }
            }
            "grep" | "glob" => {
                let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
                if let Some(matches) = response.get("matches").and_then(|v| v.as_str()) {
                    res_parts.push(format!("`{pattern}` → {} matches", matches.lines().count()));
                    if !matches.trim().is_empty() {
                        let lines: Vec<&str> = matches.lines().collect();
                        let limit = 50;
                        let mut formatted = "\n```grep\n".to_owned();
                        for line in lines.iter().take(limit) {
                            formatted.push_str(line);
                            formatted.push('\n');
                        }
                        if lines.len() > limit {
                            formatted.push_str(&format!(
                                "... and {} more matches\n",
                                lines.len() - limit
                            ));
                        }
                        formatted.push_str("```");
                        res_parts.push(formatted);
                    }
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

    summary
}
pub fn save_session(chat: &ChatState) -> Result<()> {
    let sessions_dir = crate::config::config_path()?
        .parent()
        .unwrap()
        .join("sessions");
    std::fs::create_dir_all(&sessions_dir)?;

    let session = ChatSession {
        id: chat.session_id.clone(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: chat.config.model.clone(),
        history: chat.history.clone(),
    };

    let path = sessions_dir.join(format!("{}.json", chat.session_id));
    let mut snippet = "Empty chat".to_owned();
    if let Some(first_msg) = chat.history.first() {
        let mut text = String::new();
        for part in &first_msg.parts {
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                text.push_str(t);
            }
        }
        if !text.is_empty() {
            snippet = text.chars().take(40).collect::<String>();
        }
    }
    let meta = SessionMeta {
        id: chat.session_id.clone(),
        snippet,
    };
    let meta_path = sessions_dir.join(format!("{}.meta", chat.session_id));

    if crate::crypto::is_home_appdata_missing() {
        let plain_data = serde_json::to_vec(&session)?;
        std::fs::write(&path, plain_data)?;
        let plain_meta = serde_json::to_vec(&meta)?;
        let _ = std::fs::write(&meta_path, plain_meta);
    } else {
        let key = crate::crypto::derive_hardware_key()?;
        let plain_data = serde_json::to_vec(&session)?;
        let cipher_data = crate::crypto::encrypt_data(&plain_data, &key)?;
        std::fs::write(&path, cipher_data)?;

        if let Ok(plain_meta) = serde_json::to_vec(&meta)
            && let Ok(cipher_meta) = crate::crypto::encrypt_data(&plain_meta, &key)
        {
            let _ = std::fs::write(meta_path, cipher_meta);
        }
    }

    Ok(())
}

pub fn load_session(id: &str) -> Result<ChatSession> {
    let sessions_dir = crate::config::config_path()?
        .parent()
        .unwrap()
        .join("sessions");
    let path = sessions_dir.join(format!("{}.json", id));
    if crate::crypto::is_home_appdata_missing() {
        let plain_data = std::fs::read(path)?;
        let session = serde_json::from_slice(&plain_data)?;
        Ok(session)
    } else {
        let key = crate::crypto::derive_hardware_key()?;
        let cipher_data = std::fs::read(path)?;
        let plain_data = crate::crypto::decrypt_data(&cipher_data, &key)?;
        let session = serde_json::from_slice(&plain_data)?;
        Ok(session)
    }
}

pub fn list_saved_sessions() -> Result<Vec<SessionMeta>> {
    let sessions_dir = crate::config::config_path()?
        .parent()
        .unwrap()
        .join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let is_missing = crate::crypto::is_home_appdata_missing();
    let key_opt = if is_missing {
        None
    } else {
        crate::crypto::derive_hardware_key().ok()
    };

    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned();
            if id.is_empty() {
                continue;
            }

            let meta_path = path.with_extension("meta");

            if is_missing {
                if meta_path.exists()
                    && let Ok(plain_data) = std::fs::read(&meta_path)
                    && let Ok(meta) = serde_json::from_slice::<SessionMeta>(&plain_data)
                {
                    result.push(meta);
                    continue;
                }

                if let Ok(plain_data) = std::fs::read(&path)
                    && let Ok(session) = serde_json::from_slice::<serde_json::Value>(&plain_data)
                {
                    let session_id = session
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned();
                    if session_id.is_empty() {
                        continue;
                    }

                    let mut snippet = "Empty chat".to_owned();
                    if let Some(history) = session.get("history").and_then(|v| v.as_array())
                        && let Some(first_msg) = history.first()
                        && let Some(parts) = first_msg.get("parts").and_then(|v| v.as_array())
                        && let Some(first_part) = parts.first()
                        && let Some(text) = first_part.get("text").and_then(|v| v.as_str())
                    {
                        snippet = text.chars().take(40).collect::<String>();
                    }
                    let meta = SessionMeta {
                        id: session_id,
                        snippet,
                    };
                    if let Ok(plain_meta) = serde_json::to_vec(&meta) {
                        let _ = std::fs::write(&meta_path, plain_meta);
                    }
                    result.push(meta);
                }
            } else if let Some(ref key) = key_opt {
                if meta_path.exists()
                    && let Ok(cipher_data) = std::fs::read(&meta_path)
                    && let Ok(plain_data) = crate::crypto::decrypt_data(&cipher_data, key)
                    && let Ok(meta) = serde_json::from_slice::<SessionMeta>(&plain_data)
                {
                    result.push(meta);
                    continue;
                }

                if let Ok(cipher_data) = std::fs::read(&path)
                    && let Ok(plain_data) = crate::crypto::decrypt_data(&cipher_data, key)
                    && let Ok(session) = serde_json::from_slice::<serde_json::Value>(&plain_data)
                {
                    let session_id = session
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned();
                    if session_id.is_empty() {
                        continue;
                    }

                    let mut snippet = "Empty chat".to_owned();
                    if let Some(history) = session.get("history").and_then(|v| v.as_array())
                        && let Some(first_msg) = history.first()
                        && let Some(parts) = first_msg.get("parts").and_then(|v| v.as_array())
                        && let Some(first_part) = parts.first()
                        && let Some(text) = first_part.get("text").and_then(|v| v.as_str())
                    {
                        snippet = text.chars().take(40).collect::<String>();
                    }
                    let meta = SessionMeta {
                        id: session_id,
                        snippet,
                    };

                    if let Ok(plain_meta) = serde_json::to_vec(&meta)
                        && let Ok(cipher_meta) = crate::crypto::encrypt_data(&plain_meta, key)
                    {
                        let _ = std::fs::write(&meta_path, cipher_meta);
                    }

                    result.push(meta);
                }
            }
        }
    }

    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}

pub fn rebuild_todos_from_history(history: &[ChatMessage]) -> Vec<crate::app::chat::TodoItem> {
    let mut current_todos = Vec::new();
    let mut proposed_todos = None;
    for msg in history {
        if msg.role == "model" {
            for part in &msg.parts {
                if let Some(call) = part.get("functionCall")
                    && call.get("name").and_then(|v| v.as_str()) == Some("todo")
                    && let Some(args) = call.get("args")
                    && let Some(todos_val) = args.get("todos")
                    && let Ok(todos) =
                        serde_json::from_value::<Vec<crate::app::chat::TodoItem>>(todos_val.clone())
                {
                    proposed_todos = Some(todos);
                }
            }
        } else if msg.role == "function" {
            for part in &msg.parts {
                if let Some(resp) = part.get("functionResponse")
                    && resp.get("name").and_then(|v| v.as_str()) == Some("todo")
                    && let Some(response_val) = resp.get("response")
                {
                    let success = response_val
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let has_error = response_val.get("error").is_some();
                    if success
                        && !has_error
                        && let Some(todos) = proposed_todos.take()
                    {
                        current_todos = todos;
                    }
                }
            }
        }
    }
    current_todos
}

pub fn rebuild_messages_from_history(
    history: &[ChatMessage],
    show_thoughts: bool,
) -> Vec<MessageLine> {
    let mut messages = Vec::new();
    let mut last_function_call: Option<(String, serde_json::Value)> = None;

    for msg in history {
        match msg.role.as_str() {
            "user" => {
                let mut text = String::new();
                for part in &msg.parts {
                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                        text.push_str(t);
                    }
                }
                messages.push(MessageLine::user(text));
            }
            "model" => {
                for part in &msg.parts {
                    if let Some(call) = part.get("functionCall")
                        && let (Some(name), Some(args)) =
                            (call.get("name").and_then(|v| v.as_str()), call.get("args"))
                    {
                        last_function_call = Some((name.to_owned(), args.clone()));
                    }
                }

                let mut text = String::new();
                let mut last_was_thought = false;
                let mut has_thought_content = false;

                for part in &msg.parts {
                    let is_thought = part
                        .get("thought")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        || part.get("thought_signature").is_some()
                        || part.get("reasoning_content").is_some()
                        || part.get("reasoning").is_some();

                    if let Some(t) = part.get("text").and_then(|v| v.as_str())
                        && !t.is_empty()
                    {
                        if is_thought {
                            if show_thoughts {
                                if !has_thought_content {
                                    text.push_str("Thinking: ");
                                    has_thought_content = true;
                                }
                                text.push_str(t);
                            }
                            last_was_thought = true;
                        } else {
                            if last_was_thought {
                                if show_thoughts {
                                    let clean_t =
                                        t.trim_start_matches('\n').trim_start_matches('\r');
                                    text.push_str(&format!("\n\n{}", clean_t));
                                } else {
                                    let clean_t =
                                        t.trim_start_matches('\n').trim_start_matches('\r');
                                    text.push_str(clean_t);
                                }
                            } else {
                                text.push_str(t);
                            }
                            last_was_thought = false;
                        }
                    }
                }
                if !text.is_empty() {
                    messages.push(MessageLine::assistant(text));
                }
            }
            "function" => {
                for part in &msg.parts {
                    if let Some(resp) = part.get("functionResponse") {
                        let name = resp.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let response = resp
                            .get("response")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);

                        let args = if let Some((ref c_name, ref c_args)) = last_function_call {
                            if c_name == name {
                                c_args.clone()
                            } else {
                                serde_json::Value::Null
                            }
                        } else {
                            serde_json::Value::Null
                        };
                        if name == "sh" {
                            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
                            let mut success = true;

                            let mut is_aborted = false;
                            let mut is_running = false;

                            if let Some(status) = response.get("status").and_then(|v| v.as_i64()) {
                                if status != 0 {
                                    success = false;
                                }
                            } else if let Some(status_str) =
                                response.get("status").and_then(|v| v.as_str())
                            {
                                if status_str == "running" {
                                    is_running = true;
                                } else {
                                    success = false;
                                }
                            } else if response.get("status").is_some()
                                && response.get("status").unwrap().is_null()
                            {
                                success = false;
                            } else {
                                success = false;
                                is_aborted = true;
                            }

                            if let Some(err_str) = response.get("error").and_then(|v| v.as_str())
                                && (err_str.contains("terminated by user via Ctrl+C")
                                    || err_str.contains("Process terminated by user via Ctrl+C"))
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
                            let error_field =
                                response.get("error").and_then(|v| v.as_str()).unwrap_or("");

                            let mut is_still_running_err = false;
                            if error_field == "Command timed out / is still running" {
                                is_still_running_err = true;
                                is_running = true;
                            }
                            let mut output = crate::app::file_ops::format_shell_output(
                                stdout,
                                stderr,
                                error_field,
                                is_still_running_err,
                                is_aborted,
                                is_running,
                                Some("[Process is still running...]"),
                            );
                            if output.is_empty() && !is_running {
                                output = "(empty output)".to_owned();
                            }

                            let body = format!("$ {}\n{}", cmd, output);
                            let persistent_session_id = args
                                .get("persistent_session_id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_owned());
                            let mut msg = MessageLine::shell(
                                cmd.to_owned(),
                                body,
                                success,
                                persistent_session_id,
                            );
                            msg.shell_pid = response
                                .get("pid")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);
                            messages.push(msg);
                        } else {
                            let summary = format_tool_summary(name, &args, &response);
                            messages.push(MessageLine::tool(summary));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_load_list_sessions() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "darwincode_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = Some(temp_dir.clone());

        let config = crate::config::StoredConfig {
            api_key: "test_key".to_owned(),
            ..Default::default()
        };
        let mut chat = ChatState::new(config);
        chat.session_id = "test_session_123".to_owned();
        chat.history.push(ChatMessage::user("Hello!".to_owned()));

        save_session(&chat).unwrap();

        let list = list_saved_sessions().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "test_session_123");
        assert_eq!(list[0].snippet, "Hello!");

        let loaded = load_session("test_session_123").unwrap();
        assert_eq!(loaded.id, "test_session_123");
        assert_eq!(loaded.history.len(), 1);

        let messages = rebuild_messages_from_history(&loaded.history, true);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].author, "You");
        assert_eq!(messages[0].text, "Hello!");

        *crate::config::TEST_CONFIG_DIR.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rebuild_tool_and_shell() {
        let mut history = Vec::new();
        history.push(ChatMessage::user("Run command".to_owned()));

        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "sh",
                    "args": { "command": "echo hello" }
                }
            })],
        });

        history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![serde_json::json!({
                "functionResponse": {
                    "name": "sh",
                    "response": { "status": 0, "stdout": "hello\n", "stderr": "" }
                }
            })],
        });

        // Add write message interaction
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "write",
                    "args": { "path": "foo.txt", "content": "hello" }
                }
            })],
        });

        history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![serde_json::json!({
                "functionResponse": {
                    "name": "write",
                    "response": { "success": true }
                }
            })],
        });

        let messages = rebuild_messages_from_history(&history, true);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].author, "You");
        assert_eq!(messages[0].text, "Run command");

        assert_eq!(messages[1].author, "Shell");
        assert_eq!(messages[1].shell_cmd, "echo hello");
        assert_eq!(messages[1].text, "$ echo hello\nhello\n");
        assert!(messages[1].shell_success);

        assert_eq!(messages[2].author, "Darwin");
        assert!(messages[2].text.contains("foo.txt"));
        assert!(messages[2].text.contains("+ hello"));
    }

    #[test]
    fn test_rebuild_todos_from_history() {
        let mut history = Vec::new();

        // 1. A failed todo call
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "todo",
                    "args": {
                        "todos": [
                            { "content": "Task 1", "status": "completed", "priority": "high" }
                        ]
                    }
                }
            })],
        });
        history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![serde_json::json!({
                "functionResponse": {
                    "name": "todo",
                    "response": { "success": false, "error": "Cannot start as completed" }
                }
            })],
        });

        // 2. A successful todo call
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "todo",
                    "args": {
                        "todos": [
                            { "content": "Task 1", "status": "pending", "priority": "high" }
                        ]
                    }
                }
            })],
        });
        history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![serde_json::json!({
                "functionResponse": {
                    "name": "todo",
                    "response": { "success": true }
                }
            })],
        });

        use crate::app::chat::{TodoPriority, TodoStatus};
        let todos = rebuild_todos_from_history(&history);
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Task 1");
        assert_eq!(todos[0].status, TodoStatus::Pending);
        assert_eq!(todos[0].priority, TodoPriority::High);
    }

    #[test]
    fn test_rebuild_reasoning_from_history() {
        let mut history = Vec::new();

        // 1. Assistant message with reasoning_content
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![
                serde_json::json!({
                    "text": "thinking step 1",
                    "reasoning_content": "thinking step 1"
                }),
                serde_json::json!({
                    "text": "final result"
                }),
            ],
        });

        // 2. Assistant message with reasoning field
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![
                serde_json::json!({
                    "text": "thinking step 2",
                    "reasoning": "thinking step 2"
                }),
                serde_json::json!({
                    "text": "second final result"
                }),
            ],
        });

        // Rebuild with show_thoughts = true
        let messages = rebuild_messages_from_history(&history, true);
        assert_eq!(messages.len(), 2);
        assert!(messages[0].text.contains("Thinking: thinking step 1"));
        assert!(messages[0].text.contains("final result"));
        assert!(messages[1].text.contains("Thinking: thinking step 2"));
        assert!(messages[1].text.contains("second final result"));

        // Rebuild with show_thoughts = false
        let messages = rebuild_messages_from_history(&history, false);
        assert_eq!(messages.len(), 2);
        assert!(!messages[0].text.contains("Thinking:"));
        assert_eq!(messages[0].text, "final result");
        assert!(!messages[1].text.contains("Thinking:"));
        assert_eq!(messages[1].text, "second final result");
    }
}
