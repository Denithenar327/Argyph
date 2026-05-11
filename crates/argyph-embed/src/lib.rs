pub mod api_key;
pub mod config;
pub mod error;
pub mod http;
pub mod local;
pub mod model_files;
pub mod model_hashes;
pub mod openai;
pub mod tokenize;
pub mod voyage;

use std::sync::Arc;

pub use error::Result;

#[async_trait::async_trait]
pub trait Embedder: Send + Sync + 'static {
    fn dimension(&self) -> usize;

    fn model_id(&self) -> &str;

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        Ok(self.embed(&[query.to_string()]).await?.remove(0))
    }
}

pub enum Provider {
    Local,
    OpenAi,
    Voyage,
}

pub fn build(provider: Provider, config: config::EmbedConfig) -> Result<Arc<dyn Embedder>> {
    match provider {
        Provider::Local => {
            let embedder = tokio::runtime::Handle::try_current()
                .map(|handle| handle.block_on(local::LocalEmbedder::new(config.clone())))
                .unwrap_or_else(|_| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| {
                            error::EmbedError::Config(format!("failed to create runtime: {e}"))
                        })?
                        .block_on(local::LocalEmbedder::new(config))
                })?;
            Ok(Arc::new(embedder))
        }
        Provider::OpenAi => Ok(Arc::new(openai::OpenAiEmbedder::new(config)?)),
        Provider::Voyage => Ok(Arc::new(voyage::VoyageEmbedder::new(config)?)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::EmbedConfig;

    #[test]
    fn build_local_not_implemented_or_fails_with_config_err() {
        let result = build(Provider::Local, EmbedConfig::default());
        match result {
            Err(error::EmbedError::Config(_)) => {}
            Ok(_) => {
                eprintln!("local provider succeeded (model was cached)");
            }
            Err(other) => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn build_voyage_fails_without_api_key() {
        if std::env::var("VOYAGE_API_KEY").is_ok() {
            return;
        }
        let result = build(Provider::Voyage, EmbedConfig::default());
        assert!(result.is_err());
        match result.err().unwrap() {
            error::EmbedError::Config(msg) => {
                assert!(msg.contains("VOYAGE_API_KEY"));
            }
            other => panic!("expected Config error about API key, got: {other:?}"),
        }
    }

    #[test]
    fn build_openai_fails_without_api_key() {
        if std::env::var("OPENAI_API_KEY").is_ok() {
            return;
        }
        let result = build(Provider::OpenAi, EmbedConfig::default());
        assert!(result.is_err());
        match result.err().unwrap() {
            error::EmbedError::Config(msg) => {
                assert!(msg.contains("OPENAI_API_KEY"));
            }
            other => panic!("expected Config error about API key, got: {other:?}"),
        }
    }
}
