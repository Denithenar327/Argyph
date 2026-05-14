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
    OutOfBudget,
    EmbedProviderError,
    LanguageUnsupported,
    SymbolNotFound,
    SymbolAmbiguous,
    Internal,
    LocateSmartDisabled,
    LocateSmartBudgetExceeded,
    ProviderError,
    StaleIndex,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::IndexNotReady => "INDEX_NOT_READY",
            Self::InvalidPath => "INVALID_PATH",
            Self::OutOfBudget => "OUT_OF_BUDGET",
            Self::EmbedProviderError => "EMBED_PROVIDER_ERROR",
            Self::LanguageUnsupported => "LANGUAGE_UNSUPPORTED",
            Self::SymbolNotFound => "SYMBOL_NOT_FOUND",
            Self::SymbolAmbiguous => "SYMBOL_AMBIGUOUS",
            Self::Internal => "INTERNAL",
            Self::LocateSmartDisabled => "LOCATE_SMART_DISABLED",
            Self::LocateSmartBudgetExceeded => "LOCATE_SMART_BUDGET_EXCEEDED",
            Self::ProviderError => "PROVIDER_ERROR",
            Self::StaleIndex => "STALE_INDEX",
        };
        write!(f, "{s}")
    }
}

pub fn correlation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn index_not_ready() -> McpErrorBody {
    McpErrorBody {
        code: ErrorCode::IndexNotReady,
        message: "Index not yet built; please retry".into(),
        retryable: true,
        retry_after_ms: Some(3000),
        correlation_id: Some(correlation_id()),
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
