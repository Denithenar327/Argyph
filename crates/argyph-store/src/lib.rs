#![forbid(unsafe_code)]

// TODO: See crates/argyph-store/MODULE.md — owns SQLite (files, symbols, edges,
// FTS5 text index) and LanceDB (chunk vectors), schema migrations, hybrid search
// via reciprocal rank fusion, and the `.argyph/` on-disk layout.

/// Persists file metadata, symbols, chunks, edges, and embedding vectors.
/// Provides hybrid BM25 + vector search and schema migration management.
pub trait Store {}
