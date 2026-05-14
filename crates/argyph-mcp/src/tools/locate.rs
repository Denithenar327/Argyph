use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::Serialize;

use argyph_core::Supervisor;

use crate::error::{ErrorCode, McpErrorBody};

pub use argyph_locate::Request as LocateRequest;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LocateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans: Option<Vec<SpanData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_coverage: Option<IndexCoverageData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}



#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SpanData {
    pub file: String,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub kind: String,
    pub path: Vec<String>,
    pub content: String,
    pub score: f32,
    pub truncated: bool,
    pub expand_to: ExpandToData,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ExpandToData {
    pub parent: Option<ExpandTargetData>,
    pub file: Option<ExpandTargetData>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ExpandTargetData {
    pub node_id: Option<String>,
    pub label: Option<String>,
    pub bytes: u32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct IndexCoverageData {
    pub tier_1_5: String,
    pub tier_2: String,
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    root: &Utf8PathBuf,
    req: LocateRequest,
) -> LocateResponse {
    let store = supervisor.store();

    let Some(embedder) = supervisor.embedder() else {
        return LocateResponse {
            spans: None,
            strategy_used: None,
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

    match argyph_locate::locate(store, embedder, root.as_std_path(), req).await {
        Ok(resp) => {
            let strategy_str = serde_json::to_value(resp.strategy_used)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", resp.strategy_used).to_lowercase());
            LocateResponse {
                spans: Some(
                    resp.spans
                        .into_iter()
                        .map(|s| SpanData {
                            file: s.file,
                            byte_range: s.byte_range,
                            line_range: s.line_range,
                            kind: s.kind,
                            path: s.path,
                            content: s.content,
                            score: s.score,
                            truncated: s.truncated,
                            expand_to: ExpandToData {
                                parent: s.expand_to.parent.map(|p| ExpandTargetData {
                                    node_id: p.node_id,
                                    label: p.label,
                                    bytes: p.bytes,
                                }),
                                file: s.expand_to.file.map(|f| ExpandTargetData {
                                    node_id: f.node_id,
                                    label: f.label,
                                    bytes: f.bytes,
                                }),
                            },
                        })
                        .collect(),
                ),
                strategy_used: Some(strategy_str),
                index_coverage: Some(IndexCoverageData {
                    tier_1_5: resp.index_coverage.tier_1_5,
                    tier_2: resp.index_coverage.tier_2,
                }),
                error: None,
            }
        }
        Err(e) => {
            let msg = e.to_string();
            let code = if msg.starts_with("INVALID_ARGUMENT") {
                ErrorCode::InvalidPath
            } else if msg.starts_with("INDEX_NOT_READY") {
                ErrorCode::IndexNotReady
            } else {
                ErrorCode::Internal
            };
            LocateResponse {
                spans: None,
                strategy_used: None,
                index_coverage: None,
                error: Some(McpErrorBody {
                    code,
                    message: msg,
                    retryable: false,
                    retry_after_ms: None,
                    correlation_id: None,
                }),
            }
        }
    }
}
