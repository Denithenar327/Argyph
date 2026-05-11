use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Option<Duration> },

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("batch too large: {batch_size}, max: {max_batch_size}")]
    BatchTooLarge {
        batch_size: usize,
        max_batch_size: usize,
    },

    #[error("empty input")]
    EmptyInput,

    #[error("configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, EmbedError>;
