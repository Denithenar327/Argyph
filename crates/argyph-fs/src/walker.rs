use camino::{Utf8Path, Utf8PathBuf};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::HashSet;
use std::time::SystemTime;

use crate::{hash, language::Language, path, FileEntry, Walker};

/// The default file size cap — 5 MiB.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// An ignore-aware filesystem walker powered by the [`ignore`] crate.
///
/// Respects `.gitignore`, `.ignore`, and system-level git exclude rules.
/// Symlinks that resolve outside the walk root are rejected.
///
/// ```no_run
/// use argyph_fs::{IgnoreWalker, Walker};
/// use camino::Utf8Path;
///
/// let walker = IgnoreWalker::new();
/// for entry in walker.walk(Utf8Path::new(".")) {
///     println!("{}  {}  {:?}", entry.path, entry.hash, entry.language);
/// }
/// ```
pub struct IgnoreWalker {
    max_file_size: u64,
    allowed_extensions: Option<HashSet<String>>,
}

impl IgnoreWalker {
    /// Create a walker with default settings (5 MiB size cap, no extension
    /// filtering).
    pub fn new() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            allowed_extensions: None,
        }
    }

    /// Set the maximum file size in bytes. Files exceeding this are skipped.
    pub fn max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }

    /// Restrict walked files to those whose (lowercase) extension is in the
    /// given set. Pass an empty slice to allow all extensions.
    pub fn allowed_extensions(mut self, exts: &[&str]) -> Self {
        if exts.is_empty() {
            self.allowed_extensions = None;
        } else {
            self.allowed_extensions = Some(exts.iter().map(|e| e.to_lowercase()).collect());
        }
        self
    }
}

impl Default for IgnoreWalker {
    fn default() -> Self {
        Self::new()
    }
}

