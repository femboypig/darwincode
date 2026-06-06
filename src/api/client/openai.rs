use anyhow::{Context, Result};
use std::io::BufRead;
use crate::api::types::{ChatMessage, Content, GeminiResponse, FunctionDeclaration};
use crate::config::StoredConfig;
use super::common::{lowercase_types, model_supports_vision, execute_with_retry};

pub fn list_models_openai(config: &StoredConfig, agent: &ureq::Agent) -> Result<Vec<String>> {
    let url = format!("{}/models", config.base_url);
    let response = execute_with_retry(agent, |a| {
        a.get(&url)
            .set("Authorization", &format!("Bearer {}", config.api_key))
            .call()
    })?;

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
}

pub fn generate_stream_openai(
    config: &StoredConfig,
    agent: &ureq::Agent,
    model: &str,
    history: &[ChatMessage],
    declarations: &[FunctionDeclaration],
    system_instruction: &Option<Content>,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    mut on_chunk: impl FnMut(GeminiResponse) -> Result<()>,
) -> Result<()> {
    let mut openai_tools = Vec::new();
    for decl in declarations {
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
                let supports_vision = model_supports_vision(model, &config.base_url);
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

    let url = format!("{}/chat/completions", config.base_url);
    let response = execute_with_retry(agent, |a| {
        a.post(&url)
            .set("Authorization", &format!("Bearer {}", config.api_key))
            .send_json(request.clone())
    })?;

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
}
