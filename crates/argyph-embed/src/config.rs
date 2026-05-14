use std::path::PathBuf;
use std::time::Duration;

use crate::Provider;

#[derive(Clone)]
pub struct EmbedConfig {
    pub batch_size: usize,
    pub max_retries: u32,
    pub base_delay: Duration,
    pub concurrency: usize,
    pub timeout: Duration,
    pub base_url: Option<String>,
    pub cache_dir: Option<PathBuf>,
}

impl EmbedConfig {
    pub fn for_provider(provider: &Provider) -> Self {
        Self {
            concurrency: provider.default_concurrency(),
            ..Self::default()
        }
    }
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            concurrency: 8,
            timeout: Duration::from_secs(30),
            base_url: None,
            cache_dir: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = EmbedConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay, Duration::from_secs(1));
        assert_eq!(config.concurrency, 8);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.base_url.is_none());
        assert!(config.cache_dir.is_none());
    }
}
