//! Tool error JSON (spec ch. 10): `{"error": {"code", "message", "hint"?}}`.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ToolError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        ToolError {
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGS", message)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "error": self })
    }
}
