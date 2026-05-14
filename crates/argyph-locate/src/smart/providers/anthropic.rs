//! Anthropic Messages API provider.

use crate::smart::model::{
    redact, ApiKey, LocateModel, LocateModelError, Message, ModelStep, Role,
};
use crate::smart::providers::openai::parse_model_output;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

const MAX_RETRIES: u32 = 3;

pub struct AnthropicModel {
    api_key: ApiKey,
    model: String,
    endpoint: String,
    client: reqwest::Client,
}

impl AnthropicModel {
    pub fn from_env(model: String, endpoint: Option<String>) -> Result<Self, LocateModelError> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| LocateModelError::Provider("ANTHROPIC_API_KEY not set".into()))?;
        Ok(Self {
            api_key: ApiKey::new(key),
            model,
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
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}
#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    _kind: String,
    text: String,
}

#[async_trait]
impl LocateModel for AnthropicModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        let mut system = String::new();
        let mut converted = Vec::new();
        for m in messages {
            match m.role {
                Role::System => system.push_str(&m.content),
                Role::Tool => converted.push(AnthropicMessage {
                    role: "user".into(),
                    content: format!(
                        "[tool:{}] {}",
                        m.tool_name.as_deref().unwrap_or(""),
                        m.content
                    ),
                }),
                Role::User => converted.push(AnthropicMessage {
                    role: "user".into(),
                    content: m.content.clone(),
                }),
                Role::Assistant => converted.push(AnthropicMessage {
                    role: "assistant".into(),
                    content: m.content.clone(),
                }),
            }
        }

        let body = AnthropicRequest {
            model: &self.model,
            system,
            messages: converted,
            max_tokens: 1024,
            temperature: 0.0,
        };

        let mut attempt: u32 = 0;
        loop {
            let resp = self
                .client
                .post(&self.endpoint)
                .header("x-api-key", self.api_key.expose())
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
                .map_err(|e| LocateModelError::Provider(redact(&e.to_string(), &self.api_key)))?;

            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(2);
                return Err(LocateModelError::RateLimit {
                    retry_after_ms: retry * 1000,
                });
            }
            if status.is_server_error() && attempt < MAX_RETRIES {
                attempt += 1;
                let backoff_ms = 200u64 * (1u64 << (attempt - 1));
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                continue;
            }
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                let scrubbed = redact(&text, &self.api_key);
                return Err(LocateModelError::Provider(format!(
                    "HTTP {status}: {scrubbed}"
                )));
            }

            let parsed: AnthropicResponse = resp
                .json()
                .await
                .map_err(|e| LocateModelError::Parse(redact(&e.to_string(), &self.api_key)))?;
            let raw = parsed
                .content
                .into_iter()
                .next()
                .ok_or_else(|| LocateModelError::Parse("empty content".into()))?
                .text;
            return parse_model_output(&raw);
        }
    }
}
