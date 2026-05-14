//! LLM provider abstraction for the locate_smart ReAct loop.
use async_trait::async_trait;

/// API key wrapper that redacts itself in Debug/Display so it cannot be
/// accidentally logged. Exposes the raw value only via `expose()`, which is
/// the only construction route into header values.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The raw secret. Treat the returned `&str` as sensitive.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKey(***redacted***)")
    }
}

impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "***redacted***")
    }
}

/// Scrub an API key substring out of arbitrary text (e.g. a provider error
/// body that echoes the Authorization header). Replaces every occurrence with
/// the redaction marker.
pub fn redact(haystack: &str, key: &ApiKey) -> String {
    if key.0.is_empty() {
        return haystack.to_string();
    }
    haystack.replace(&key.0, "***redacted***")
}

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
