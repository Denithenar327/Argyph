use camino::Utf8Path;
use serde::Deserialize;

use crate::error::Result;

/// Layered configuration (env > repo file > defaults).
///
/// Priority: `ARGYPH_*` env vars > `.argyph/config.toml` > built-in defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub index: IndexConfig,

    #[serde(default)]
    pub search: SearchConfig,

    #[serde(default)]
    pub pack: PackConfig,

    #[serde(default)]
    pub locate: LocateConfig,

    #[serde(default)]
    pub locate_smart: LocateSmartConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct IndexConfig {
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,

    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchConfig {
    #[serde(default = "default_hybrid_alpha")]
    pub hybrid_alpha: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PackConfig {
    #[serde(default = "default_token_budget")]
    pub default_token_budget: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocateConfig {
    #[serde(default = "default_locate_max_file_bytes")]
    pub max_file_bytes: u64,
}

impl Default for LocateConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: default_locate_max_file_bytes(),
        }
    }
}

fn default_locate_max_file_bytes() -> u64 {
    10_485_760
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LocateSmartConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
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

fn default_exclude() -> Vec<String> {
    vec!["docs/generated/**".into(), "**/*.min.js".into()]
}

fn default_languages() -> Vec<String> {
    vec!["rust".into(), "typescript".into(), "python".into()]
}

fn default_hybrid_alpha() -> f64 {
    0.5
}

fn default_token_budget() -> u64 {
    50000
}

impl Default for Config {
    fn default() -> Self {
        Self {
            index: IndexConfig {
                exclude: default_exclude(),
                languages: default_languages(),
            },
            search: SearchConfig {
                hybrid_alpha: default_hybrid_alpha(),
            },
            pack: PackConfig {
                default_token_budget: default_token_budget(),
            },
            locate: LocateConfig::default(),
            locate_smart: LocateSmartConfig::default(),
        }
    }
}

impl Config {
    pub fn load(root: &Utf8Path) -> Result<Self> {
        let config_path = root.join(".argyph").join("config.toml");
        let mut config = if config_path.exists() {
            match std::fs::read_to_string(config_path.as_str()) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => config,
                    Err(e) => {
                        tracing::warn!("invalid .argyph/config.toml: {e}, using defaults");
                        Self::default()
                    }
                },
                Err(e) => {
                    tracing::warn!("cannot read .argyph/config.toml: {e}, using defaults");
                    Self::default()
                }
            }
        } else {
            Self::default()
        };
        config.apply_env_overrides();
        Ok(config)
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("ARGYPH_LOCATE_SMART_ENABLED") {
            self.locate_smart.enabled = v == "true" || v == "1";
        }
        if let Ok(v) = std::env::var("ARGYPH_LOCATE_SMART_PROVIDER") {
            self.locate_smart.provider = Some(v);
        }
        if let Ok(v) = std::env::var("ARGYPH_LOCATE_SMART_MODEL") {
            self.locate_smart.model = Some(v);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let config = Config::default();
        assert_eq!(config.index.languages.len(), 3);
        assert_eq!(config.search.hybrid_alpha, 0.5);
        assert_eq!(config.pack.default_token_budget, 50000);
    }

    #[test]
    fn load_non_existent_config_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let config = Config::load(&root).unwrap();
        assert_eq!(config.index.languages.len(), 3);
    }
}
