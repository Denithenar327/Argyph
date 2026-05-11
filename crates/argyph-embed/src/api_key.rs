use std::fmt;

use crate::error::{EmbedError, Result};

#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn from_env(key: &str) -> Result<Self> {
        std::env::var(key)
            .map(ApiKey)
            .map_err(|_| EmbedError::Config(format!("environment variable `{key}` not set")))
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ApiKey").field(&"***").finish()
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl std::ops::Deref for ApiKey {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
impl From<String> for ApiKey {
    fn from(s: String) -> Self {
        ApiKey(s)
    }
}

#[cfg(test)]
impl From<&str> for ApiKey {
    fn from(s: &str) -> Self {
        ApiKey(s.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn api_key_display_is_redacted() {
        let key = ApiKey("sk-secret-123".to_string());
        assert_eq!(format!("{key}"), "***");
    }

    #[test]
    fn api_key_debug_is_redacted() {
        let key = ApiKey("sk-secret-123".to_string());
        let debug = format!("{key:?}");
        assert!(debug.contains("***"));
        assert!(!debug.contains("sk-secret"));
    }

    #[test]
    fn api_key_deref_gives_raw_value() {
        let key = ApiKey("sk-secret-123".to_string());
        assert_eq!(&*key, "sk-secret-123");
    }

    #[test]
    fn api_key_from_env_missing() {
        // Ensure the env var is not set
        let result = ApiKey::from_env("ARGYPH_TEST_NONEXISTENT_KEY");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, EmbedError::Config(_)));
    }
}
