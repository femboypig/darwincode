use anyhow::{Context, Result};
use std::io::BufRead;
use crate::api::types::{
    ChatMessage, Content, FunctionDeclaration, GeminiResponse, GenerateContentRequest,
    GenerateContentResponse, ListModelsResponse, Tool,
};
use crate::config::StoredConfig;
use super::common::execute_with_retry;

pub fn list_models_gemini(config: &StoredConfig, agent: &ureq::Agent) -> Result<Vec<String>> {
    let url = format!("{}/models", config.base_url);
    let response = execute_with_retry(agent, |a| {
        a.get(&url)
            .set("x-goog-api-key", &config.api_key)
            .call()
    })?;

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

pub fn generate_stream_gemini(
    config: &StoredConfig,
    agent: &ureq::Agent,
    model: &str,
    history: &[ChatMessage],
    declarations: &[FunctionDeclaration],
    system_instruction: &Option<Content>,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    mut on_chunk: impl FnMut(GeminiResponse) -> Result<()>,
) -> Result<()> {
    let mut tools = Vec::new();
    if !declarations.is_empty() {
        tools.push(Tool {
            function_declarations: declarations.to_vec(),
        });
    }

    let request = GenerateContentRequest {
        system_instruction: system_instruction.clone(),
        contents: history.iter().map(Content::from_message).collect(),
        tools: if tools.is_empty() { None } else { Some(tools) },
    };

    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        anyhow::bail!("Stream cancelled");
    }

    let url = format!(
        "{}/models/{model}:streamGenerateContent",
        config.base_url
    );
    let response = execute_with_retry(agent, |a| {
        a.post(&url)
            .set("x-goog-api-key", &config.api_key)
            .query("alt", "sse")
            .send_json(serde_json::to_value(&request).unwrap())
    })?;

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
