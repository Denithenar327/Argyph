#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

mod chunker;
mod error;
pub mod languages;
mod parser;
pub mod structural;
pub mod types;

pub use chunker::{ast_chunks, char_split, fallback_chunks};
pub use error::{ParseError, Result};
pub use parser::{DefaultParser, Parser};
pub use types::{
    ByteRange, Chunk, ChunkId, ChunkKind, Import, ParsedFile, Symbol, SymbolId, SymbolKind,
};
