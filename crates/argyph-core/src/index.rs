use std::sync::Arc;
use std::time::SystemTime;

use argyph_fs::FileEntry;
use argyph_store::Store;
use camino::Utf8Path;

use crate::error::Result;

/// The single domain facade that UI layers consume.
///
/// All queries go through `Index`; no caller outside `argyph-core` touches the
/// underlying [`Store`] directly.
pub struct Index {
    store: Arc<dyn Store>,
}

impl Index {
    pub(crate) fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Returns a fixed-version string used by MCP `get_index_status`.
    pub fn protocol_version() -> &'static str {
        "0.1.0"
    }

    /// Look up a single file by its repository-relative path.
    pub async fn get_file(&self, path: &Utf8Path) -> Result<Option<FileEntry>> {
        Ok(self.store.get_file(path).await?)
    }

    /// Return every file entry in the index, ordered by path.
    pub async fn list_files(&self) -> Result<Vec<FileEntry>> {
        Ok(self.store.list_files().await?)
    }

    /// Snapshot of the current index — tier state, file count, and timings.
    pub async fn status(&self) -> Result<IndexStatus> {
        let files = self.store.list_files().await?;
        Ok(IndexStatus {
            protocol_version: Self::protocol_version().to_string(),
            file_count: files.len() as u64,
            snapshot_at: SystemTime::now(),
        })
    }
}

/// Read-only snapshot returned by [`Index::status`].
#[derive(Debug, Clone)]
pub struct IndexStatus {
    pub protocol_version: String,
    pub file_count: u64,
    pub snapshot_at: SystemTime,
}
