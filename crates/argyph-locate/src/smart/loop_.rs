//! Bounded ReAct loop driver.

use crate::smart::model::{LocateModelError, Message, ModelStep, Role};
use crate::smart::prompts::{user_message, SYSTEM_PROMPT};
use crate::smart::tools::{dispatch, SubToolCtx, SubToolOutput};
use crate::smart::validate::SpanHistory;
#[cfg(test)]
use crate::types::Strategy;
use crate::types::{IndexCoverage, Span};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::smart::model::LocateModel;

#[derive(Debug, Clone, Deserialize)]
pub struct SmartRequest {
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

#[derive(Debug, Clone, Serialize)]
pub struct SmartResponse {
    pub spans: Vec<Span>,
    pub strategy_used: &'static str,
    pub reasoning_summary: String,
    pub steps_taken: u8,
    pub index_coverage: IndexCoverage,
}

#[derive(Debug)]
pub enum SmartError {
    /// Step or token budget exhausted. `partial` contains every span the loop
    /// has observed up to the point of exhaustion (deduped) so the caller can
    /// still surface useful results.
    BudgetExceeded {
        steps_taken: u8,
        partial: Option<SmartResponse>,
    },
    ProviderError(String),
    FabricatedNodeIds(Vec<String>),
    Other(anyhow::Error),
}

/// Cheap token estimate. Real tokenizers vary, but ~4 bytes/token is a stable
/// upper-bound heuristic across BPE family models.
fn approx_tokens(s: &str) -> u32 {
    s.len().div_ceil(4) as u32
}

fn build_partial(history: &SpanHistory, steps_taken: u8, reason: &str) -> Option<SmartResponse> {
    let spans = history.all();
    if spans.is_empty() {
        return None;
    }
    Some(SmartResponse {
        spans,
        strategy_used: "smart",
        reasoning_summary: format!("partial result: {reason}"),
        steps_taken,
        index_coverage: IndexCoverage {
            tier_1_5: "ready".into(),
            tier_2: "ready".into(),
        },
    })
}

pub async fn run(
    model: Arc<dyn LocateModel>,
    ctx: SubToolCtx,
    req: SmartRequest,
) -> Result<SmartResponse, SmartError> {
    let mut history = SpanHistory::default();
    let mut messages: Vec<Message> = vec![
        Message {
            role: Role::System,
            content: SYSTEM_PROMPT.into(),
            tool_call_id: None,
            tool_name: None,
        },
        Message {
            role: Role::User,
            content: user_message(&req.query),
            tool_call_id: None,
            tool_name: None,
        },
    ];

    let mut steps_taken: u8 = 0;
    let max_steps = req.max_steps.max(1);
    let max_tokens = req.max_output_tokens.max(64);
    let mut tokens_consumed: u32 = 0;

    loop {
        if steps_taken >= max_steps {
            return Err(SmartError::BudgetExceeded {
                steps_taken,
                partial: build_partial(&history, steps_taken, "step budget exhausted"),
            });
        }
        if tokens_consumed >= max_tokens {
            return Err(SmartError::BudgetExceeded {
                steps_taken,
                partial: build_partial(&history, steps_taken, "token budget exhausted"),
            });
        }
        steps_taken += 1;

        let step = match model.step(&messages).await {
            Ok(s) => s,
            Err(LocateModelError::RateLimit { retry_after_ms }) => {
                tokio::time::sleep(std::time::Duration::from_millis(retry_after_ms)).await;
                continue;
            }
            Err(e) => return Err(SmartError::ProviderError(e.to_string())),
        };

        match step {
            ModelStep::ToolCall {
                id,
                name,
                arguments,
            } => {
                let result = dispatch(&ctx, &name, &arguments, 16_384).await;
                let (tool_msg, observed_spans) = match result {
                    Ok(SubToolOutput::Locate(resp)) => {
                        let spans = resp.spans.clone();
                        let body = match serde_json::to_string(&resp) {
                            Ok(s) => s,
                            Err(e) => {
                                return Err(SmartError::Other(anyhow::anyhow!(
                                    "failed to serialize locate output: {e}"
                                )));
                            }
                        };
                        (body, spans)
                    }
                    Ok(other) => {
                        let body = match serde_json::to_string(&other) {
                            Ok(s) => s,
                            Err(e) => {
                                return Err(SmartError::Other(anyhow::anyhow!(
                                    "failed to serialize sub-tool output: {e}"
                                )));
                            }
                        };
                        (body, Vec::new())
                    }
                    Err(e) => {
                        let scrubbed = e.to_string().replace('"', "'");
                        (format!("{{\"error\":\"{scrubbed}\"}}"), Vec::new())
                    }
                };
                tokens_consumed = tokens_consumed.saturating_add(approx_tokens(&tool_msg));
                history.record_many(observed_spans);
                messages.push(Message {
                    role: Role::Tool,
                    content: tool_msg,
                    tool_call_id: Some(id),
                    tool_name: Some(name),
                });
            }
            ModelStep::Final {
                selected_node_ids,
                reasoning_summary,
            } => {
                let _ = tokens_consumed.saturating_add(approx_tokens(&reasoning_summary));
                return match history.resolve(&selected_node_ids) {
                    Ok(spans) => Ok(SmartResponse {
                        spans,
                        strategy_used: "smart",
                        reasoning_summary,
                        steps_taken,
                        index_coverage: IndexCoverage {
                            tier_1_5: "ready".into(),
                            tier_2: "ready".into(),
                        },
                    }),
                    Err(missing) => Err(SmartError::FabricatedNodeIds(missing)),
                };
            }
        }
    }
}

#[cfg(test)]
fn _strategy_marker() -> Strategy {
    Strategy::Hybrid
}
