use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("store error: {0}")]
    Store(#[from] argyph_store::StoreError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(#[from] argyph_parse::ParseError),

    #[error("graph error: {0}")]
    Graph(#[from] argyph_graph::GraphError),
}

pub type Result<T> = std::result::Result<T, CoreError>;
