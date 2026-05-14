use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing;

use crate::api_key::ApiKey;
use crate::config::EmbedConfig;
use crate::error::{EmbedError, Result};

const VOYAGE_BASE_URL: &str = "https://api.voyageai.com";
const MAX_TOKENS_PER_TEXT: usize = 16384;
const MAX_BATCH_SIZE: usize = 128;
const DEFAULT_MODEL: &str = "voyage-code-2";

#[derive(Serialize)]
struct VoyageEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
    input_type: &'a str,
}

#[derive(Deserialize)]
struct VoyageEmbedResponse {
    data: Vec<VoyageEmbeddingData>,
}

#[derive(Deserialize)]
struct VoyageEmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct VoyageErrorResponse {
    detail: Option<String>,
}

pub struct VoyageEmbedder {
    api_key: ApiKey,
    client: reqwest::Client,
    config: EmbedConfig,
    model: String,
}

impl VoyageEmbedder {
    pub fn new(config: EmbedConfig) -> Result<Self> {
        let api_key = ApiKey::from_env("VOYAGE_API_KEY")?;
        Self::with_api_key(config, api_key)
    }

    pub fn with_api_key(config: EmbedConfig, api_key: ApiKey) -> Result<Self> {
        let client = crate::http::build_client(&config)
            .map_err(|e| EmbedError::Config(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            client,
            config,
            model: DEFAULT_MODEL.to_string(),
        })
    }

    fn dimension_for_model(model: &str) -> usize {
        match model {
            "voyage-code-3" => 1024,
            "voyage-large-2" => 512,
            _ => 1536,
        }
    }

    fn base_url(&self) -> &str {
        self.config.base_url.as_deref().unwrap_or(VOYAGE_BASE_URL)
    }

    fn truncate_text(text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= MAX_TOKENS_PER_TEXT {
            text.to_string()
        } else {
            words[..MAX_TOKENS_PER_TEXT].join(" ")
        }
    }
}

#[async_trait::async_trait]
impl crate::Embedder for VoyageEmbedder {
    fn dimension(&self) -> usize {
        Self::dimension_for_model(&self.model)
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Err(EmbedError::EmptyInput);
        }

        if self.config.batch_size > MAX_BATCH_SIZE {
            return Err(EmbedError::BatchTooLarge {
                batch_size: self.config.batch_size,
                max_batch_size: MAX_BATCH_SIZE,
            });
        }

        let truncated: Vec<String> = texts.iter().map(|t| Self::truncate_text(t)).collect();
        let url = format!("{}/v1/embeddings", self.base_url());

        let mut all_embeddings: Vec<Option<Vec<f32>>> = vec![None; texts.len()];

        for (batch_idx, chunk) in truncated.chunks(self.config.batch_size).enumerate() {
            let batch: Vec<String> = chunk.to_vec();
            let batch_start = batch_idx * self.config.batch_size;

            tracing::debug!(
                model = %self.model,
                batch_index = batch_idx,
                batch_size = batch.len(),
                url = %url,
                input_type = "document",
                "sending embedding request"
            );

            let response_data = self.send_with_retry(&url, &batch, "document").await?;

            for (i, data) in response_data.into_iter().enumerate() {
                let global_idx = batch_start + i;
                if global_idx < all_embeddings.len() {
                    all_embeddings[global_idx] = Some(data.embedding);
                }
            }

            tracing::info!(
                model = %self.model,
                batch_index = batch_idx,
                batch_size = batch.len(),
                "batch embedding completed"
            );
        }

        all_embeddings
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| EmbedError::InvalidResponse("missing embeddings in response".into()))
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        if query.is_empty() {
            return Err(EmbedError::EmptyInput);
        }

        let truncated = Self::truncate_text(query);
        let url = format!("{}/v1/embeddings", self.base_url());
        let batch = vec![truncated];

        tracing::debug!(
            model = %self.model,
            url = %url,
            input_type = "query",
            "sending query embedding request"
        );

        let mut response_data = self.send_with_retry(&url, &batch, "query").await?;

        response_data
            .pop()
            .map(|d| d.embedding)
            .ok_or_else(|| EmbedError::InvalidResponse("empty response for query embedding".into()))
    }
}

impl VoyageEmbedder {
    async fn send_with_retry(
        &self,
        url: &str,
        batch: &[String],
        input_type: &str,
    ) -> Result<Vec<VoyageEmbeddingData>> {
        let request_body = VoyageEmbedRequest {
            model: &self.model,
            input: batch,
            input_type,
        };

        let mut last_error: Option<EmbedError> = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.config.base_delay * 2u32.pow(attempt - 1);
                tokio::time::sleep(delay).await;
            }

