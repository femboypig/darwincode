use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::BufRead;

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
        Self {
            config,
            agent,
        }
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
            let body: serde_json::Value = serde_json::from_str(&body_str)
                .with_context(|| {
                    let truncated = if body_str.len() > 500 {
                        format!("{}...", &body_str[..500])
                    } else {
                        body_str.clone()
                    };
                    format!("failed to parse OpenAI models response. Raw body: {}", truncated)
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
                .context("failed to read Gemini models response body")?;
            let response_data: ListModelsResponse = serde_json::from_str(&body_str)
                .with_context(|| {
                    let truncated = if body_str.len() > 500 {
                        format!("{}...", &body_str[..500])
                    } else {
                        body_str.clone()
                    };
                    format!("failed to parse Gemini models response. Raw body: {}", truncated)
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
        }
        
        if self.config.enable_bash_tools {
            declarations.push(FunctionDeclaration {
                name: "run_bash_command".to_owned(),
                description: "Run a bash command.".to_owned(),
                parameters: Some(serde_json::json!({
                    "type": "OBJECT",
                    "properties": {
                        "command": {
                            "type": "STRING",
                            "description": "The bash command to run"
                        }
                    },
                    "required": ["command"]
                })),
            });
        }

        let system_instruction = Some(Content {
            role: "system".to_owned(),
            parts: vec![serde_json::json!({
                "text": "You are darwincode, an expert agentic AI coding assistant operating inside a terminal TUI.\n\n## CORE OBJECTIVE\nDeliver highly precise, robust, and compile-checked solutions to the user's coding requests. Work efficiently, minimize conversational fluff, and keep responses concise and focused.\n\n## TOOL USAGE DIRECTIVES\nYou are equipped with specialized native tools to interact with the environment. You must use them strictly as defined below:\n\n1. **Reading & Exploring Workspace**:\n   - Use `list_directory` to examine directories.\n   - Use `search_files` to find files matching a name/pattern.\n   - Use `read_file` to read the contents of a file.\n   - *CRITICAL*: Never use generic shell commands (e.g. `ls`, `find`, `grep`, `cat`) via bash to read or explore the workspace.\n\n2. **Writing & Modifying Files**:\n   - `edit_file`: This is your primary tool for modifying existing files. Use it to replace specific contiguous blocks of text.\n   - `write_file`: Use this ONLY to create entirely new files. NEVER use `write_file` to edit or overwrite existing files unless you are performing a complete rewrite of more than 80% of the file's content. Rewriting files unnecessarily is forbidden.\n   - *CRITICAL*: Never use shell redirection, `echo`, `sed`, `awk`, or editor commands via bash to edit files.\n\n3. **Shell Commands**:\n   - Use `run_bash_command` exclusively for running compilers, executing test suites, executing build scripts, or launching compiled binaries. Do not use it for file management.\n\n## BEHAVIORAL PROTOCOLS\n- **No Fluff**: Keep your explanations very brief and concise. The user is in a terminal TUI where screen space is highly valuable. Do not summarize tool outputs or write lengthy intros.\n- **Action-Oriented**: If you need information, immediately call the appropriate tool. Do not ask for permission to read files or search the workspace.\n- **Verification**: Always verify that your changes compile and pass tests by running the appropriate compile/test commands (e.g. `cargo check`, `cargo test`) using `run_bash_command` before concluding your turn."
            })],
        });

        if self.config.api_key.starts_with("sk-") {
            // Build OpenAI compatible request
            let mut openai_tools = Vec::new();
            for decl in &declarations {
                let mut params = decl.parameters.clone().unwrap_or(serde_json::json!({}));
                if let Some(obj) = params.as_object_mut() {
                    if let Some(t) = obj.get_mut("type") {
                        if let Some(s) = t.as_str() {
                            *t = serde_json::json!(s.to_lowercase());
                        }
                    }
                    if let Some(props) = obj.get_mut("properties") {
                        if let Some(props_obj) = props.as_object_mut() {
                            for (_, prop_val) in props_obj {
                                if let Some(prop_obj) = prop_val.as_object_mut() {
                                    if let Some(t) = prop_obj.get_mut("type") {
                                        if let Some(s) = t.as_str() {
                                            *t = serde_json::json!(s.to_lowercase());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
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
            if let Some(sys) = &system_instruction {
                if let Some(text) = sys.parts.first().and_then(|p| p.get("text")).and_then(|t| t.as_str()) {
                    openai_messages.push(serde_json::json!({
                        "role": "system",
                        "content": text
                    }));
                }
            }
            
            let mut call_counter = 0;
            let mut tool_call_ids: Vec<(String, String)> = Vec::new();
            
            for (i, msg) in history.iter().enumerate() {
                match msg.role.as_str() {
                    "user" => {
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
                    "model" => {
                        let mut content = String::new();
                        let mut tool_calls = Vec::new();
                        
                        // Lookahead to collect all responded tool names in the subsequent "function" message(s)
                        let mut responded_names = Vec::new();
                        if let Some(next_msg) = history.get(i + 1) {
                            if next_msg.role == "function" {
                                for part in &next_msg.parts {
                                    if let Some(resp) = part.get("functionResponse") {
                                        if let Some(name) = resp.get("name").and_then(|v| v.as_str()) {
                                            responded_names.push(name.to_owned());
                                        }
                                    }
                                }
                            }
                        }

                        for part in &msg.parts {
                            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                content.push_str(text);
                            }
                            if let Some(call) = part.get("functionCall") {
                                if let Some(name) = call.get("name").and_then(|v| v.as_str()) {
                                    // Only include the tool call if it actually has a response
                                    let is_responded = if let Some(pos) = responded_names.iter().position(|n| n == name) {
                                        responded_names.remove(pos);
                                        true
                                    } else {
                                        false
                                    };

                                    if is_responded {
                                        let args = call.get("args").cloned().unwrap_or(serde_json::json!({}));
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
                        }
                        if !tool_calls.is_empty() {
                            let mut msg_obj = serde_json::json!({
                                "role": "assistant",
                                "tool_calls": tool_calls
                            });
                            if !content.is_empty() {
                                msg_obj.as_object_mut().unwrap().insert("content".to_owned(), serde_json::json!(content));
                            }
                            openai_messages.push(msg_obj);
                        } else {
                            openai_messages.push(serde_json::json!({
                                "role": "assistant",
                                "content": content
                            }));
                        }
                    }
                    "function" => {
                        for part in &msg.parts {
                            if let Some(resp) = part.get("functionResponse") {
                                if let Some(name) = resp.get("name").and_then(|v| v.as_str()) {
                                    let response = resp.get("response").cloned().unwrap_or(serde_json::json!({}));
                                    let call_id = if let Some(pos) = tool_call_ids.iter().position(|(n, _)| n == name) {
                                        let (_, cid) = tool_call_ids.remove(pos);
                                        cid
                                    } else {
                                        format!("call_unknown_{}", name)
                                    };
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
                request.as_object_mut().unwrap().insert("tools".to_owned(), serde_json::json!(openai_tools));
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
                let line = line.context("failed to read stream line")?;
                if line.starts_with("data: ") {
                    let json_str = line["data: ".len()..].trim();
                    if json_str == "[DONE]" {
                        break;
                    }
                    if json_str.is_empty() {
                        continue;
                    }
                    
                    let chunk: serde_json::Value = serde_json::from_str(json_str)
                        .context("failed to parse stream chunk JSON")?;
                    
                    if let Some(choices) = chunk.get("choices").and_then(|v| v.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                    if !content.is_empty() {
                                        on_chunk(GeminiResponse::Turn(vec![serde_json::json!({
                                            "text": content
                                        })]))?;
                                    }
                                }
                                
                                if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                                    for tc in tool_calls {
                                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        if idx >= accumulated_tools.len() {
                                            accumulated_tools.resize(idx + 1, ToolCallAccumulator::default());
                                        }
                                        let acc = &mut accumulated_tools[idx];
                                        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                            acc.id = Some(id.to_owned());
                                        }
                                        if let Some(func) = tc.get("function") {
                                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                                acc.name = Some(name.to_owned());
                                            }
                                            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                                acc.arguments.push_str(args);
                                            }
                                        }
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
            // Original Gemini implementation
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

            let url = format!("{}/models/{model}:streamGenerateContent", self.config.base_url);
            let response = self
                .agent
                .post(&url)
                .query("key", &self.config.api_key)
                .query("alt", "sse")
                .send_json(serde_json::to_value(request)?)
                .map_err(read_error)?;

            let reader = std::io::BufReader::new(response.into_reader());
            for line in reader.lines() {
                let line = line.context("failed to read stream line")?;
                if line.starts_with("data: ") {
                    let json_str = &line["data: ".len()..];
                    let chunk: GenerateContentResponse = serde_json::from_str(json_str)
                        .context("failed to parse stream chunk JSON")?;
                    if let Some(gemini_response) = chunk.into_response() {
                        on_chunk(gemini_response)?;
                    }
                }
            }

            Ok(())
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub parts: Vec<Part>,
}

impl ChatMessage {
    pub fn user(text: String) -> Self {
        Self {
            role: "user".to_owned(),
            parts: vec![serde_json::json!({ "text": text })],
        }
    }
}

pub type Part = serde_json::Value;

pub enum GeminiResponse {
    Turn(Vec<Part>),
}

#[derive(Debug, Deserialize)]
struct ListModelsResponse {
    models: Vec<Model>,
}

#[derive(Debug, Deserialize)]
struct Model {
    name: String,
    #[serde(rename = "supportedGenerationMethods", default)]
    supported_generation_methods: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Tool {
    function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct FunctionDeclaration {
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct GenerateContentRequest {
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content>,
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
}

#[derive(Debug, Serialize)]
struct Content {
    role: String,
    parts: Vec<Part>,
}

impl Content {
    fn from_message(message: &ChatMessage) -> Self {
        Self {
            role: message.role.clone(),
            parts: message.parts.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    candidates: Option<Vec<Candidate>>,
}

impl GenerateContentResponse {
    fn into_response(self) -> Option<GeminiResponse> {
        let parts = self.candidates?.into_iter().next()?.content?.parts;
        (!parts.is_empty()).then_some(GeminiResponse::Turn(parts))
    }
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Option<ResponseContent>,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    parts: Vec<Part>,
}

fn read_error(error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(code, response) => {
            let message = response
                .into_string()
                .unwrap_or_else(|_| "unknown error".to_owned());
            anyhow::anyhow!("Gemini request failed with HTTP {code}: {message}")
        }
        ureq::Error::Transport(error) => anyhow::anyhow!("Gemini request failed: {error}"),
    }
}
