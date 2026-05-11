use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct McpErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    IndexNotReady,
    InvalidPath,
    Internal,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexNotReady => write!(f, "INDEX_NOT_READY"),
            Self::InvalidPath => write!(f, "INVALID_PATH"),
            Self::Internal => write!(f, "INTERNAL"),
        }
    }
}

pub fn correlation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn index_not_ready() -> McpErrorBody {
    let cid = correlation_id();
    McpErrorBody {
        code: ErrorCode::IndexNotReady,
        message: "Index not yet built; please retry".into(),
        retryable: true,
        retry_after_ms: Some(3000),
        correlation_id: Some(cid),
    }
}

pub fn internal(msg: impl Into<String>) -> McpErrorBody {
    McpErrorBody {
        code: ErrorCode::Internal,
        message: msg.into(),
        retryable: false,
        retry_after_ms: None,
        correlation_id: Some(correlation_id()),
    }
}
