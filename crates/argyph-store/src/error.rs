use thiserror::Error;

/// Crate-level error type for all store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("migration error: {0}")]
    Migration(String),
}

/// Crate-level result alias.
pub type Result<T> = std::result::Result<T, StoreError>;
