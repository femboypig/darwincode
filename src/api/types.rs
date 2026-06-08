use serde::Serialize;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub parts: Vec<Part>,
}

impl ChatMessage {
    #[allow(dead_code)]
    pub fn user(text: String) -> Self {
        Self {
            role: "user".to_owned(),
            parts: vec![serde_json::json!({ "text": text })],
        }
    }
}

pub type Part = serde_json::Value;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum GeminiResponse {
    Turn(Vec<Part>),
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Tool {
    pub(crate) function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FunctionDeclaration {
    pub(crate) name: String,
    pub(crate) description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parameters: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GenerateContentRequest {
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    pub(crate) system_instruction: Option<Content>,
    pub(crate) contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<Tool>>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Content {
    pub(crate) role: String,
    pub(crate) parts: Vec<Part>,
}