impl Walker for IgnoreWalker {
    fn walk(&self, root: &Utf8Path) -> impl Iterator<Item = FileEntry> {
        let root_abs = root
            .canonicalize_utf8()
            .unwrap_or_else(|_| root.to_path_buf());
        let max_size = self.max_file_size;
        let allowed = self.allowed_extensions.clone();

        let candidates: Vec<_> = WalkBuilder::new(root_abs.as_std_path())
            .standard_filters(true)
            .build()
            .filter_map(move |entry| {
                let entry = entry.ok()?;

                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    return None;
                }

                let rel = entry
                    .path()
                    .strip_prefix(root_abs.as_std_path())
                    .ok()
                    .and_then(|p| Utf8PathBuf::from_path_buf(p.to_path_buf()).ok())?;

                if let Some(ref allowed) = allowed {
                    let ext = rel
                        .extension()
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    if !allowed.contains(&ext) {
                        return None;
                    }
                }

                let abs = root_abs.join(&rel);

                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => return None,
                };

                let size = metadata.len();
                if size > max_size {
                    return None;
                }

                if !path::is_symlink_safe(&abs, &root_abs) {
                    return None;
                }

                Some((rel, abs, metadata))
            })
            .collect();

        candidates
            .par_iter()
            .filter_map(|(rel, abs, metadata)| {
                let file_hash = match hash::hash_file(abs) {
                    Ok(h) => h,
                    Err(_) => return None,
                };

                let lang = rel.extension().and_then(Language::from_extension);
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

                Some(FileEntry {
                    path: rel.clone(),
                    hash: file_hash,
                    language: lang,
                    size: metadata.len(),
                    modified,
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

/// A simple walker that walks the filesystem without ignore-rule processing.
///
/// Useful as a polling fallback when `git` integration is not needed or when
/// the `ARGYPH_WATCHER=poll` env var is set.
pub struct PollingWalker {
    max_file_size: u64,
}

impl PollingWalker {
    pub fn new() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
        }
    }

    pub fn max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }
}

impl Default for PollingWalker {
    fn default() -> Self {
        Self::new()
    }
}

impl Walker for PollingWalker {
    fn walk(&self, root: &Utf8Path) -> impl Iterator<Item = FileEntry> {
        let root_abs = root
            .canonicalize_utf8()
            .unwrap_or_else(|_| root.to_path_buf());
        let max_size = self.max_file_size;

        let candidates: Vec<_> = ignore::WalkBuilder::new(root_abs.as_std_path())
            .standard_filters(false)
            .hidden(false)
            .build()
            .filter_map(move |entry| {
                let entry = entry.ok()?;
                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    return None;
                }
                let rel = entry
                    .path()
                    .strip_prefix(root_abs.as_std_path())
                    .ok()
                    .and_then(|p| Utf8PathBuf::from_path_buf(p.to_path_buf()).ok())?;
                let abs = root_abs.join(&rel);
                let metadata = entry.metadata().ok()?;
                let size = metadata.len();
                if size > max_size {
                    return None;
                }
                if !path::is_symlink_safe(&abs, &root_abs) {
                    return None;
                }
                Some((rel, abs, metadata))
            })
            .collect();

        candidates
            .par_iter()
            .filter_map(|(rel, abs, metadata)| {
                let file_hash = hash::hash_file(abs).ok()?;
                let lang = rel.extension().and_then(Language::from_extension);
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                Some(FileEntry {
                    path: rel.clone(),
                    hash: file_hash,
                    language: lang,
                    size: metadata.len(),
                    modified,
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::needless_borrows_for_generic_args)]
mod tests {
    use super::*;

    fn fixture_root() -> Utf8PathBuf {
        Utf8PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/tiny-rust-app"
        ))
    }

    #[test]
    fn walk_yields_files() {
        let walker = IgnoreWalker::new();
        let entries: Vec<_> = walker.walk(&fixture_root()).collect();
        // Must find at least the Rust source files
        assert!(!entries.is_empty(), "no entries found in fixture");

        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(
            paths.contains(&"src/main.rs"),
            "missing src/main.rs — got {paths:?}"
        );
        assert!(
            paths.contains(&"src/lib.rs"),
            "missing src/lib.rs — got {paths:?}"
        );
        assert!(
            paths.contains(&"README.md"),
            "missing README.md — got {paths:?}"
        );
    }

    #[test]
    fn language_assignment() {
        let walker = IgnoreWalker::new();
        let entries: Vec<_> = walker.walk(&fixture_root()).collect();
        for entry in &entries {
            if entry.path.as_str().ends_with(".rs") {
                assert_eq!(
                    entry.language,
                    Some(Language::Rust),
                    "{}: expected Rust, got {:?}",
                    entry.path,
                    entry.language
                );
            }
            if entry.path.as_str().ends_with(".md") {
                assert_eq!(
                    entry.language,
                    Some(Language::Markdown),
                    "{}: expected Markdown, got {:?}",
                    entry.path,
                    entry.language
                );
            }
        }
    }

    #[test]
    fn respects_gitignore() {
        let walker = IgnoreWalker::new();
        let entries: Vec<_> = walker.walk(&fixture_root()).collect();
        let junk = entries.iter().find(|e| e.path.as_str().contains("junk"));
        assert!(junk.is_none(), "junk.txt should be excluded by .gitignore");
    }

    #[test]
    fn hash_is_deterministic() {
        let walker = IgnoreWalker::new();
        let first: Vec<_> = walker.walk(&fixture_root()).collect();
        let second: Vec<_> = walker.walk(&fixture_root()).collect();
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.hash, b.hash);
        }
    }

    #[test]
    fn size_cap_skips_large_files() {
        let dir = std::env::temp_dir();
        let small = dir.join("argyph_small.bin");
        let large = dir.join("argyph_large.bin");
        std::fs::write(&small, b"tiny").unwrap();
        std::fs::write(&large, vec![0u8; 1024 * 1024 + 1]).unwrap();

        let root = Utf8PathBuf::from_path_buf(dir).unwrap();
        let walker = IgnoreWalker::new().max_file_size(1024 * 1024);
        let entries: Vec<_> = walker.walk(&root).collect();

        // small file should appear — we only check that large is NOT present
        let has_large = entries
            .iter()
            .any(|e| e.path.as_str().contains("argyph_large"));
        assert!(!has_large, "large file should be skipped by size cap");

        std::fs::remove_file(&small).unwrap();
        std::fs::remove_file(&large).unwrap();
    }

    #[test]
    fn allowed_extensions_filter() {
        let walker = IgnoreWalker::new().allowed_extensions(&["md"]);
        let entries: Vec<_> = walker.walk(&fixture_root()).collect();
        for entry in &entries {
            assert!(
                entry.path.as_str().ends_with(".md"),
                "allowed \"md\" only, but got {}",
                entry.path
            );
        }
        assert!(!entries.is_empty(), "README.md should be included");
    }

    #[test]
    fn walk_returns_deterministic_order() {
        let walker = IgnoreWalker::new();
        let a: Vec<_> = walker.walk(&fixture_root()).map(|e| e.path).collect();
        let b: Vec<_> = walker.walk(&fixture_root()).map(|e| e.path).collect();
        assert_eq!(a, b);
    }
}
