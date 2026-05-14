//! Anthropic Messages API provider.

#![cfg(feature = "smart")]

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use crate::smart::providers::openai::parse_model_output;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct AnthropicModel {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    client: reqwest::Client,
}

impl AnthropicModel {
    pub fn from_env(model: String, endpoint: Option<String>) -> Result<Self, LocateModelError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| LocateModelError::Provider("ANTHROPIC_API_KEY not set".into()))?;
        Ok(Self {
            api_key, model,
            endpoint: endpoint.unwrap_or_else(|| "https://api.anthropic.com/v1/messages".into()),
            client: reqwest::Client::new(),
        })
    }
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    system: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    temperature: f32,
}
#[derive(Serialize)]
struct AnthropicMessage { role: String, content: String }

#[derive(Deserialize)]
struct AnthropicResponse { content: Vec<AnthropicContent> }
#[derive(Deserialize)]
struct AnthropicContent { #[serde(rename = "type")] _kind: String, text: String }

#[async_trait]
impl LocateModel for AnthropicModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        let mut system = String::new();
        let mut converted = Vec::new();
        for m in messages {
            match m.role {
                Role::System    => system.push_str(&m.content),
                Role::Tool      => converted.push(AnthropicMessage {
                    role: "user".into(),
                    content: format!("[tool:{}] {}", m.tool_name.as_deref().unwrap_or(""), m.content),
                }),
                Role::User      => converted.push(AnthropicMessage { role: "user".into(),      content: m.content.clone() }),
                Role::Assistant => converted.push(AnthropicMessage { role: "assistant".into(), content: m.content.clone() }),
            }
        }

        let body = AnthropicRequest {
            model: &self.model, system,
            messages: converted, max_tokens: 1024, temperature: 0.0,
        };
        let resp = self.client.post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body).send().await
            .map_err(|e| LocateModelError::Provider(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LocateModelError::RateLimit { retry_after_ms: 2000 });
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(LocateModelError::Provider(format!("HTTP {status}: {text}")));
        }

        let parsed: AnthropicResponse = resp.json().await
            .map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let raw = parsed.content.into_iter().next()
            .ok_or_else(|| LocateModelError::Parse("empty content".into()))?
            .text;
        parse_model_output(&raw)
    }
}