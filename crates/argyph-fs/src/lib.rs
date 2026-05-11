#![forbid(unsafe_code)]

mod hash;
mod language;
mod path;
mod walker;
mod watcher;

use camino::Utf8Path;
use std::time::SystemTime;

pub use hash::{hash_file, Blake3Hash};
pub use language::Language;
pub use walker::{IgnoreWalker, PollingWalker};
pub use watcher::{ChangeKind, ChangedPath, FileWatcher, FsWatcher, PollingWatcher};

/// Walks a repository root, yielding [`FileEntry`] records with path, hash,
/// detected language, size, and modification time. Honors `.gitignore` and
/// related exclusion files.
pub trait Walker {
    fn walk(&self, root: &Utf8Path) -> impl Iterator<Item = FileEntry>;
}

/// A file discovered during repository walking.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to the walk root.
    pub path: camino::Utf8PathBuf,
    /// BLAKE3 content hash.
    pub hash: Blake3Hash,
    /// Detected language, or `None` if unrecognized.
    pub language: Option<Language>,
    /// File size in bytes.
    pub size: u64,
    /// Last modification time (falls back to [`SystemTime::UNIX_EPOCH`] if
    /// the platform cannot determine it).
    pub modified: SystemTime,
}
