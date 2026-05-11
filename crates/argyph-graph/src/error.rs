use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("no symbols found")]
    NoSymbols,
}
