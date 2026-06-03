use anyhow::{Context, Result};
use std::io::BufRead;

use crate::api::types::{
    ChatMessage, Content, FunctionDeclaration, GeminiResponse, GenerateContentRequest,
    GenerateContentResponse, ListModelsResponse, Tool,
};
use crate::config::StoredConfig;

#[derive(Debug)]
pub struct GeminiClient {
    config: StoredConfig,
    agent: ureq::Agent,
}

impl GeminiClient {
    pub fn new(config: StoredConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .timeout_read(std::time::Duration::from_secs(900))
            .build();
        Self { config, agent }
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        if self.config.api_key.starts_with("sk-") {
            let url = format!("{}/models", self.config.base_url);
            let response = self
                .agent
                .get(&url)
                .set("Authorization", &format!("Bearer {}", self.config.api_key))
                .call()
                .map_err(read_error)?;

            let body_str = response
                .into_string()
                .context("failed to read OpenAI models response body")?;
            let body: serde_json::Value = serde_json::from_str(&body_str).with_context(|| {
                let truncated = if body_str.len() > 500 {
                    format!("{}...", &body_str[..500])
                } else {
                    body_str.clone()
                };
                format!(
                    "failed to parse OpenAI models response. Raw body: {}",
                    truncated
                )
            })?;

            let mut names = Vec::new();
            if let Some(data) = body.get("data").and_then(|v| v.as_array()) {
                for m in data {
                    if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
                        names.push(id.to_owned());
                    }
                }
            }

            names.sort();
            Ok(names)
        } else {
            let url = format!("{}/models", self.config.base_url);
            let response = self
                .agent
                .get(&url)
                .query("key", &self.config.api_key)
                .call()
                .map_err(read_error)?;

            let body_str = response
                .into_string()
                .context("failed to read API models response body")?;
            let response_data: ListModelsResponse =
                serde_json::from_str(&body_str).with_context(|| {
                    let truncated = if body_str.len() > 500 {
                        format!("{}...", &body_str[..500])
                    } else {
                        body_str.clone()
                    };
                    format!(
                        "failed to parse API models response. Raw body: {}",
                        truncated
                    )
                })?;

            let mut names = response_data
                .models
                .into_iter()
                .filter(|model| {
                    model
                        .supported_generation_methods
                        .iter()
                        .any(|method| method == "generateContent")
                })
                .map(|model| model.name)
                .collect::<Vec<_>>();

            names.sort();
            Ok(names)
        }
    }

    pub fn generate_stream(
        &self,
        history: &[ChatMessage],
        cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
        mut on_chunk: impl FnMut(GeminiResponse) -> Result<()>,
    ) -> Result<()> {
        let model = self.config.model.trim_start_matches("models/");

        let mut tools = Vec::new();
        let mut declarations = Vec::new();

        if self.config.enable_codebase_tools {
            declarations.push(FunctionDeclaration {
                name: "read_file".to_owned(),
                description: "Read the contents of a file.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "path": {
                            "type": "STRING",
                            "description": "Path to the file"
                        }
                    },
                    "required": ["path"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "list_directory".to_owned(),
                description: "List the contents of a directory.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "path": {
                            "type": "STRING",
                            "description": "Path to the directory"
                        }
                    },
                    "required": ["path"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "search_files".to_owned(),
                description: "Search for a pattern in files using grep.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "pattern": {
                            "type": "STRING",
                            "description": "The regex pattern to search for"
                        },
                        "path": {
                            "type": "STRING",
                            "description": "Optional directory to search in"
                        }
                    },
                    "required": ["pattern"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "edit_file".to_owned(),
                description: "Edit a file by replacing an old string with a new string.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "path": {
                            "type": "STRING",
                            "description": "Path to the file to edit"
                        },
                        "old_string": {
                            "type": "STRING",
                            "description": "The exact old string to replace"
                        },
                        "new_string": {
                            "type": "STRING",
                            "description": "The new string to replace it with"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "write_file".to_owned(),
                description: "Write content to a file (creates the file and any parent directories if they do not exist, or overwrites if it does exist).".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "path": {
                            "type": "STRING",
                            "description": "Path to the file to write"
                        },
                        "content": {
                            "type": "STRING",
                            "description": "The file content to write"
                        }
                    },
                    "required": ["path", "content"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "read_files".to_owned(),
                description: "Read the contents of multiple files at once.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "paths": {
                            "type": "ARRAY",
                            "items": {
                                "type": "STRING"
                            },
                            "description": "Array of file paths to read"
                        }
                    },
                    "required": ["paths"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "edit_files".to_owned(),
                description: "Atomically edit multiple files at once. Applies replacements in memory, validates them, and writes all modifications together. If any edit fails, all changes are rolled back.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "edits": {
                            "type": "ARRAY",
                            "items": {
                                "type": "OBJECT",
                                "properties": {
                                    "path": {
                                         "type": "STRING",
                                         "description": "Path to the file to edit"
                                    },
                                    "old_string": {
                                         "type": "STRING",
                                         "description": "The exact old string to replace"
                                    },
                                    "new_string": {
                                         "type": "STRING",
                                         "description": "The new string to replace it with"
                                    }
                                },
                                "required": ["path", "old_string", "new_string"]
                            },
                            "description": "Array of edits to apply atomically"
                        }
                    },
                    "required": ["edits"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "web_search".to_owned(),
                description: "Perform a Google/DuckDuckGo search for the given query, or fetch the direct content of a webpage if query is a URL (starts with http/https). Use this to research libraries, error codes, search for information, or fetch public websites.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "query": {
                            "type": "STRING",
                            "description": "The search query or webpage URL to fetch"
                        }
                    },
                    "required": ["query"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "ask_user".to_owned(),
                description: "Ask the user a clarifying question when there are multiple paths or ambiguities, and get their choice or text response. Use this to confirm design preferences, clarify intent, or ask them questions.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "question": {
                            "type": "STRING",
                            "description": "The question to present to the user"
                        },
                        "options": {
                            "type": "ARRAY",
                            "items": {
                                "type": "STRING"
                            },
                            "description": "Optional list of predefined choices for multiple-choice input"
                        }
                    },
                    "required": ["question"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "edit_file_lines".to_owned(),
                description: "Edit a file by replacing a range of lines (1-based, inclusive) with new content.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "path": {
                            "type": "STRING",
                            "description": "Path to the file to edit"
                        },
                        "start_line": {
                            "type": "INTEGER",
                            "description": "The 1-based starting line number (inclusive) to replace"
                        },
                        "end_line": {
                            "type": "INTEGER",
                            "description": "The 1-based ending line number (inclusive) to replace"
                        },
                        "new_content": {
                            "type": "STRING",
                            "description": "The new content to put in place of the target lines"
                        }
                    },
                    "required": ["path", "start_line", "end_line", "new_content"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "apply_patch".to_owned(),
                description: "Apply a unified diff / patch to the workspace using git apply.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "patch": {
                            "type": "STRING",
                            "description": "The unified diff/patch content to apply to the workspace"
                        }
                    },
                    "required": ["patch"]
                })),
            });
        }

        if self.config.enable_bash_tools {
            declarations.push(FunctionDeclaration {
                name: "run_bash_command".to_owned(),
                description: "Run a bash command. Can optionally be run in background, take input on stdin, or run in a persistent session.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "command": {
                            "type": "STRING",
                            "description": "The bash command to run"
                        },
                        "background": {
                            "type": "BOOLEAN",
                            "description": "If true, run the command in the background and return the PID immediately"
                        },
                        "input": {
                            "type": "STRING",
                            "description": "Optional text to feed into the stdin of the command at startup. Note: For interactive commands like 'read' that wait for confirmation, you must append an actual newline '\\n' to the end of the input string to simulate pressing Enter."
                        },
                        "persistent_session_id": {
                            "type": "STRING",
                            "description": "Optional session ID to run the command in a persistent bash shell session"
                        }
                    },
                    "required": ["command"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "check_process".to_owned(),
                description: "Check if a background process is still running.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "pid": {
                            "type": "INTEGER",
                            "description": "The PID of the background process to check"
                        }
                    },
                    "required": ["pid"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "kill_process".to_owned(),
                description: "Terminate a background process by PID.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "pid": {
                            "type": "INTEGER",
                            "description": "The PID of the background process to kill"
                        }
                    },
                    "required": ["pid"]
                })),
            });
            declarations.push(FunctionDeclaration {
                name: "get_logs".to_owned(),
                description:
                    "Retrieve accumulated stdout and stderr logs for a background process."
                        .to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "pid": {
                            "type": "INTEGER",
                            "description": "The PID of the background process"
                        },
                        "limit": {
                            "type": "INTEGER",
                            "description": "Optional maximum number of trailing lines to return"
                        }
                    },
                    "required": ["pid"]
                })),
            });
        }

        let system_instruction = Some(Content {
            role: "system".to_owned(),
            parts: vec![serde_json::json!({
                "text": "You are darwincode, a premium, world-class expert agentic AI coding assistant operating inside a terminal TUI.\n\n## CORE OBJECTIVE\nDeliver highly precise, robust, compile-checked, and elegant solutions to the user's coding requests. Work efficiently, minimize conversational fluff, and keep responses concise and focused.\n\n## TOOL USAGE DIRECTIVES\nYou have access to specialized native tools to interact with the environment. You must use them strictly as defined below:\n\n1. **Reading & Exploring Workspace**:\n   - Use `list_directory` to examine directories.\n   - Use `search_files` to find files matching a name/pattern.\n   - Use `read_file` to read the contents of a file, or `read_files` to read multiple files simultaneously.\n   - *CRITICAL*: Never use generic shell commands (e.g. `ls`, `find`, `grep`, `cat`) via `run_bash_command` to read or explore the workspace.\n\n2. **Writing & Modifying Files**:\n   - `edit_file`: Replace specific contiguous blocks of text in a single file.\n     - *CRITICAL RULE*: You MUST read the file contents first (using `read_file` or `read_files`) before calling `edit_file` to ensure you know the exact whitespace, layout, and contents of `old_string`. Hallucinating `old_string` will result in failure!\n   - `edit_file_lines`: Edit a file by replacing a range of lines (1-based, inclusive) with new content. Safer when the context contains duplicate lines or hard-to-reproduce whitespace.\n   - `edit_files`: Atomically edit multiple files simultaneously. Highly recommended when modifying multiple interdependent files. Same reading-first rules apply!\n   - `write_file`: Use this ONLY to create entirely new files. NEVER use `write_file` to edit or overwrite existing files unless you are performing a complete rewrite of more than 80% of the content.\n   - `apply_patch`: Apply a unified diff / patch to the workspace using git apply. This is extremely robust and avoids line match / whitespace issues.\n   - *CRITICAL*: Never use shell redirection, `echo`, `sed`, `awk`, or editors via `run_bash_command` to edit files.\n\n3. **Shell Commands**:\n   - Use `run_bash_command` exclusively for running compilers, executing test suites, running build scripts, or launching compiled binaries. Do not use it for file management or exploration. Supports background execution (with `background: true`, which returns a PID) and persistent sequential shell sessions (with `persistent_session_id`).\n   - When sending standard input via the `input` parameter to `run_bash_command` (especially for interactive commands like `read`), you MUST append a newline character `\\n` to the end of the input string for the command to register and finalize the input (otherwise the command will hang waiting for Enter).\n   - Use `check_process`, `kill_process`, and `get_logs` to manage and monitor background processes.\n\n4. **Web Search & Direct Page Retrieval**:\n   - Use `web_search` to query the web (Google/DuckDuckGo) or fetch the direct text/HTML content of a web link (when the query starts with http:// or https://). Use this to search for libraries, reference codebases, documentation, or read example websites.\n\n5. **User Clarifications**:\n   - Use `ask_user` when you need clarification, user design preferences (e.g., choice between multiple architectures, options for styling, list of pages), or to resolve ambiguity. Avoid making assumptions on complex decisions; just ask the user!\n   - *CRITICAL*: The user is allowed to select from the options OR write any custom text response. Accept the response as valid input. If the user's response is reasonable and constructive, proceed with it. If it is a joke, obviously nonsensical, or unrelated, do not argue, criticize, or write patronizing meta-commentary; instead, politely guide them back to the necessary context or proceed with the most reasonable path available. Never re-ask the exact same question in a loop.\n\n## BEHAVIORAL & FAULT-TOLERANCE PROTOCOLS\n- **Self-Correction on Tool Failures**: If a tool returns an error (e.g. `old_string not found` or `No such file or directory`):\n  1. DO NOT give up, apologize, or stop.\n  2. Read the error message carefully.\n  3. Call the appropriate tool (e.g. `read_file` to inspect the actual file contents, or `list_directory` to check paths).\n  4. Promptly perform a corrected tool call immediately.\n- **No Fluff**: Keep your explanations extremely brief and concise. The user is in a terminal TUI where screen space is highly valuable. Do not summarize tool outputs or write lengthy intros.\n- **Action-Oriented**: If you need information, immediately call the appropriate tool. Do not ask for permission to read files or search the workspace.\n- **Verification**: Always verify that your changes compile and pass tests by running the appropriate compile/test commands (e.g. `cargo check`, `cargo test`, `npm run build`, etc.) using `run_bash_command` before concluding your turn."
            })],
        });

        if self.config.api_key.starts_with("sk-") {
            let mut openai_tools = Vec::new();
            for decl in &declarations {
                let mut params = decl.parameters.clone().unwrap_or(serde_json::json!({}));
                lowercase_types(&mut params);

                openai_tools.push(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": decl.name.clone(),
                        "description": decl.description.clone(),
                        "parameters": params
                    }
                }));
            }

            let mut openai_messages = Vec::new();
            let model_lower = model.to_lowercase();
            let is_reasoning_model = model_lower.contains("reasoner") || model_lower.contains("r1");
            let has_reasoning = is_reasoning_model
                || history.iter().any(|msg| {
                    msg.parts.iter().any(|part| {
                        part.get("thought")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                            || part.get("reasoning_content").is_some()
                    })
                });
            if let Some(sys) = &system_instruction
                && let Some(text) = sys
                    .parts
                    .first()
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
            {
                openai_messages.push(serde_json::json!({
                    "role": "system",
                    "content": text
                }));
            }

            let mut call_counter = 0;
            let mut tool_call_ids: Vec<(String, String)> = Vec::new();

            for (i, msg) in history.iter().enumerate() {
                match msg.role.as_str() {
                    "user" => {
                        let has_images = msg
                            .parts
                            .iter()
                            .any(|part| part.get("inlineData").is_some());
                        let supports_vision = model_supports_vision(model, &self.config.base_url);
                        if has_images && supports_vision {
                            let mut content_array = Vec::new();
                            for part in &msg.parts {
                                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                                    if !t.is_empty() {
                                        content_array.push(serde_json::json!({
                                            "type": "text",
                                            "text": t
                                        }));
                                    }
                                } else if let Some(inline_data) = part.get("inlineData")
                                    && let Some(mime) =
                                        inline_data.get("mimeType").and_then(|v| v.as_str())
                                    && let Some(data) =
                                        inline_data.get("data").and_then(|v| v.as_str())
                                {
                                    content_array.push(serde_json::json!({
                                        "type": "image_url",
                                        "image_url": {
                                            "url": format!("data:{};base64,{}", mime, data)
                                        }
                                    }));
                                }
                            }
                            openai_messages.push(serde_json::json!({
                                "role": "user",
                                "content": content_array
                            }));
                        } else {
                            let mut text = String::new();
                            for part in &msg.parts {
                                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                                    text.push_str(t);
                                }
                            }
                            openai_messages.push(serde_json::json!({
                                "role": "user",
                                "content": text
                            }));
                        }
                    }
                    "model" => {
                        let mut content = String::new();
                        let mut reasoning_content = String::new();
                        let mut tool_calls = Vec::new();

                        let mut responded_names = Vec::new();
                        let mut next_idx = i + 1;
                        while let Some(next_msg) = history.get(next_idx)
                            && next_msg.role == "function"
                        {
                            for part in &next_msg.parts {
                                if let Some(resp) = part.get("functionResponse")
                                    && let Some(name) = resp.get("name").and_then(|v| v.as_str())
                                {
                                    responded_names.push(name.to_owned());
                                }
                            }
                            next_idx += 1;
                        }

                        for part in &msg.parts {
                            let text = part.get("text").and_then(|v| v.as_str());
                            let reasoning = part.get("reasoning_content").and_then(|v| v.as_str());

                            if reasoning.is_some()
                                || part
                                    .get("thought")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false)
                                || text
                                    .map(|t| {
                                        t.starts_with("Thinking:")
                                            || t.starts_with("Thinking...")
                                            || t.starts_with("░ Thinking:")
                                            || t.starts_with("░ Thinking...")
                                    })
                                    .unwrap_or(false)
                            {
                                let mut r = reasoning.or(text).unwrap_or("");
                                if r.starts_with("Thinking:") {
                                    r = &r["Thinking:".len()..];
                                } else if r.starts_with("Thinking...") {
                                    r = &r["Thinking...".len()..];
                                } else if r.starts_with("░ Thinking:") {
                                    r = &r["░ Thinking:".len()..];
                                } else if r.starts_with("░ Thinking...") {
                                    r = &r["░ Thinking...".len()..];
                                }
                                reasoning_content.push_str(r);
                            } else if let Some(t) = text {
                                content.push_str(t);
                            }
                            if let Some(call) = part.get("functionCall")
                                && let Some(name) = call.get("name").and_then(|v| v.as_str())
                            {
                                let is_responded = if let Some(pos) =
                                    responded_names.iter().position(|n| n == name)
                                {
                                    responded_names.remove(pos);
                                    true
                                } else {
                                    false
                                };

                                if is_responded {
                                    let args =
                                        call.get("args").cloned().unwrap_or(serde_json::json!({}));
                                    let call_id = format!("call_{}", call_counter);
                                    call_counter += 1;
                                    tool_call_ids.push((name.to_owned(), call_id.clone()));
                                    tool_calls.push(serde_json::json!({
                                        "id": call_id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": args.to_string()
                                        }
                                    }));
                                }
                            }
                        }

                        let mut msg_obj = serde_json::json!({
                            "role": "assistant"
                        });
                        if !tool_calls.is_empty() {
                            msg_obj
                                .as_object_mut()
                                .unwrap()
                                .insert("tool_calls".to_owned(), serde_json::json!(tool_calls));
                        }
                        if !content.is_empty() {
                            msg_obj
                                .as_object_mut()
                                .unwrap()
                                .insert("content".to_owned(), serde_json::json!(content));
                        } else if !tool_calls.is_empty() {
                            msg_obj
                                .as_object_mut()
                                .unwrap()
                                .insert("content".to_owned(), serde_json::Value::Null);
                        } else {
                            msg_obj
                                .as_object_mut()
                                .unwrap()
                                .insert("content".to_owned(), serde_json::json!(""));
                        }
                        if !reasoning_content.is_empty() {
                            msg_obj.as_object_mut().unwrap().insert(
                                "reasoning_content".to_owned(),
                                serde_json::json!(reasoning_content),
                            );
                        } else if has_reasoning {
                            msg_obj
                                .as_object_mut()
                                .unwrap()
                                .insert("reasoning_content".to_owned(), serde_json::json!(""));
                        }
                        openai_messages.push(msg_obj);
                    }
                    "function" => {
                        for part in &msg.parts {
                            if let Some(resp) = part.get("functionResponse")
                                && let Some(name) = resp.get("name").and_then(|v| v.as_str())
                            {
                                let response = resp
                                    .get("response")
                                    .cloned()
                                    .unwrap_or(serde_json::json!({}));
                                if let Some(pos) = tool_call_ids.iter().position(|(n, _)| n == name)
                                {
                                    let (_, call_id) = tool_call_ids.remove(pos);
                                    openai_messages.push(serde_json::json!({
                                        "role": "tool",
                                        "tool_call_id": call_id,
                                        "name": name,
                                        "content": response.to_string()
                                    }));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            let mut request = serde_json::json!({
                "model": model,
                "messages": openai_messages,
                "stream": true
            });

            if !openai_tools.is_empty() {
                request
                    .as_object_mut()
                    .unwrap()
                    .insert("tools".to_owned(), serde_json::json!(openai_tools));
            }

            if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                anyhow::bail!("Stream cancelled");
            }

            let url = format!("{}/chat/completions", self.config.base_url);
            let response = self
                .agent
                .post(&url)
                .set("Authorization", &format!("Bearer {}", self.config.api_key))
                .send_json(request)
                .map_err(read_error)?;

            #[derive(Default, Clone)]
            struct ToolCallAccumulator {
                id: Option<String>,
                name: Option<String>,
                arguments: String,
            }

            let mut accumulated_tools: Vec<ToolCallAccumulator> = Vec::new();
            let reader = std::io::BufReader::new(response.into_reader());

            for line in reader.lines() {
                if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                    anyhow::bail!("Stream cancelled");
                }
                let line = line.context("failed to read stream line")?;
                if let Some(stripped) = line.strip_prefix("data: ") {
                    let json_str = stripped.trim();
                    if json_str == "[DONE]" {
                        break;
                    }
                    if json_str.is_empty() {
                        continue;
                    }

                    let chunk: serde_json::Value = serde_json::from_str(json_str)
                        .context("failed to parse stream chunk JSON")?;

                    if let Some(err) = chunk.get("error") {
                        let msg = err
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        anyhow::bail!("API Error: {}", msg);
                    }

                    if let Some(choices) = chunk.get("choices").and_then(|v| v.as_array())
                        && let Some(choice) = choices.first()
                        && let Some(delta) = choice.get("delta")
                    {
                        if let Some(content) = delta.get("content").and_then(|v| v.as_str())
                            && !content.is_empty()
                        {
                            on_chunk(GeminiResponse::Turn(vec![serde_json::json!({
                                "text": content
                            })]))?;
                        }
                        let reasoning = delta
                            .get("reasoning_content")
                            .or_else(|| delta.get("reasoning"))
                            .and_then(|v| v.as_str());
                        if let Some(reasoning) = reasoning
                            && !reasoning.is_empty()
                        {
                            on_chunk(GeminiResponse::Turn(vec![serde_json::json!({
                                "text": reasoning,
                                "thought": true,
                                "reasoning_content": reasoning
                            })]))?;
                        }

                        if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array())
                        {
                            for tc in tool_calls {
                                let idx =
                                    tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                if idx >= accumulated_tools.len() {
                                    accumulated_tools
                                        .resize(idx + 1, ToolCallAccumulator::default());
                                }
                                let acc = &mut accumulated_tools[idx];
                                if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                    acc.id = Some(id.to_owned());
                                }
                                if let Some(func) = tc.get("function") {
                                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                        acc.name = Some(name.to_owned());
                                    }
                                    if let Some(args) =
                                        func.get("arguments").and_then(|v| v.as_str())
                                    {
                                        acc.arguments.push_str(args);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            for acc in accumulated_tools {
                if let Some(name) = acc.name {
                    let args: serde_json::Value = serde_json::from_str(&acc.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    on_chunk(GeminiResponse::Turn(vec![serde_json::json!({
                        "functionCall": {
                            "name": name,
                            "args": args
                        }
                    })]))?;
                }
            }

            Ok(())
        } else {
            if !declarations.is_empty() {
                tools.push(Tool {
                    function_declarations: declarations,
                });
            }

            let request = GenerateContentRequest {
                system_instruction,
                contents: history.iter().map(Content::from_message).collect(),
                tools: if tools.is_empty() { None } else { Some(tools) },
            };

            if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                anyhow::bail!("Stream cancelled");
            }

            let url = format!(
                "{}/models/{model}:streamGenerateContent",
                self.config.base_url
            );
            let response = self
                .agent
                .post(&url)
                .query("key", &self.config.api_key)
                .query("alt", "sse")
                .send_json(serde_json::to_value(request)?)
                .map_err(read_error)?;

            let reader = std::io::BufReader::new(response.into_reader());
            for line in reader.lines() {
                if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                    anyhow::bail!("Stream cancelled");
                }
                let line = line.context("failed to read stream line")?;
                if let Some(json_str) = line.strip_prefix("data: ") {
                    let chunk: GenerateContentResponse = serde_json::from_str(json_str)
                        .context("failed to parse stream chunk JSON")?;
                    if let Some(err) = &chunk.error {
                        anyhow::bail!("API Error ({}): {}", err.code.unwrap_or(0), err.message);
                    }
                    if let Some(gemini_response) = chunk.into_response() {
                        on_chunk(gemini_response)?;
                    }
                }
            }

            Ok(())
        }
    }
}

fn lowercase_types(value: &mut serde_json::Value) {
    if let Some(obj) = value.as_object_mut() {
        if let Some(s) = obj
            .get("type")
            .and_then(|t| t.as_str())
            .map(|s| s.to_lowercase())
        {
            obj.insert("type".to_owned(), serde_json::json!(s));
        }
        for val in obj.values_mut() {
            lowercase_types(val);
        }
    } else if let Some(arr) = value.as_array_mut() {
        for val in arr {
            lowercase_types(val);
        }
    }
}

fn read_error(error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(code, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "unknown error".to_owned());
            anyhow::anyhow!("API request failed with HTTP {code}: {message}")
        }
        ureq::Error::Transport(error) => anyhow::anyhow!("API request failed: {error}"),
    }
}

