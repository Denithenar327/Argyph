//! OpenAI / OpenAI-compatible provider.

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OpenAiModel {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    client: reqwest::Client,
}

impl OpenAiModel {
    pub fn from_env(model: String, endpoint: Option<String>) -> Result<Self, LocateModelError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| LocateModelError::Provider("OPENAI_API_KEY not set".into()))?;
        Ok(Self {
            api_key,
            model,
            endpoint: endpoint.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into()),
            client: reqwest::Client::new(),
        })
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    response_format: ResponseFormat,
}
#[derive(Serialize)]
struct ChatMessage { role: String, content: String }
#[derive(Serialize)]
struct ResponseFormat { #[serde(rename = "type")] kind: String }

#[derive(Deserialize)]
struct ChatResponse { choices: Vec<Choice> }
#[derive(Deserialize)]
struct Choice { message: ChoiceMessage }
#[derive(Deserialize)]
struct ChoiceMessage { content: String }

#[async_trait]
impl LocateModel for OpenAiModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        let chat_msgs: Vec<ChatMessage> = messages.iter().map(|m| ChatMessage {
            role: match m.role {
                Role::System    => "system",
                Role::User      => "user",
                Role::Assistant => "assistant",
                Role::Tool      => "tool",
            }.to_string(),
            content: m.content.clone(),
        }).collect();

        let body = ChatRequest {
            model: &self.model,
            messages: &chat_msgs,
            temperature: 0.0,
            response_format: ResponseFormat { kind: "json_object".into() },
        };

        let resp = self.client.post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await
            .map_err(|e| LocateModelError::Provider(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry = resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(2);
            return Err(LocateModelError::RateLimit { retry_after_ms: retry * 1000 });
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(LocateModelError::Provider(format!("HTTP {status}: {text}")));
        }

        let parsed: ChatResponse = resp.json().await
            .map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let raw = parsed.choices.into_iter().next()
            .ok_or_else(|| LocateModelError::Parse("no choices".into()))?
            .message.content;

        parse_model_output(&raw)
    }
}

pub(crate) fn parse_model_output(raw: &str) -> Result<ModelStep, LocateModelError> {
    let v: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| LocateModelError::Parse(format!("not JSON: {e}: {raw}")))?;
    if let Some(t) = v.get("tool") {
        let name = t.get("name").and_then(|x| x.as_str())
            .ok_or_else(|| LocateModelError::Parse("tool.name missing".into()))?
            .to_string();
        let arguments = t.get("arguments").cloned().unwrap_or(serde_json::json!({}));
        return Ok(ModelStep::ToolCall {
            id: format!("call_{}", rand_id()),
            name, arguments,
        });
    }
    if let Some(f) = v.get("final") {
        let ids: Vec<String> = serde_json::from_value(
            f.get("selected_node_ids").cloned().unwrap_or(serde_json::json!([]))
        ).map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let summary = f.get("reasoning_summary")
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        return Ok(ModelStep::Final { selected_node_ids: ids, reasoning_summary: summary });
    }
    Err(LocateModelError::Parse(format!("expected `tool` or `final` key: {raw}")))
}

fn rand_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::ZERO).as_nanos().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    #[test] fn parses_tool_call() {
        let raw = r#"{"tool":{"name":"locate","arguments":{"query":"x"}}}"#;
        let step = parse_model_output(raw).unwrap();
        assert!(matches!(step, ModelStep::ToolCall { ref name, .. } if name == "locate"));
    }
    #[test] fn parses_final() {
        let raw = r#"{"final":{"selected_node_ids":["n1"],"reasoning_summary":"r"}}"#;
        let step = parse_model_output(raw).unwrap();
        assert!(matches!(step, ModelStep::Final { ref selected_node_ids, .. } if selected_node_ids == &["n1".to_string()]));
    }
    #[test] fn rejects_unknown_shape() {
        assert!(parse_model_output(r#"{"hello":"world"}"#).is_err());
    }
}