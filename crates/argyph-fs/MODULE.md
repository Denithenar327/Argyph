# `argyph-fs` — filesystem layer

## Purpose

Everything that touches the filesystem at the byte/path level: walking, watching, hashing, language detection, ignore rules. This crate isolates the rest of the project from OS-level filesystem quirks.

## Owns

- File-tree walking via the `ignore` crate. Honors `.gitignore`, `.ignore`, `.argyphignore`, and global git excludes.
- Content hashing with BLAKE3.
- Language detection from file extensions (and a small set of shebang heuristics for ambiguous extensions like `.h`).
- The `Walker` trait and its default implementation.
- Filesystem watching via the `notify` crate, with a debouncer.
- A polling fallback walker for sandboxed environments where `notify` cannot install watches.
- Symlink policy (resolve, but never escape the indexed root).
- Per-file size cap enforcement (default 5 MB, configurable).

## Must never own

- Parsing, tokenization, or any content-aware processing beyond hashing and extension-based language detection.
- Persistence of any kind. The walker yields `FileEntry` values; storage is `argyph-store`'s problem.
- Symbol or graph data.
- Embedding logic.
- Any direct interaction with `argyph-mcp` or `argyph-cli`.

## Public surface

```rust
pub trait Walker {
    fn walk(&self, root: &Path) -> impl Iterator<Item = FileEntry>;
}

pub struct DefaultWalker { /* private */ }
pub struct PollingWalker { /* private */ }

pub struct FileEntry {
    pub path: Utf8PathBuf,
    pub hash: Blake3Hash,
    pub language: Option<Language>,
    pub size: u64,
    pub modified: SystemTime,
}

pub enum Language {
    Rust, TypeScript, Python, /* added per language pack */
}

pub struct FsWatcher { /* private */ }

impl FsWatcher {
    pub fn new(root: &Path, debounce: Duration) -> Result<Self>;
    pub async fn next_batch(&mut self) -> Vec<ChangedPath>;
    pub fn shutdown(self);
}

pub struct ChangedPath {
    pub path: Utf8PathBuf,
    pub kind: ChangeKind, // Created | Modified | Deleted | Renamed
}
```

## Internal structure

- `src/walker.rs` — default and polling walkers.
- `src/hash.rs` — BLAKE3 wrapper.
- `src/language.rs` — extension and shebang detection.
- `src/watcher.rs` — `notify` integration and debouncer.
- `src/path.rs` — root-relative path normalization, traversal protection.

## Failure modes

- Windows path separator and case-sensitivity bugs. Always use `camino::Utf8Path` and normalize through `path::validate_repo_path()`.
- Watcher OS quirks: macOS FSEvents coalesces aggressively; Linux inotify has a per-user watch limit (default 8192) that big repos blow up; Windows ReadDirectoryChangesW has its own rename quirks. Test in CI on all three OSes.
- Symlinks pointing outside the repo. Resolve but reject any path that does not normalize to a descendant of the indexed root.
- Per-file size cap: an AI agent will be tempted to remove this for "completeness." Don't. It is a real DoS guard.

## Honest limitations

- We do not follow symlinks across filesystems by default.
- Renames are reported as a delete + create pair on most platforms. Higher layers (`argyph-graph`) have to be tolerant of this.
- Polling fallback is O(repo size) per poll. Acceptable for sandboxed CI use, not great for huge repos.

## Stability

- The `Walker` trait and `FileEntry` shape are part of the inter-crate contract. Adding fields requires a coordinated change with `argyph-store`.
- The `notify`-based watcher behavior may change as upstream evolves; our debouncer is the stable surface.
