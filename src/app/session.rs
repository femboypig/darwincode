use anyhow::Result;
use crate::app::{ChatState, MessageLine};
use crate::gemini::ChatMessage;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub timestamp: u64,
    pub model: String,
    pub history: Vec<ChatMessage>,
}

#[derive(Clone, Debug)]
pub struct SessionMeta {
    pub id: String,
    pub snippet: String,
}

#[derive(Debug, Default)]
pub struct SessionPickerState {
    pub sessions: Vec<SessionMeta>,
    pub selected: usize,
}

impl SessionPickerState {
    pub fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1) % self.sessions.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.sessions.len() - 1);
        }
    }

    pub fn selected_session(&self) -> Option<&SessionMeta> {
        self.sessions.get(self.selected)
    }
}
pub fn format_tool_summary(name: &str, args: &serde_json::Value, response: &serde_json::Value) -> String {
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

    let mut summary = format!("**{tool_label}** ");
    let mut res_parts = Vec::new();
    
    if let Some(err) = response.get("error").and_then(|v| v.as_str()) {
        res_parts.push(format!("Error: {err}"));
    } else {
        match name {
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
    
    summary
}
pub fn save_session(chat: &ChatState) -> Result<()> {
    let sessions_dir = crate::config::config_path()?.parent().unwrap().join("sessions");
    std::fs::create_dir_all(&sessions_dir)?;
    
    let session = ChatSession {
        id: chat.session_id.clone(),
        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        model: chat.config.model.clone(),
        history: chat.history.clone(),
    };
    
    let path = sessions_dir.join(format!("{}.json", chat.session_id));
    let key = crate::crypto::derive_hardware_key()?;
    let plain_data = serde_json::to_vec(&session)?;
    let cipher_data = crate::crypto::encrypt_data(&plain_data, &key)?;
    std::fs::write(path, cipher_data)?;
    Ok(())
}

pub fn load_session(id: &str) -> Result<ChatSession> {
    let sessions_dir = crate::config::config_path()?.parent().unwrap().join("sessions");
    let path = sessions_dir.join(format!("{}.json", id));
    let key = crate::crypto::derive_hardware_key()?;
    let cipher_data = std::fs::read(path)?;
    let plain_data = crate::crypto::decrypt_data(&cipher_data, &key)?;
    let session = serde_json::from_slice(&plain_data)?;
    Ok(session)
}

pub fn list_saved_sessions() -> Result<Vec<SessionMeta>> {
    let sessions_dir = crate::config::config_path()?.parent().unwrap().join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let key = crate::crypto::derive_hardware_key()?;
    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && let Ok(cipher_data) = std::fs::read(&path)
                && let Ok(plain_data) = crate::crypto::decrypt_data(&cipher_data, &key)
                    && let Ok(session) = serde_json::from_slice::<serde_json::Value>(&plain_data) {
                        let id = session.get("id").and_then(|v| v.as_str()).unwrap_or("").to_owned();
                        if id.is_empty() { continue; }
                        
                        let mut snippet = "Empty chat".to_owned();
                        if let Some(history) = session.get("history").and_then(|v| v.as_array())
                            && let Some(first_msg) = history.first()
                                && let Some(parts) = first_msg.get("parts").and_then(|v| v.as_array())
                                    && let Some(first_part) = parts.first()
                                        && let Some(text) = first_part.get("text").and_then(|v| v.as_str()) {
                                            snippet = text.chars().take(40).collect::<String>();
                                        }
                        result.push(SessionMeta { id, snippet });
                    }
    }
    
    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}

pub fn rebuild_messages_from_history(history: &[ChatMessage]) -> Vec<MessageLine> {
    let mut messages = Vec::new();
    let mut last_function_call = None;

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
                        && let (Some(name), Some(args)) = (call.get("name").and_then(|v| v.as_str()), call.get("args")) {
                            last_function_call = Some((name.to_owned(), args.clone()));
                        }
                }

                let mut text = String::new();
                for part in &msg.parts {
                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                        text.push_str(t);
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
                        let response = resp.get("response").cloned().unwrap_or(serde_json::Value::Null);

                        let args = if let Some((ref c_name, ref c_args)) = last_function_call {
                            if c_name == name {
                                c_args.clone()
                            } else {
                                serde_json::Value::Null
                            }
                        } else {
                            serde_json::Value::Null
                        };
                        if name == "run_bash_command" {
                            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
                            let mut output = String::new();
                            let mut success = true;
                            
                            if let Some(status) = response.get("status").and_then(|v| v.as_i64()) {
                                if status != 0 { success = false; }
                            } else if response.get("error").is_some() {
                                success = false;
                            }
                            
                            let stdout = response.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
                            let stderr = response.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                            if !stdout.is_empty() { output.push_str(stdout); }
                            if !stderr.is_empty() { 
                                if !output.is_empty() { output.push('\n'); }
                                output.push_str(stderr);
                            }
                            
                            messages.push(MessageLine::shell(cmd.to_owned(), output, success));
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
        let temp_dir = std::env::temp_dir().join(format!("darwincode_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        unsafe {
            std::env::set_var("HOME", &temp_dir);
        }
        
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
        
        let messages = rebuild_messages_from_history(&loaded.history);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].author, "You");
        assert_eq!(messages[0].text, "Hello!");
        
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rebuild_tool_and_shell() {
        let mut history = Vec::new();
        history.push(ChatMessage::user("Run command".to_owned()));
        
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![
                serde_json::json!({
                    "functionCall": {
                        "name": "run_bash_command",
                        "args": { "command": "echo hello" }
                    }
                })
            ],
        });
        
        history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![
                serde_json::json!({
                    "functionResponse": {
                        "name": "run_bash_command",
                        "response": { "status": 0, "stdout": "hello\n", "stderr": "" }
                    }
                })
            ],
        });

        // Add write_file message interaction
        history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![
                serde_json::json!({
                    "functionCall": {
                        "name": "write_file",
                        "args": { "path": "foo.txt", "content": "hello" }
                    }
                })
            ],
        });

        history.push(ChatMessage {
            role: "function".to_owned(),
            parts: vec![
                serde_json::json!({
                    "functionResponse": {
                        "name": "write_file",
                        "response": { "success": true }
                    }
                })
            ],
        });
        
        let messages = rebuild_messages_from_history(&history);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].author, "You");
        assert_eq!(messages[0].text, "Run command");
        
        assert_eq!(messages[1].author, "Shell");
        assert_eq!(messages[1].shell_cmd, "echo hello");
        assert_eq!(messages[1].text, "hello\n");
        assert!(messages[1].shell_success);

        assert_eq!(messages[2].author, "Darwin");
        assert!(messages[2].text.contains("foo.txt"));
        assert!(messages[2].text.contains("+ hello"));
    }
}
