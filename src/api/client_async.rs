use crate::api::types::{ChatMessage, Content, FunctionDeclaration, GeminiResponse};
use crate::config::StoredConfig;
use anyhow::Result;
use futures::StreamExt;
use std::pin::Pin;

type BoxStream = Pin<Box<dyn futures::Stream<Item = Result<GeminiResponse>> + Send>>;

pub struct AsyncGeminiClient {
    config: StoredConfig,
    client: reqwest::Client,
}

impl AsyncGeminiClient {
    #[allow(dead_code)]
    pub fn new(config: StoredConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(900))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create reqwest client");

        Self { config, client }
    }

    pub fn new_with_client(config: StoredConfig, client: reqwest::Client) -> Self {
        Self { config, client }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        if self.config.api_key.starts_with("sk-") {
            self.list_models_openai().await
        } else {
            self.list_models_gemini().await
        }
    }

    async fn list_models_gemini(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.config.base_url.trim_end_matches('/'));

        let response = crate::api::client::common::execute_with_retry_async(&self.client, |c| {
            c.get(&url).query(&[("key", &self.config.api_key)]).send()
        })
        .await?;

        if !response.status().is_success() {
            anyhow::bail!("API error: {}", response.status());
        }

        let body: serde_json::Value = response.json().await?;

        let models = body["models"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing models array"))?
            .iter()
            .filter_map(|m| {
                let name = m["name"].as_str()?;
                let methods = m["supportedGenerationMethods"].as_array()?;
                let supports_streaming = methods
                    .iter()
                    .any(|m| m.as_str().map(|s| s == "generateContent").unwrap_or(false));
                if supports_streaming {
                    Some(name.to_owned())
                } else {
                    None
                }
            })
            .collect();

        Ok(models)
    }

    async fn list_models_openai(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.config.base_url.trim_end_matches('/'));

        let response = crate::api::client::common::execute_with_retry_async(&self.client, |c| {
            c.get(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .send()
        })
        .await?;

        if !response.status().is_success() {
            anyhow::bail!("API error: {}", response.status());
        }

        let body: serde_json::Value = response.json().await?;

        let models = body["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing data array"))?
            .iter()
            .filter_map(|m| m["id"].as_str().map(|s| s.to_owned()))
            .collect();

        Ok(models)
    }

    pub async fn generate_stream(
        &self,
        history: &[ChatMessage],
        cancel_token: tokio_util::sync::CancellationToken,
        _dev_mode_label: &str,
        declarations: Vec<FunctionDeclaration>,
        system_instruction: Option<Content>,
    ) -> Result<BoxStream> {
        let mut active_model = self.config.model.trim_start_matches("models/").to_owned();

        if let Some(ref agent_id) = self.config.active_agent {
            let custom_agents = crate::app::load_custom_agents();
            if let Some(model_override) = custom_agents.get(agent_id).and_then(|a| a.model.as_ref())
            {
                active_model = model_override.trim_start_matches("models/").to_owned();
            }
        }

        if self.config.api_key.starts_with("sk-") {
            self.generate_stream_openai(
                &active_model,
                history,
                &declarations,
                &system_instruction,
                cancel_token,
            )
            .await
        } else {
            self.generate_stream_gemini(
                &active_model,
                history,
                &declarations,
                &system_instruction,
                cancel_token,
            )
            .await
        }
    }

    async fn generate_stream_gemini(
        &self,
        model: &str,
        history: &[ChatMessage],
        declarations: &[FunctionDeclaration],
        system_instruction: &Option<Content>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<BoxStream> {
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.config.base_url.trim_end_matches('/'),
            model,
            self.config.api_key
        );

        let mut contents = Vec::new();
        for msg in history {
            contents.push(Content {
                role: msg.role.clone(),
                parts: msg.parts.clone(),
            });
        }

        let tools = if !declarations.is_empty() {
            Some(vec![crate::api::types::Tool {
                function_declarations: declarations.to_vec(),
            }])
        } else {
            None
        };

        let request_body = crate::api::types::GenerateContentRequest {
            system_instruction: system_instruction.clone(),
            contents,
            tools,
        };

        let response = crate::api::client::common::execute_with_retry_async(&self.client, |c| {
            c.post(&url).json(&request_body).send()
        })
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            anyhow::bail!("API error {}: {}", status, error_text);
        }

        let byte_stream = response.bytes_stream();

        let stream = Box::pin(
            byte_stream
                .map(move |chunk_result| match chunk_result {
                    Ok(chunk) => {
                        let text = String::from_utf8_lossy(&chunk);

                        for line in text.lines() {
                            if let Some(parts_vec) = line
                                .strip_prefix("data: ")
                                .and_then(|data| {
                                    serde_json::from_str::<serde_json::Value>(data).ok()
                                })
                                .and_then(|parsed| {
                                    parsed["candidates"]
                                        .as_array()
                                        .and_then(|c| c.first())
                                        .and_then(|f| f["content"]["parts"].as_array())
                                        .map(|p| p.to_vec())
                                })
                            {
                                if !parts_vec.is_empty() {
                                    return Some(Ok(GeminiResponse::Turn(parts_vec)));
                                }
                            }
                        }
                        None
                    }
                    Err(e) => Some(Err(anyhow::anyhow!("Stream error: {}", e))),
                })
                .filter_map(|x| async move { x })
                .take_until({
                    let cancel_token = cancel_token.clone();
                    async move {
                        cancel_token.cancelled().await;
                    }
                }),
        );

        Ok(stream)
    }

    async fn generate_stream_openai(
        &self,
        model: &str,
        history: &[ChatMessage],
        declarations: &[FunctionDeclaration],
        system_instruction: &Option<Content>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<BoxStream> {
        let mut openai_tools = Vec::new();
        for decl in declarations {
            let mut params = decl.parameters.clone().unwrap_or(serde_json::json!({}));
            crate::api::client::common::lowercase_types(&mut params);

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
        if let Some(sys) = system_instruction
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
                    let supports_vision = crate::api::client::common::model_supports_vision(
                        model,
                        &self.config.base_url,
                    );
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
                                && let Some(data) = inline_data.get("data").and_then(|v| v.as_str())
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
                            let is_responded =
                                if let Some(pos) = responded_names.iter().position(|n| n == name) {
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
                            if let Some(pos) = tool_call_ids.iter().position(|(n, _)| n == name) {
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

        let url = format!("{}/chat/completions", self.config.base_url);
        let response = crate::api::client::common::execute_with_retry_async(&self.client, |c| {
            c.post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .json(&request)
                .send()
        })
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            anyhow::bail!("API error {}: {}", status, error_text);
        }

        let byte_stream = response.bytes_stream();

        #[derive(Default, Clone)]
        struct ToolCallAccumulator {
            id: Option<String>,
            name: Option<String>,
            arguments: String,
        }

        let mut accumulated_tools: Vec<ToolCallAccumulator> = Vec::new();
        let mut finished = false;

        let stream = Box::pin(
            byte_stream
                .map(move |chunk_result| {
                    if finished {
                        return None;
                    }
                    match chunk_result {
                        Ok(chunk) => {
                            let text = String::from_utf8_lossy(&chunk);
                            let mut responses = Vec::new();

                            for line in text.lines() {
                                let line = line.trim();
                                if line.is_empty() {
                                    continue;
                                }
                                if line == "data: [DONE]" {
                                    finished = true;
                                    break;
                                }
                                if let Some(stripped) = line.strip_prefix("data: ") {
                                    let json_str = stripped.trim();
                                    if json_str == "[DONE]" {
                                        finished = true;
                                        break;
                                    }
                                    if let Ok(chunk_val) =
                                        serde_json::from_str::<serde_json::Value>(json_str)
                                    {
                                        if let Some(err) = chunk_val.get("error") {
                                            let msg = err
                                                .get("message")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown error");
                                            return Some(Err(anyhow::anyhow!(
                                                "API Error: {}",
                                                msg
                                            )));
                                        }

                                        if let Some(choices) =
                                            chunk_val.get("choices").and_then(|v| v.as_array())
                                            && let Some(choice) = choices.first()
                                            && let Some(delta) = choice.get("delta")
                                        {
                                            if let Some(content) =
                                                delta.get("content").and_then(|v| v.as_str())
                                                && !content.is_empty()
                                            {
                                                responses.push(serde_json::json!({
                                                    "text": content
                                                }));
                                            }
                                            let reasoning = delta
                                                .get("reasoning_content")
                                                .or_else(|| delta.get("reasoning"))
                                                .and_then(|v| v.as_str());
                                            if let Some(reasoning) = reasoning
                                                && !reasoning.is_empty()
                                            {
                                                responses.push(serde_json::json!({
                                                    "text": reasoning,
                                                    "thought": true,
                                                    "reasoning_content": reasoning
                                                }));
                                            }

                                            if let Some(tool_calls) =
                                                delta.get("tool_calls").and_then(|v| v.as_array())
                                            {
                                                for tc in tool_calls {
                                                    let idx = tc
                                                        .get("index")
                                                        .and_then(|v| v.as_u64())
                                                        .unwrap_or(0)
                                                        as usize;
                                                    if idx >= accumulated_tools.len() {
                                                        accumulated_tools.resize(
                                                            idx + 1,
                                                            ToolCallAccumulator::default(),
                                                        );
                                                    }
                                                    let acc = &mut accumulated_tools[idx];
                                                    if let Some(id) =
                                                        tc.get("id").and_then(|v| v.as_str())
                                                    {
                                                        acc.id = Some(id.to_owned());
                                                    }
                                                    if let Some(func) = tc.get("function") {
                                                        if let Some(name) = func
                                                            .get("name")
                                                            .and_then(|v| v.as_str())
                                                        {
                                                            acc.name = Some(name.to_owned());
                                                        }
                                                        if let Some(args) = func
                                                            .get("arguments")
                                                            .and_then(|v| v.as_str())
                                                        {
                                                            acc.arguments.push_str(args);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if finished {
                                for acc in accumulated_tools.drain(..) {
                                    if let Some(name) = acc.name {
                                        let args: serde_json::Value =
                                            serde_json::from_str(&acc.arguments)
                                                .unwrap_or_else(|_| serde_json::json!({}));
                                        responses.push(serde_json::json!({
                                            "functionCall": {
                                                "name": name,
                                                "args": args
                                            }
                                        }));
                                    }
                                }
                            }

                            if !responses.is_empty() {
                                Some(Ok(GeminiResponse::Turn(responses)))
                            } else {
                                None
                            }
                        }
                        Err(e) => Some(Err(anyhow::anyhow!("Stream error: {}", e))),
                    }
                })
                .filter_map(|x| async move { x })
                .take_until({
                    let cancel_token = cancel_token.clone();
                    async move {
                        cancel_token.cancelled().await;
                    }
                }),
        );

        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_client_creation() {
        let config = StoredConfig {
            api_key: "test_key".to_owned(),
            model: "gemini-2.0-flash".to_owned(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_owned(),
            ..Default::default()
        };

        let _client = AsyncGeminiClient::new(config);
    }
}
