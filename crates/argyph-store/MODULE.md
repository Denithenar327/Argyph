# `argyph-store` — persistence

## Purpose

The persistence layer. Owns SQLite (for files, symbols, edges, FTS5 text index, and chunk vectors). Provides the `Store` trait that the rest of the project consumes.

## Owns

- SQLite schema and migrations for the `files`, `symbols`, `chunks`, `edges`, `vectors` tables and the FTS5 virtual table over chunk text.
- The `Store` trait and its default implementation.
- Hybrid search query: BM25 from SQLite FTS5 fused with vector similarity via reciprocal rank fusion (RRF).
- Schema migration runner — runs at boot, before any read/write.
- Integrity check on startup; on detected corruption, drop and rebuild the index with a clear user warning.
- WAL mode configuration on SQLite.
- The on-disk layout under `.argyph/`:
  ```
  .argyph/
    meta.sqlite        # SQLite (files, symbols, chunks-text, edges, FTS5, vectors)
    meta.sqlite-wal
    schema_version
  ```

## Must never own

- Embedding generation (lives in `argyph-embed`).
- Parsing or chunking (lives in `argyph-parse`).
- MCP or CLI surfaces.
- Any business logic about *when* to read or write.

## Public surface

```rust
#[async_trait]
pub trait Store: Send + Sync {
    async fn upsert_files(&self, files: &[FileEntry]) -> Result<()>;
    async fn upsert_symbols(&self, symbols: &[Symbol]) -> Result<()>;
    async fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>;
    async fn upsert_vectors(&self, vectors: &[(ChunkId, Vec<f32>)]) -> Result<()>;
    async fn upsert_edges(&self, edges: &[Edge]) -> Result<()>;

    async fn delete_file(&self, path: &Utf8Path) -> Result<()>;
    async fn list_files(&self) -> Result<Vec<FileEntry>>;
    async fn file_meta(&self, path: &Utf8Path) -> Result<Option<FileEntry>>;

    async fn find_symbol(&self, name: &str, scope: Option<&str>) -> Result<Vec<Symbol>>;
    async fn find_references(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    async fn neighbors(&self, sel: SymbolSelector, kind: EdgeKind) -> Result<Vec<Edge>>;

    async fn search_text(&self, q: &TextQuery) -> Result<Vec<TextHit>>;
    async fn search_hybrid(
        &self,
        query: &str,
        query_vec: &[f32],
        k: usize,
        filter: Filter,
    ) -> Result<Vec<SearchHit>>;

    async fn missing_vectors(&self, model: &str) -> Result<Vec<ChunkId>>;
}

pub struct DefaultStore { /* private */ }

impl DefaultStore {
    pub async fn open(root: &Path) -> Result<Self>;
    pub async fn close(self) -> Result<()>;
}
```

## Internal structure

- `src/lib.rs` — `Store` trait, `DefaultStore` factory.
- `src/sqlite/mod.rs` — SQLite connection pool and `Store` trait implementation.
- `src/meta.rs` — SQLite connection pool, query helpers.
- `src/schema.rs` — **architecture-protected.** Schema definitions, never edited; new versions add migrations.
- `src/migrations/` — **architecture-protected.** Numbered SQL files: `001_initial.sql`, `002_add_symbols.sql`, etc.
- `src/hybrid.rs` — reciprocal rank fusion of BM25 and vector results.
- `src/error.rs` — typed errors.

## Failure modes

- **AI agents editing existing migrations.** Hard rule: never. Add a new migration file; never edit an old one. AI Agent Rule #2.
- **AI agents bypassing the `Store` trait.** Other crates must never `use rusqlite` directly. The trait is the seam.
- **AI agents writing schema changes without bumping `schema_version`.** Tested in CI: an integration test that fails if the migration set's resulting schema diverges from `schema.rs`.
- **Power-loss scenarios.** WAL mode + transactional writes mostly handle this. The integrity check on startup is the backstop.
- **AI agents disabling WAL mode for "simplicity."** Don't.

## Honest limitations

- SQLite is excellent up to ~10M rows for our query patterns. Past that, query plan tuning matters; we'll address it if/when we hit it.
- Vector search uses brute-force cosine similarity in Rust over SQLite BLOBs, keeping the build portable with zero native dependencies beyond SQLite.
- We do not currently support concurrent readers from a different process. The `argyph` binary is the single owner.
- The embedded model fingerprint is checked on startup; if the configured model doesn't match the index's embeddings, the user is told to reindex Tier 2.

## Stability

- `Store` trait is *the* most central inter-crate contract. Adding methods is fine; changing existing method signatures is a coordinated change across multiple crates.
- Schema migrations are append-only.
- The on-disk layout (`.argyph/` contents) is part of the user-visible contract; changes require a migration plan.
