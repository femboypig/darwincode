use serde::{Deserialize, Serialize};

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
pub(crate) struct ListModelsResponse {
    pub(crate) models: Vec<Model>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Model {
    pub(crate) name: String,
    #[serde(rename = "supportedGenerationMethods", default)]
    pub(crate) supported_generation_methods: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Tool {
    pub(crate) function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FunctionDeclaration {
    pub(crate) name: String,
    pub(crate) description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GenerateContentRequest {
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    pub(crate) system_instruction: Option<Content>,
    pub(crate) contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<Tool>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Content {
    pub(crate) role: String,
    pub(crate) parts: Vec<Part>,
}

impl Content {
    pub(crate) fn from_message(message: &ChatMessage) -> Self {
        Self {
            role: message.role.clone(),
            parts: message.parts.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GenerateContentResponse {
    pub(crate) candidates: Option<Vec<Candidate>>,
}

impl GenerateContentResponse {
    pub(crate) fn into_response(self) -> Option<GeminiResponse> {
        let parts = self.candidates?.into_iter().next()?.content?.parts;
        (!parts.is_empty()).then_some(GeminiResponse::Turn(parts))
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Candidate {
    pub(crate) content: Option<ResponseContent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponseContent {
    pub(crate) parts: Vec<Part>,
}
