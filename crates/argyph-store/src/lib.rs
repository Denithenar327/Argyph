#![forbid(unsafe_code)]

mod error;
mod migration;
pub mod schema;
mod sqlite;

use argyph_fs::FileEntry;
use argyph_graph::edge::{Edge, EdgeKind};
use argyph_graph::graph::SymbolOutline;
use argyph_graph::selector::SymbolSelector;
use argyph_parse::types::{Chunk, Symbol};
use camino::Utf8Path;

pub use error::{Result, StoreError};
pub use sqlite::SqliteStore;

/// Persists file metadata, symbols, chunks, edges, and embedding vectors.
/// Provides hybrid BM25 + vector search and schema migration management.
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    /// Insert or update file entries. Path is the primary key — existing rows
    /// are updated with the new hash, language, and size.
    async fn upsert_files(&self, files: &[FileEntry]) -> Result<()>;

    /// Look up a single file by its repository-relative path.
    async fn get_file(&self, path: &Utf8Path) -> Result<Option<FileEntry>>;

    /// Return every file entry, ordered by path.
    async fn list_files(&self) -> Result<Vec<FileEntry>>;

    /// Remove a file from the index.
    async fn delete_file(&self, path: &Utf8Path) -> Result<()>;

    /// Insert or replace symbol rows. Idempotent — re-running with the same
    /// symbol IDs updates the rows.
    async fn upsert_symbols(&self, symbols: &[Symbol]) -> Result<()>;

    /// Insert or replace chunk rows. FTS5 index is kept in sync via triggers.
    async fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>;

    /// Replace all edges for the files represented by the given edge set, then
    /// insert the new edges. Atomic within a transaction.
    async fn upsert_edges(&self, edges: &[Edge]) -> Result<()>;

    /// Find symbols by name, optionally scoped to a specific file.
    async fn find_symbol(&self, name: &str, file: Option<&Utf8Path>) -> Result<Vec<Symbol>>;

    /// Find reference edges pointing to the given symbol selector.
    async fn find_references(&self, sel: &SymbolSelector) -> Result<Vec<Edge>>;

    /// Find edges where `from_id` matches the selector and kind matches.
    /// Outgoing edges from the matched symbols.
    async fn neighbors(&self, sel: &SymbolSelector, kind: EdgeKind) -> Result<Vec<Edge>>;

    /// Find callers of the selected symbol(s) — edges where `to_id` matches
    /// and kind is Calls.
    async fn get_callers(&self, sel: &SymbolSelector) -> Result<Vec<Edge>>;

    /// Find callees of the selected symbol(s) — edges where `from_id` matches
    /// and kind is Calls.
    async fn get_callees(&self, sel: &SymbolSelector) -> Result<Vec<Edge>>;

    /// Find import edges originating from symbols in the given file.
    async fn get_imports(&self, file: &Utf8Path) -> Result<Vec<Edge>>;

    /// Return a hierarchical outline of all symbols defined in a file.
    async fn get_symbol_outline(&self, file: &Utf8Path) -> Result<Vec<SymbolOutline>>;

    /// Flush and close the store. The store may not be used after calling this.
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use argyph_fs::{Blake3Hash, Language};
    use argyph_graph::edge::{Confidence, EdgeKind};
    use argyph_graph::selector::SymbolSelector;
    use argyph_parse::types::{ByteRange, Chunk, ChunkId, ChunkKind, Symbol, SymbolId, SymbolKind};
    use camino::Utf8PathBuf;
    use rusqlite::params;
    use std::time::SystemTime;

    fn open_temp() -> SqliteStore {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        SqliteStore::open_at(&root).unwrap()
    }

    fn open_mem() -> SqliteStore {
        SqliteStore::open_in_memory().unwrap()
    }

    fn make_entry(path: &str, content: &[u8]) -> FileEntry {
        let hash = Blake3Hash::from(*blake3::hash(content).as_bytes());
        let ext = path.rsplit('.').next().unwrap_or("");
        let lang = argyph_fs::Language::from_extension(ext);
        FileEntry {
            path: Utf8PathBuf::from(path),
            hash,
            language: lang,
            size: content.len() as u64,
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    fn make_symbol(file: &str, name: &str, kind: SymbolKind, start: usize, end: usize) -> Symbol {
        let path = Utf8PathBuf::from(file);
        Symbol {
            id: SymbolId::new(&path, name, start),
            name: name.to_string(),
            kind,
            file: path,
            range: ByteRange::new(start, end),
            signature: None,
            parent: None,
        }
    }

    fn make_chunk(file: &str, text: &str, kind: ChunkKind, start: usize, end: usize) -> Chunk {
        Chunk {
            id: ChunkId::from_text(text),
            file: Utf8PathBuf::from(file),
            range: ByteRange::new(start, end),
            text: text.to_string(),
            kind,
            language: Language::Rust,
        }
    }

    fn make_edge(
        from_file: &str,
        from_name: &str,
        from_pos: usize,
        to_file: &str,
        to_name: &str,
        to_pos: usize,
        kind: EdgeKind,
        confidence: Confidence,
    ) -> Edge {
        Edge {
            from: SymbolId::new(&Utf8PathBuf::from(from_file), from_name, from_pos),
            to: SymbolId::new(&Utf8PathBuf::from(to_file), to_name, to_pos),
            kind,
            confidence,
        }
    }

    // ---- Existing file-entry tests ----

    #[tokio::test]
    async fn upsert_and_list() {
        let store = open_temp();
        let entries = vec![
            make_entry("src/main.rs", b"fn main() {}"),
            make_entry("src/lib.rs", b"pub fn add(a: i32, b: i32) -> i32 { a + b }"),
        ];
        store.upsert_files(&entries).await.unwrap();

        let mut list = store.list_files().await.unwrap();
        list.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].path.as_str(), "src/lib.rs");
        assert_eq!(list[1].path.as_str(), "src/main.rs");
        for entry in &list {
            let expected = entries.iter().find(|e| e.path == entry.path).unwrap();
            assert_eq!(entry.hash, expected.hash);
            assert_eq!(entry.language, expected.language);
            assert_eq!(entry.size, expected.size);
        }
    }

    #[tokio::test]
    async fn get_file_found_and_not_found() {
        let store = open_temp();
        let entry = make_entry("README.md", b"# Hello");
        store.upsert_files(&[entry.clone()]).await.unwrap();

        let found = store
            .get_file(&Utf8PathBuf::from("README.md"))
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().hash, entry.hash);

        let missing = store
            .get_file(&Utf8PathBuf::from("nope.txt"))
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let store = open_temp();
        let a = make_entry("a.rs", b"a");
        let b = make_entry("b.rs", b"b");
        store.upsert_files(&[a, b]).await.unwrap();

        store.delete_file(&Utf8PathBuf::from("a.rs")).await.unwrap();
        let list = store.list_files().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path.as_str(), "b.rs");
    }

    #[tokio::test]
    async fn upsert_is_idempotent() {
        let store = open_temp();
        let e1 = make_entry("x.rs", b"v1");
        let e2 = FileEntry {
            hash: Blake3Hash::from(*blake3::hash(b"v2").as_bytes()),
            size: 2,
            ..e1.clone()
        };

        store.upsert_files(&[e1.clone()]).await.unwrap();
        store.upsert_files(&[e2.clone()]).await.unwrap();

        let found = store
            .get_file(&Utf8PathBuf::from("x.rs"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.hash, e2.hash);
        assert_eq!(found.size, 2);
    }

    #[tokio::test]
    async fn round_trip_many_entries() {
        let store = open_temp();
        let count = 300;
        let entries: Vec<_> = (0..count)
            .map(|i| make_entry(&format!("src/mod{i}.rs"), format!("// file {i}").as_bytes()))
            .collect();

        store.upsert_files(&entries).await.unwrap();
        let list = store.list_files().await.unwrap();

        assert_eq!(list.len(), count);
        let paths: std::collections::HashSet<_> =
            list.iter().map(|e| e.path.as_str().to_string()).collect();
        for entry in &entries {
            assert!(paths.contains(entry.path.as_str()));
        }
    }

    #[tokio::test]
    async fn empty_upsert_does_not_crash() {
        let store = open_temp();
        store.upsert_files(&[]).await.unwrap();
        assert!(store.list_files().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn language_is_round_tripped() {
        let store = open_temp();
        let entries = vec![
            make_entry("lib.rs", b"rust"),
            make_entry("app.ts", b"ts"),
            make_entry("util.py", b"py"),
            make_entry("readme.md", b"md"),
        ];
        store.upsert_files(&entries).await.unwrap();
        let list = store.list_files().await.unwrap();
        for entry in &list {
            let expected = entries.iter().find(|e| e.path == entry.path).unwrap();
            assert_eq!(
                entry.language, expected.language,
                "language mismatch for {}",
                entry.path
            );
        }
    }

    #[tokio::test]
    async fn db_persists_across_opens() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let entry = make_entry("persist.rs", b"data");

        {
            let store = SqliteStore::open_at(&root).unwrap();
            store.upsert_files(&[entry.clone()]).await.unwrap();
        }
        {
            let store = SqliteStore::open_at(&root).unwrap();
            let found = store
                .get_file(&Utf8PathBuf::from("persist.rs"))
                .await
                .unwrap();
            assert!(found.is_some());
            assert_eq!(found.unwrap().hash, entry.hash);
        }
    }

    // ---- New symbol / chunk / edge tests ----

    #[tokio::test]
    async fn upsert_symbols_and_find_by_name() {
        let store = open_mem();
        let sym = make_symbol("src/lib.rs", "add", SymbolKind::Function, 10, 50);
        store.upsert_symbols(&[sym.clone()]).await.unwrap();

        let found = store.find_symbol("add", None).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "add");
        assert_eq!(found[0].file.as_str(), "src/lib.rs");
    }

    #[tokio::test]
    async fn find_symbol_scoped_to_file() {
        let store = open_mem();
        let a = make_symbol("src/a.rs", "helper", SymbolKind::Function, 0, 20);
        let b = make_symbol("src/b.rs", "helper", SymbolKind::Function, 0, 20);
        store.upsert_symbols(&[a, b]).await.unwrap();

        let found = store
            .find_symbol("helper", Some(&Utf8PathBuf::from("src/b.rs")))
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file.as_str(), "src/b.rs");
    }

    #[tokio::test]
    async fn find_symbol_missing() {
        let store = open_mem();
        let found = store.find_symbol("nope", None).await.unwrap();
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn upsert_symbols_is_idempotent() {
        let store = open_mem();
        let sym = make_symbol("x.rs", "f", SymbolKind::Function, 5, 25);
        store.upsert_symbols(&[sym]).await.unwrap();
        store
            .upsert_symbols(&[make_symbol("x.rs", "f", SymbolKind::Function, 5, 25)])
            .await
            .unwrap();
        let found = store.find_symbol("f", None).await.unwrap();
        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn upsert_chunks_round_trips_via_fts5() {
        let store = open_mem();
        let chunk = make_chunk(
            "src/lib.rs",
            "fn greet() { hello(); }",
            ChunkKind::FunctionBody,
            0,
            25,
        );
        store.upsert_chunks(&[chunk]).await.unwrap();

        // Verify via FTS5: BM25 search should find it.
        let conn = store.conn.lock().expect("poisoned");
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                params!["greet"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count > 0, "FTS5 should match 'greet'");
    }

    #[tokio::test]
    async fn upsert_chunks_fts5_does_not_match_absent_term() {
        let store = open_mem();
        let chunk = make_chunk("src/lib.rs", "fn foo() {}", ChunkKind::FunctionBody, 0, 12);
        store.upsert_chunks(&[chunk]).await.unwrap();

        let conn = store.conn.lock().expect("poisoned");
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                params!["zzzz_not_present"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn empty_upsert_edges_does_nothing() {
        let store = open_mem();
        store.upsert_edges(&[]).await.unwrap();
    }

    #[tokio::test]
    async fn upsert_edges_and_find_references() {
        let store = open_mem();
        let edge = make_edge(
            "src/main.rs",
            "main",
            0,
            "src/lib.rs",
            "add",
            10,
            EdgeKind::References,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[edge.clone()]).await.unwrap();

        let refs = store
            .find_references(&SymbolSelector::ByName {
                file: Utf8PathBuf::from("src/lib.rs"),
                name: "add".into(),
            })
            .await
            .unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, EdgeKind::References);
        assert_eq!(refs[0].from.as_str(), "src/main.rs::main::0");
        assert_eq!(refs[0].to.as_str(), "src/lib.rs::add::10");
    }

    #[tokio::test]
    async fn get_callers_and_callees() {
        let store = open_mem();
        let caller = make_symbol("src/a.rs", "caller_fn", SymbolKind::Function, 0, 40);
        let callee = make_symbol("src/a.rs", "callee_fn", SymbolKind::Function, 50, 90);
        store
            .upsert_symbols(&[caller.clone(), callee.clone()])
            .await
            .unwrap();

        let edge = make_edge(
            "src/a.rs",
            "caller_fn",
            0,
            "src/a.rs",
            "callee_fn",
            50,
            EdgeKind::Calls,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[edge]).await.unwrap();

        let callers = store
            .get_callers(&SymbolSelector::ById(callee.id.clone()))
            .await
            .unwrap();
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].from.as_str(), "src/a.rs::caller_fn::0");

        let callees = store
            .get_callees(&SymbolSelector::ById(caller.id.clone()))
            .await
            .unwrap();
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].to.as_str(), "src/a.rs::callee_fn::50");
    }

    #[tokio::test]
    async fn get_imports_finds_file_imports() {
        let store = open_mem();
        let edge = make_edge(
            "src/main.rs",
            "main",
            0,
            "src/math.rs",
            "add",
            10,
            EdgeKind::Imports,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[edge.clone()]).await.unwrap();

        let imports = store
            .get_imports(&Utf8PathBuf::from("src/main.rs"))
            .await
            .unwrap();
        assert!(!imports.is_empty());
        assert_eq!(imports[0].kind, EdgeKind::Imports);
        assert!(imports[0].from.as_str().starts_with("src/main.rs::"));
    }

    #[tokio::test]
    async fn get_imports_empty_for_unrelated_file() {
        let store = open_mem();
        let edge = make_edge(
            "src/main.rs",
            "main",
            0,
            "src/math.rs",
            "add",
            10,
            EdgeKind::Imports,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[edge]).await.unwrap();

        let imports = store
            .get_imports(&Utf8PathBuf::from("src/other.rs"))
            .await
            .unwrap();
        assert!(imports.is_empty());
    }

    #[tokio::test]
    async fn get_symbol_outline_returns_symbols_in_file() {
        let store = open_mem();
        let a = make_symbol("src/lib.rs", "new", SymbolKind::Function, 0, 30);
        let b = make_symbol("src/lib.rs", "add", SymbolKind::Function, 40, 70);
        store.upsert_symbols(&[a, b]).await.unwrap();

        let outline = store
            .get_symbol_outline(&Utf8PathBuf::from("src/lib.rs"))
            .await
            .unwrap();
        assert_eq!(outline.len(), 2);
        let names: Vec<&str> = outline.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"add"));
        assert!(names.contains(&"new"));
    }

    #[tokio::test]
    async fn get_symbol_outline_handles_parent_child() {
        let store = open_mem();
        let parent = Symbol {
            id: SymbolId::new(&Utf8PathBuf::from("src/struct.rs"), "MyStruct", 10),
            name: "MyStruct".into(),
            kind: SymbolKind::Struct,
            file: Utf8PathBuf::from("src/struct.rs"),
            range: ByteRange::new(10, 200),
            signature: None,
            parent: None,
        };
        let child_id = SymbolId::new(&Utf8PathBuf::from("src/struct.rs"), "method_a", 50);
        let child = Symbol {
            id: child_id.clone(),
            name: "method_a".into(),
            kind: SymbolKind::Method,
            file: Utf8PathBuf::from("src/struct.rs"),
            range: ByteRange::new(50, 100),
            signature: None,
            parent: Some(SymbolId::new(
                &Utf8PathBuf::from("src/struct.rs"),
                "MyStruct",
                10,
            )),
        };
        store.upsert_symbols(&[parent, child]).await.unwrap();

        let outline = store
            .get_symbol_outline(&Utf8PathBuf::from("src/struct.rs"))
            .await
            .unwrap();
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0].name, "MyStruct");
        assert_eq!(outline[0].children.len(), 1);
        assert_eq!(outline[0].children[0].name, "method_a");
    }

    #[tokio::test]
    async fn neighbors_returns_outgoing_edges_of_kind() {
        let store = open_mem();
        let edge = make_edge(
            "src/a.rs",
            "a_fn",
            0,
            "src/b.rs",
            "b_fn",
            100,
            EdgeKind::References,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[edge]).await.unwrap();

        let outgoing = store
            .neighbors(
                &SymbolSelector::ById(SymbolId::new(&Utf8PathBuf::from("src/a.rs"), "a_fn", 0)),
                EdgeKind::References,
            )
            .await
            .unwrap();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].to.as_str(), "src/b.rs::b_fn::100");
    }

    #[tokio::test]
    async fn find_references_by_qualified_name() {
        let store = open_mem();
        let edge = make_edge(
            "src/main.rs",
            "main",
            0,
            "src/math.rs",
            "multiply",
            42,
            EdgeKind::References,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[edge]).await.unwrap();

        let refs = store
            .find_references(&SymbolSelector::Qualified("multiply".into()))
            .await
            .unwrap();
        assert_eq!(refs.len(), 1);
    }

    #[tokio::test]
    async fn edge_replace_deletes_old_file_edges() {
        let store = open_mem();
        let e1 = make_edge(
            "src/a.rs",
            "a_fn",
            0,
            "src/b.rs",
            "b_fn",
            100,
            EdgeKind::References,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[e1]).await.unwrap();
        assert_eq!(
            store
                .find_references(&SymbolSelector::ByName {
                    file: Utf8PathBuf::from("src/b.rs"),
                    name: "b_fn".into(),
                })
                .await
                .unwrap()
                .len(),
            1
        );

        // Upsert new edges with same file prefix — old edges deleted.
        let e2 = make_edge(
            "src/a.rs",
            "a_fn",
            0,
            "src/c.rs",
            "c_fn",
            200,
            EdgeKind::References,
            Confidence::Heuristic,
        );
        store.upsert_edges(&[e2]).await.unwrap();

        // Old reference to b_fn should be gone since src/a.rs was replaced.
        let refs_b = store
            .find_references(&SymbolSelector::ByName {
                file: Utf8PathBuf::from("src/b.rs"),
                name: "b_fn".into(),
            })
            .await
            .unwrap();
        assert!(
            refs_b.is_empty(),
            "old edge from src/a.rs should be deleted"
        );

        // New reference to c_fn should exist.
        let refs_c = store
            .find_references(&SymbolSelector::ByName {
                file: Utf8PathBuf::from("src/c.rs"),
                name: "c_fn".into(),
            })
            .await
            .unwrap();
        assert_eq!(refs_c.len(), 1);
    }

    #[tokio::test]
    async fn empty_upserts_noop() {
        let store = open_mem();
        store.upsert_symbols(&[]).await.unwrap();
        store.upsert_chunks(&[]).await.unwrap();
        store.upsert_edges(&[]).await.unwrap();
    }
}
