#![forbid(unsafe_code)]

// TODO: See crates/argyph-fs/MODULE.md — owns file-tree walking (ignore-aware),
// BLAKE3 content hashing, language detection via extension, filesystem watching,
// and symlink traversal protection.

/// Walks a repository root, yielding [`FileEntry`] records with path, hash,
/// detected language, size, and modification time. Honors `.gitignore` and
/// related exclusion files.
pub trait Walker {}
