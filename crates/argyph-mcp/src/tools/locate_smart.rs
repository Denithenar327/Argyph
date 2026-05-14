use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;
use argyph_locate::smart::{SmartError, SmartRequest, SubToolCtx};

use crate::error::{ErrorCode, McpErrorBody};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LocateSmartRequest {
    pub query: String,
    #[serde(default = "default_max_steps")]
    pub max_steps: u8,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
}

fn default_max_steps() -> u8 {
    4
}
fn default_max_output_tokens() -> u32 {
    1024
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LocateSmartResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps_taken: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_coverage: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    _root: &camino::Utf8PathBuf,
    req: LocateSmartRequest,
) -> LocateSmartResponse {
    let cfg = &supervisor.config().locate_smart;
    if !cfg.enabled {
        return LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::LocateSmartDisabled,
                message: "locate_smart is disabled in this Argyph configuration".into(),
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        };
    }

    let Some(provider) = cfg.provider.as_deref() else {
        return LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::ProviderError,
                message: "locate_smart.provider not set".into(),
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        };
    };

    let Some(model_id) = cfg.model.as_deref() else {
        return LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::ProviderError,
                message: "locate_smart.model not set".into(),
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        };
    };

    let model = match argyph_locate::smart::build_model(provider, model_id, cfg.endpoint.clone()) {
        Ok(m) => m,
        Err(e) => {
            return LocateSmartResponse {
                spans: None,
                strategy_used: None,
                reasoning_summary: None,
                steps_taken: None,
                index_coverage: None,
                error: Some(McpErrorBody {
                    code: ErrorCode::ProviderError,
                    message: e.to_string(),
                    retryable: false,
                    retry_after_ms: None,
                    correlation_id: None,
                }),
            };
        }
    };

    let store = supervisor.store();
    let Some(embedder) = supervisor.embedder() else {
        return LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::IndexNotReady,
                message: "Embedder not ready".into(),
                retryable: true,
                retry_after_ms: Some(1000),
                correlation_id: None,
            }),
        };
    };

    let ctx = SubToolCtx {
        store,
        embedder,
        root: supervisor.root().as_std_path().to_path_buf(),
    };

    let smart_req = SmartRequest {
        query: req.query,
        max_steps: req.max_steps,
        max_output_tokens: req.max_output_tokens,
    };

    match argyph_locate::smart::run(model, ctx, smart_req).await {
        Ok(resp) => {
            let spans_json = serde_json::to_value(&resp.spans).unwrap_or(serde_json::json!([]));
            let coverage_json = serde_json::json!({
                "tier_1_5": resp.index_coverage.tier_1_5,
                "tier_2": resp.index_coverage.tier_2,
            });
            LocateSmartResponse {
                spans: Some(spans_json),
                strategy_used: Some(resp.strategy_used.to_string()),
                reasoning_summary: Some(resp.reasoning_summary),
                steps_taken: Some(resp.steps_taken),
                index_coverage: Some(coverage_json),
                error: None,
            }
        }
        Err(SmartError::BudgetExceeded {
            steps_taken,
            partial: _,
        }) => LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: Some(steps_taken),
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::LocateSmartBudgetExceeded,
                message: format!("step budget exhausted after {steps_taken} steps"),
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        },
        Err(SmartError::FabricatedNodeIds(ids)) => LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::Internal,
                message: format!(
                    "model returned node_ids not produced in this loop: {ids:?}"
                ),
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        },
        Err(SmartError::ProviderError(e)) => LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::ProviderError,
                message: e,
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        },
        Err(SmartError::Other(e)) => LocateSmartResponse {
            spans: None,
            strategy_used: None,
            reasoning_summary: None,
            steps_taken: None,
            index_coverage: None,
            error: Some(McpErrorBody {
                code: ErrorCode::Internal,
                message: e.to_string(),
                retryable: false,
                retry_after_ms: None,
                correlation_id: None,
            }),
        },
    }
}