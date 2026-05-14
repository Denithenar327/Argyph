//! Ollama / llama.cpp-compatible local provider.
//! Uses the OpenAI-compatible chat completions endpoint.

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use crate::smart::providers::openai::parse_model_output;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OllamaModel {
    pub model: String,
    pub endpoint: String,
    client: reqwest::Client,
}

impl OllamaModel {
    pub fn new(model: String, endpoint: Option<String>) -> Self {
        Self {
            model,
            endpoint: endpoint.unwrap_or_else(|| "http://localhost:11434/v1/chat/completions".into()),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ChatReq<'a> {
    model: &'a str,
    messages: Vec<ChatMsg>,
    temperature: f32,
    stream: bool,
}
#[derive(Serialize)]
struct ChatMsg { role: String, content: String }

#[derive(Deserialize)]
struct ChatResp { choices: Vec<Choice> }
#[derive(Deserialize)]
struct Choice { message: Msg }
#[derive(Deserialize)]
struct Msg { content: String }

#[async_trait]
impl LocateModel for OllamaModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        let msgs: Vec<ChatMsg> = messages.iter().map(|m| ChatMsg {
            role: match m.role {
                Role::System    => "system",
                Role::User      => "user",
                Role::Assistant => "assistant",
                Role::Tool      => "user",
            }.to_string(),
            content: m.content.clone(),
        }).collect();

        let body = ChatReq { model: &self.model, messages: msgs, temperature: 0.0, stream: false };
        let resp = self.client.post(&self.endpoint).json(&body).send().await
            .map_err(|e| LocateModelError::Provider(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(LocateModelError::Provider(format!("HTTP {status}: {text}")));
        }
        let parsed: ChatResp = resp.json().await
            .map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let raw = parsed.choices.into_iter().next()
            .ok_or_else(|| LocateModelError::Parse("no choices".into()))?
            .message.content;
        parse_model_output(&raw)
    }
}