fn model_supports_vision(model: &str, base_url: &str) -> bool {
    let m = model.to_lowercase();
    let b = base_url.to_lowercase();

    // Explicitly blacklisted text-only models and endpoints
    if m.contains("deepseek")
        || b.contains("deepseek")
        || m.contains("coder")
        || m.contains("reasoner")
        || m.contains("r1")
    {
        return false;
    }

    // Default to true so custom/other vision models work out-of-the-box
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_supports_vision() {
        // Standard vision models
        assert!(model_supports_vision("gpt-4o", "https://api.openai.com/v1"));
        assert!(model_supports_vision(
            "claude-3-5-sonnet",
            "https://api.anthropic.com/v1"
        ));
        assert!(model_supports_vision(
            "gemini-1.5-flash",
            "https://generativelanguage.googleapis.com"
        ));

        // Custom proxy model names (like big-pickle on opencode.ai)
        assert!(model_supports_vision(
            "big-pickle",
            "https://opencode.ai/zen/v1"
        ));

        // Blacklisted text-only models and endpoints
        assert!(!model_supports_vision(
            "deepseek-chat",
            "https://api.deepseek.com/v1"
        ));
        assert!(!model_supports_vision(
            "deepseek-coder",
            "https://api.deepseek.com/v1"
        ));
        assert!(!model_supports_vision(
            "deepseek-reasoner",
            "https://api.deepseek.com/v1"
        ));
        assert!(!model_supports_vision(
            "qwen2.5-coder",
            "https://api.openai.com/v1"
        ));
        assert!(!model_supports_vision(
            "big-pickle",
            "https://api.deepseek.com/v1"
        ));
    }
}
