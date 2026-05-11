use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("tree-sitter query error: {0}")]
    Query(#[from] tree_sitter::QueryError),

    #[error("tree-sitter language error: {0}")]
    Language(#[from] tree_sitter::LanguageError),

    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ParseError>;
