-- Symbols: one row per extracted code symbol.
-- id is the SymbolId string (e.g. "src/main.rs::add::42").
CREATE TABLE IF NOT EXISTS symbols (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    file TEXT NOT NULL,
    range_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    signature TEXT,
    parent_id TEXT REFERENCES symbols(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
CREATE INDEX IF NOT EXISTS idx_symbols_file_name ON symbols(file, name);

-- Chunks: AST-aware text chunks for embedding (Phase 3) + BM25 search.
-- Implicit rowid is used by the FTS5 content= triggers below.
CREATE TABLE IF NOT EXISTS chunks (
    id TEXT PRIMARY KEY,
    file TEXT NOT NULL,
    range_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    text TEXT NOT NULL,
    kind TEXT NOT NULL,
    language TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file);

-- Edges: symbol graph edges built by argyph-graph.
-- from_id / to_id are SymbolId strings.
-- Dedup: one edge per (from, to, kind) combination.
CREATE TABLE IF NOT EXISTS edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    confidence TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE INDEX IF NOT EXISTS idx_edges_kind ON edges(kind);
CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_dedup ON edges(from_id, to_id, kind);

-- FTS5 virtual table over chunks.text for BM25 (Phase 3 hybrid search).
-- Uses external content table with triggers so text is stored once in chunks.
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS chunks_fts_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER IF NOT EXISTS chunks_fts_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
END;

CREATE TRIGGER IF NOT EXISTS chunks_fts_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.rowid, new.text);
END;
