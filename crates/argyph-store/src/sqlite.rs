use camino::{Utf8Path, Utf8PathBuf};
use rusqlite::{params, Connection};
use std::sync::Mutex;
use std::time::SystemTime;

use argyph_fs::{Blake3Hash, FileEntry, Language};

use crate::error::Result;
use crate::migration;
use crate::Store;

/// A SQLite-backed implementation of [`Store`].
///
/// Creates `.argyph/meta.sqlite` under the given root on first open and runs
/// pending schema migrations. Enables WAL mode automatically.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Open (or create) the store under `root/.argyph/meta.sqlite`.
    pub fn open_at(root: &Utf8Path) -> Result<Self> {
        let dir = root.join(".argyph");
        std::fs::create_dir_all(dir.as_std_path())?;

        let db_path = dir.join("meta.sqlite");
        let conn = Connection::open(db_path.as_std_path())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        migration::run(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait::async_trait]
impl Store for SqliteStore {
    #[allow(clippy::expect_used)]
    async fn upsert_files(&self, files: &[FileEntry]) -> Result<()> {
        let mut conn = self.conn.lock().expect("mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO files (path, hash, language, size, last_seen)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(path) DO UPDATE SET
                     hash     = excluded.hash,
                     language = excluded.language,
                     size     = excluded.size,
                     last_seen = datetime('now')",
            )?;

            for entry in files {
                let lang_val = entry.language.map(language_to_str);
                stmt.execute(params![
                    entry.path.as_str(),
                    entry.hash.as_bytes().as_slice(),
                    lang_val,
                    entry.size as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn get_file(&self, path: &Utf8Path) -> Result<Option<FileEntry>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        let mut stmt =
            conn.prepare_cached("SELECT path, hash, language, size FROM files WHERE path = ?1")?;
        let mut rows = stmt.query_map(params![path.as_str()], |row| {
            let path_str: String = row.get(0)?;
            let hash_blob: Vec<u8> = row.get(1)?;
            let language: Option<String> = row.get(2)?;
            let size: i64 = row.get(3)?;
            Ok((path_str, hash_blob, language, size))
        })?;

        match rows.next() {
            Some(Ok((path_str, hash_blob, lang_str, size))) => {
                let path = Utf8PathBuf::from(&path_str);
                let hash = blob_to_hash(&hash_blob);
                let language = lang_str.and_then(|s| str_to_language(&s));
                Ok(Some(FileEntry {
                    path,
                    hash,
                    language,
                    size: size as u64,
                    modified: SystemTime::UNIX_EPOCH,
                }))
            }
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    #[allow(clippy::expect_used)]
    async fn list_files(&self) -> Result<Vec<FileEntry>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        let mut stmt =
            conn.prepare_cached("SELECT path, hash, language, size FROM files ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            let path_str: String = row.get(0)?;
            let hash_blob: Vec<u8> = row.get(1)?;
            let language: Option<String> = row.get(2)?;
            let size: i64 = row.get(3)?;
            Ok((path_str, hash_blob, language, size))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (path_str, hash_blob, lang_str, size) = row?;
            let path = Utf8PathBuf::from(&path_str);
            let hash = blob_to_hash(&hash_blob);
            let language = lang_str.and_then(|s| str_to_language(&s));
            entries.push(FileEntry {
                path,
                hash,
                language,
                size: size as u64,
                modified: SystemTime::UNIX_EPOCH,
            });
        }
        Ok(entries)
    }

    #[allow(clippy::expect_used)]
    async fn delete_file(&self, path: &Utf8Path) -> Result<()> {
        let conn = self.conn.lock().expect("mutex poisoned");
        conn.execute("DELETE FROM files WHERE path = ?1", params![path.as_str()])?;
        Ok(())
    }
}

fn blob_to_hash(blob: &[u8]) -> Blake3Hash {
    let mut bytes = [0u8; 32];
    let len = blob.len().min(32);
    bytes[..len].copy_from_slice(&blob[..len]);
    Blake3Hash::from(bytes)
}

fn language_to_str(lang: Language) -> &'static str {
    match lang {
        Language::Rust => "Rust",
        Language::TypeScript => "TypeScript",
        Language::Python => "Python",
        Language::JavaScript => "JavaScript",
        Language::Markdown => "Markdown",
    }
}

fn str_to_language(s: &str) -> Option<Language> {
    match s {
        "Rust" => Some(Language::Rust),
        "TypeScript" => Some(Language::TypeScript),
        "Python" => Some(Language::Python),
        "JavaScript" => Some(Language::JavaScript),
        "Markdown" => Some(Language::Markdown),
        _ => None,
    }
}
