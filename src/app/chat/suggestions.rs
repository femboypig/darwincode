use crate::api::ChatMessage;
use crate::config::PermissionLevel;

#[derive(Clone, Debug)]
pub struct CommandSuggestion {
    pub name: String,
    pub description: String,
}

pub enum ChatCommand {
    Settings,
    Exit,
    Models,
    Theme,
    Agents,
    Agent(Option<String>),
    Custom(String),
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
            "/theme" => Self::Theme,
            "/agents" => Self::Agents,
            "/agent" => {
                let arg = parts.next().map(|s| s.to_owned());
                Self::Agent(arg)
            }
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
            value => {
                let name = value.trim_start_matches('/');
                let custom_cmds = crate::app::load_custom_commands();
                if custom_cmds.contains_key(name) {
                    Self::Custom(name.to_owned())
                } else {
                    Self::Unknown(value.to_owned())
                }
            }
        })
    }

    pub fn suggestions() -> Vec<CommandSuggestion> {
        let mut suggs = vec![
            CommandSuggestion {
                name: "/settings".to_owned(),
                description: "Open settings".to_owned(),
            },
            CommandSuggestion {
                name: "/models".to_owned(),
                description: "List available models".to_owned(),
            },
            CommandSuggestion {
                name: "/theme".to_owned(),
                description: "Change active theme".to_owned(),
            },
            CommandSuggestion {
                name: "/agents".to_owned(),
                description: "List and switch active agents".to_owned(),
            },
            CommandSuggestion {
                name: "/agent".to_owned(),
                description: "Switch to a specific agent by name".to_owned(),
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
        ];

        let custom_cmds = crate::app::load_custom_commands();
        let mut sorted_cmds: Vec<_> = custom_cmds.into_iter().collect();
        sorted_cmds.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, config) in sorted_cmds {
            suggs.push(CommandSuggestion {
                name: format!("/{}", name),
                description: config.description.clone(),
            });
        }

        suggs
    }
}

pub fn extract_paths(text: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((idx, c)) = chars.next() {
        if c == '@' {
            if idx > 0 {
                let prev_char = text[..idx].chars().next_back().unwrap();
                if prev_char.is_alphanumeric() {
                    continue;
                }
            }

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

            let mut raw_path = String::new();
            while let Some(&(_, wc)) = chars.peek() {
                if wc.is_whitespace() {
                    break;
                }
                raw_path.push(wc);
                chars.next();
            }

            if !raw_path.is_empty() {
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
}
