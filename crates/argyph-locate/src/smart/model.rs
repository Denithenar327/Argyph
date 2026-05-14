//! Stub. Implemented in subsequent tasks.
use async_trait::async_trait;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ModelStep {
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    Final {
        selected_node_ids: Vec<String>,
        reasoning_summary: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum LocateModelError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("model output parse error: {0}")]
    Parse(String),
    #[error("rate limited; retry_after={retry_after_ms}ms")]
    RateLimit { retry_after_ms: u64 },
    #[error("budget exceeded: {0}")]
    Budget(String),
}

#[async_trait]
pub trait LocateModel: Send + Sync {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError>;
}
