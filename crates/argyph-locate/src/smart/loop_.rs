//! Bounded ReAct loop driver.

use crate::smart::model::{LocateModelError, Message, ModelStep, Role};
use crate::smart::prompts::{SYSTEM_PROMPT, user_message};
use crate::smart::tools::{dispatch, SubToolCtx, SubToolOutput};
use crate::smart::validate::SpanHistory;
use crate::types::{IndexCoverage, Span};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use crate::types::Strategy;
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
fn default_max_steps() -> u8 { 4 }
fn default_max_output_tokens() -> u32 { 1024 }

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
    BudgetExceeded { steps_taken: u8, partial: Option<SmartResponse> },
    ProviderError(String),
    FabricatedNodeIds(Vec<String>),
    Other(anyhow::Error),
}

pub async fn run(
    model: Arc<dyn LocateModel>,
    ctx: SubToolCtx,
    req: SmartRequest,
) -> Result<SmartResponse, SmartError> {
    let mut history = SpanHistory::default();
    let mut messages: Vec<Message> = vec![
        Message { role: Role::System, content: SYSTEM_PROMPT.into(), tool_call_id: None, tool_name: None },
        Message { role: Role::User,   content: user_message(&req.query), tool_call_id: None, tool_name: None },
    ];

    let mut steps_taken: u8 = 0;
    let max_steps = req.max_steps.max(1);

    loop {
        if steps_taken >= max_steps {
            return Err(SmartError::BudgetExceeded { steps_taken, partial: None });
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
            ModelStep::ToolCall { id, name, arguments } => {
                let result = dispatch(&ctx, &name, &arguments, 16_384).await;
                let (tool_msg, observed_spans) = match result {
                    Ok(SubToolOutput::Locate(resp)) => {
                        let spans = resp.spans.clone();
                        let body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".into());
                        (body, spans)
                    }
                    Ok(other) => (serde_json::to_string(&other).unwrap_or_else(|_| "{}".into()), Vec::new()),
                    Err(e) => (format!("{{\"error\":\"{e}\"}}"), Vec::new()),
                };
                history.record_many(observed_spans);
                messages.push(Message {
                    role: Role::Tool,
                    content: tool_msg,
                    tool_call_id: Some(id),
                    tool_name: Some(name),
                });
            }
            ModelStep::Final { selected_node_ids, reasoning_summary } => {
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
fn _strategy_marker() -> Strategy { Strategy::Hybrid }