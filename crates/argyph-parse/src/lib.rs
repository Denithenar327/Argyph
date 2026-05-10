#![forbid(unsafe_code)]

// TODO: See crates/argyph-parse/MODULE.md — owns tree-sitter parsing, language-pack
// registry, per-language .scm queries, AST-aware chunking, and symbol extraction.

/// Parses a source file with tree-sitter, extracting symbols, AST-aware chunks,
/// and raw import statements. Language packs register their tree-sitter grammar
/// and `.scm` queries at startup.
pub trait Parser {}
