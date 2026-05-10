#![forbid(unsafe_code)]

mod error;
mod migration;
pub mod schema;
mod sqlite;

use argyph_fs::FileEntry;
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use argyph_fs::Blake3Hash;
    use camino::Utf8PathBuf;
    use std::time::SystemTime;

    fn open_temp() -> SqliteStore {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        SqliteStore::open_at(&root).unwrap()
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
        // Verify every entry is present by path
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
}
