use crate::error::Result;
use camino::Utf8Path;

/// Layered configuration (env > repo file > defaults).
///
/// Currently a minimal stub; full figment-backed layering lands with M2.
#[derive(Debug, Clone)]
pub struct Config;

impl Config {
    /// Load configuration from the repo root. Returns defaults for now.
    pub fn load(_root: &Utf8Path) -> Result<Self> {
        Ok(Self)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self
    }
}