            let response = self
                .client
                .post(url)
                .bearer_auth(&*self.api_key)
                .json(&request_body)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        match resp.json::<VoyageEmbedResponse>().await {
                            Ok(parsed) => return Ok(parsed.data),
                            Err(e) => {
                                last_error = Some(EmbedError::InvalidResponse(format!(
                                    "failed to parse response: {e}"
                                )));
                                break;
                            }
                        }
                    }

                    if status.as_u16() == 429 {
                        let retry_after = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .map(Duration::from_secs);
                        return Err(EmbedError::RateLimited { retry_after });
                    }

                    if status.as_u16() == 401 || status.as_u16() == 403 {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(EmbedError::Auth(body));
                    }

                    let body_text = resp.text().await.unwrap_or_default();
                    let detail = serde_json::from_str::<VoyageErrorResponse>(&body_text)
                        .ok()
                        .and_then(|e| e.detail);
                    let error_msg = if let Some(d) = detail {
                        format!("HTTP {}: {}", status.as_u16(), d)
                    } else {
                        format!("HTTP {}: {}", status.as_u16(), body_text)
                    };
                    last_error = Some(EmbedError::Http(error_msg));
                }
                Err(e) => {
                    last_error = Some(EmbedError::Http(e.to_string()));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| EmbedError::Http("unknown error".into())))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::api_key::ApiKey;
    use crate::config::EmbedConfig;
    use crate::Embedder;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(base_url: String) -> EmbedConfig {
        EmbedConfig {
            base_url: Some(base_url),
            ..EmbedConfig::default()
        }
    }

    fn test_config_batch64(base_url: String) -> EmbedConfig {
        EmbedConfig {
            base_url: Some(base_url),
            batch_size: 64,
            ..EmbedConfig::default()
        }
    }

    fn test_api_key() -> ApiKey {
        ApiKey::from("vp-test-key")
    }

    fn make_voyage_response(embeddings: Vec<Vec<f32>>) -> serde_json::Value {
        let data: Vec<_> = embeddings
            .into_iter()
            .map(|embedding| {
                json!({
                    "object": "embedding",
                    "embedding": embedding,
                })
            })
            .collect();

        json!({
            "object": "list",
            "data": data,
            "model": "voyage-code-2",
        })
    }

    #[tokio::test]
    async fn happy_path_returns_correct_vectors() {
        let mock_server = MockServer::start().await;
        let expected = vec![vec![0.1_f32, 0.2, 0.3], vec![0.4, 0.5, 0.6]];

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(make_voyage_response(expected.clone())),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config(mock_server.uri());
        let embedder = VoyageEmbedder::with_api_key(config, test_api_key()).unwrap();

        let texts: Vec<String> = vec!["hello".into(), "world".into()];
        let result = embedder.embed(&texts).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![0.1_f32, 0.2, 0.3]);
        assert_eq!(result[1], vec![0.4, 0.5, 0.6]);
    }

    #[tokio::test]
    async fn auth_failure_401_returns_auth_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid api key"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config(mock_server.uri());
        let embedder = VoyageEmbedder::with_api_key(config, test_api_key()).unwrap();

        let texts: Vec<String> = vec!["hello".into()];
        let result = embedder.embed(&texts).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            EmbedError::Auth(_) => {}
            other => panic!("expected Auth error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn rate_limit_429_returns_rate_limited_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_string("rate limited")
                    .insert_header("retry-after", "42"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config(mock_server.uri());
        let embedder = VoyageEmbedder::with_api_key(config, test_api_key()).unwrap();

        let texts: Vec<String> = vec!["hello".into()];
        let result = embedder.embed(&texts).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            EmbedError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(42)));
            }
            other => panic!("expected RateLimited error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn batching_splits_200_texts_into_4_batches() {
        let mock_server = MockServer::start().await;

        let generate_response = |count: usize| -> serde_json::Value {
            let embeddings: Vec<Vec<f32>> = (0..count).map(|_| vec![0.1, 0.2, 0.3]).collect();
            make_voyage_response(embeddings)
        };

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
                let input_len = body["input"].as_array().map(|a| a.len()).unwrap_or(0);
                let resp = generate_response(input_len);
                ResponseTemplate::new(200).set_body_json(resp)
            })
            .expect(4)
            .mount(&mock_server)
            .await;

        let config = test_config_batch64(mock_server.uri());
        let embedder = VoyageEmbedder::with_api_key(config, test_api_key()).unwrap();

        let texts: Vec<String> = (0..200).map(|i| format!("text {i}")).collect();
        let result = embedder.embed(&texts).await.unwrap();

        assert_eq!(result.len(), 200);
        for embedding in &result {
            assert_eq!(embedding, &vec![0.1_f32, 0.2, 0.3]);
        }
    }

    #[tokio::test]
    async fn empty_input_returns_empty_input_error() {
        let mock_server = MockServer::start().await;
        let config = test_config(mock_server.uri());
        let embedder = VoyageEmbedder::with_api_key(config, test_api_key()).unwrap();

        let texts: Vec<String> = vec![];
        let result = embedder.embed(&texts).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            EmbedError::EmptyInput => {}
            other => panic!("expected EmptyInput error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn embed_query_uses_input_type_query() {
        let mock_server = MockServer::start().await;
        let expected = vec![0.1_f32, 0.2, 0.3];
        let response_value = make_voyage_response(vec![expected.clone()]);

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
                let input_type = body["input_type"].as_str().unwrap_or("");
                assert_eq!(
                    input_type, "query",
                    "embed_query must send input_type: query"
                );
                ResponseTemplate::new(200).set_body_json(response_value.clone())
            })
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = test_config(mock_server.uri());
        let embedder = VoyageEmbedder::with_api_key(config, test_api_key()).unwrap();

        let result = embedder.embed_query("hello").await.unwrap();
        assert_eq!(result, expected);
    }

    #[cfg(feature = "live-providers")]
    #[tokio::test]
    async fn voyage_live_smoke() {
        if std::env::var("VOYAGE_API_KEY").is_err() {
            return;
        }
        let config = EmbedConfig::default();
        let embedder = VoyageEmbedder::new(config).unwrap();

        assert_eq!(embedder.dimension(), 1536);
        assert_eq!(embedder.model_id(), "voyage-code-2");

        let texts: Vec<String> = vec!["hello world".into(), "goodbye world".into()];
        let embeddings = embedder.embed(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        for embedding in &embeddings {
            assert_eq!(embedding.len(), 1536);
            let sum: f32 = embedding.iter().sum();
            assert!(sum != 0.0, "embedding should not be all zeros");
        }
    }
}
