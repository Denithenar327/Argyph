#![forbid(unsafe_code)]

mod priority;
pub mod render;
pub mod tokenize;
pub mod truncate;

use camino::{Utf8Path, Utf8PathBuf};
use std::time::SystemTime;

use priority::prioritize;
use tokenize::TokenCounter;
use truncate::truncate_file;

/// Errors that can occur during repository packing.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    /// The scope resolved to zero candidate files — nothing to pack.
    #[error("empty scope: no files to pack")]
    EmptyScope,
    /// The token budget cannot even cover per-file overhead for the candidate
    /// set. The value is the minimum budget required.
    #[error("token budget too small: {0} bytes minimum required")]
    BudgetTooSmall(usize),
    /// An I/O or other infrastructure error prevented reading a file.
    #[error("IO error: {0}")]
    Io(String),
}

/// Convenience alias for [`std::result::Result`] with [`PackError`].
pub type Result<T> = std::result::Result<T, PackError>;

/// What subset of the repository to pack.
///
/// `All` includes every file the [`PackContext`] considers in scope. `Paths`
/// restricts to explicit paths. `Symbol` packs files that contain a named
/// symbol (lookup provided by the context).
pub enum PackScope {
    All,
    Paths(Vec<Utf8PathBuf>),
    Symbol(String),
}

/// Output format for the packed representation.
#[derive(Debug)]
pub enum PackFormat {
    /// Primary format — XML with `<file>` elements and CDATA-wrapped content.
    Xml,
    /// Human-friendly format — markdown with fenced code blocks.
    Markdown,
}

/// Flags controlling which categories of files are included in the pack.
pub struct PackInclude {
    /// When `true`, test files (e.g. `*_test.rs`, `tests/` directory) are
    /// included.
    pub tests: bool,
    /// When `true`, documentation files (`.md`) are included.
    pub docs: bool,
}

/// Describes a pack operation to be executed by a [`Packer`].
pub struct PackRequest {
    pub scope: PackScope,
    pub format: PackFormat,
    /// Soft token budget — the packer attempts to stay under this limit.
    pub token_budget: usize,
    pub include: PackInclude,
}

/// The result of a successful pack operation.
#[derive(Debug)]
pub struct PackResult {
    pub format: PackFormat,
    /// The rendered output string (XML or markdown).
    pub content: String,
    /// Token count of the final output as measured by the default tokenizer.
    pub token_count: usize,
    /// Files that were included in full.
    pub files_included: Vec<Utf8PathBuf>,
    /// Files that were included but truncated to fit the budget.
    pub files_truncated: Vec<Utf8PathBuf>,
    /// Files that could not be included at all under the budget.
    pub files_omitted: Vec<Utf8PathBuf>,
}

/// Abstracts filesystem access and metadata needed by the packer.
///
/// All path arguments use [`Utf8Path`] from the `camino` crate. Implementors
/// do not need to perform `std::path` conversions — the packer only operates
/// on UTF-8 paths.
pub trait PackContext {
    /// Return every file path that matches the given scope.
    fn list_files(&self, scope: &PackScope) -> Vec<Utf8PathBuf>;
    /// Read the full contents of `file` as a string.
    fn read(&self, file: &Utf8Path) -> Result<String>;
    /// Return the last-modified timestamp of `file`, if available.
    fn modified(&self, file: &Utf8Path) -> Option<SystemTime>;
    /// Return the count of inbound edges (callers, importers) for `file` in
    /// the symbol graph. Returns 0 if no graph data is available.
    fn in_edges(&self, file: &Utf8Path) -> Result<usize>;
}

/// Produces a token-budgeted, flattened representation of a repository or
/// subset.
pub trait Packer {
    /// Execute a pack operation given a request and a context for file access.
    fn pack(&self, req: &PackRequest, ctx: &dyn PackContext) -> Result<PackResult>;
}

/// Default implementation of [`Packer`] using the `cl100k_base` tokenizer and
/// a priority heuristic that sorts files by entry points, docs, recency, and
/// symbol-graph centrality before lexicographic order.
pub struct DefaultPacker {
    counter: TokenCounter,
}

impl DefaultPacker {
    /// Create a new `DefaultPacker`, initialising the tokenizer.
    pub fn new() -> Result<Self> {
        Ok(Self {
            counter: TokenCounter::new()?,
        })
    }
}

impl Packer for DefaultPacker {
    fn pack(&self, req: &PackRequest, ctx: &dyn PackContext) -> Result<PackResult> {
        let mut files = ctx.list_files(&req.scope);

        if !req.include.tests {
            files.retain(|f| !is_test_file(f));
        }
        if !req.include.docs {
            files.retain(|f| !is_doc_file(f));
        }

        if files.is_empty() {
            return Err(PackError::EmptyScope);
        }

        let ordered = prioritize(&files, ctx);

        let (file_entries, _budget_used) =
            self.read_with_budget(&ordered, req.token_budget, &req.format, ctx)?;

        let mut files_included = Vec::new();
        let mut files_truncated = Vec::new();
        for (path, _, is_truncated, _) in &file_entries {
            if *is_truncated {
                files_truncated.push(path.clone());
            } else {
                files_included.push(path.clone());
            }
        }

        let included_set: std::collections::HashSet<_> =
            file_entries.iter().map(|(p, _, _, _)| p).collect();
        let files_omitted: Vec<_> = ordered
            .iter()
            .filter(|p| !included_set.contains(p))
            .cloned()
            .collect();

        let file_refs: Vec<(Utf8PathBuf, &str, bool, usize)> = file_entries
            .iter()
            .map(|(p, c, t, n)| (p.clone(), c.as_str(), *t, *n))
            .collect();

        let content = match req.format {
            PackFormat::Xml => render::xml::render_xml(&file_refs, "repository"),
            PackFormat::Markdown => {
                render::markdown::render_markdown(&file_refs, "repository")
            }
        };

        let token_count = self.counter.count(&content);

        Ok(PackResult {
            format: match req.format {
                PackFormat::Xml => PackFormat::Xml,
                PackFormat::Markdown => PackFormat::Markdown,
            },
            content,
            token_count,
            files_included,
            files_truncated,
            files_omitted,
        })
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// Internal representation of a file packed into the result.
type FileEntry = (Utf8PathBuf, String, bool, usize);

impl DefaultPacker {
    fn read_with_budget(
        &self,
        ordered: &[Utf8PathBuf],
        budget: usize,
        format: &PackFormat,
        ctx: &dyn PackContext,
    ) -> Result<(Vec<FileEntry>, usize)> {
        let overhead_per_file: usize = match format {
            PackFormat::Xml => 120,
            PackFormat::Markdown => 80,
        };

        let total_overhead = overhead_per_file.saturating_mul(ordered.len());
        if total_overhead >= budget {
            return Err(PackError::BudgetTooSmall(total_overhead));
        }

        let mut remaining_budget = budget.saturating_sub(total_overhead);
        let mut entries: Vec<(Utf8PathBuf, String, bool, usize)> = Vec::new();
        let file_count = ordered.len();

        for (idx, file) in ordered.iter().enumerate() {
            if remaining_budget == 0 {
                break;
            }

            let remaining_files = file_count.saturating_sub(entries.len());
            let per_file = remaining_budget / remaining_files.max(1);
            if per_file == 0 {
                break;
            }

            let content = match ctx.read(file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let full_count = self.counter.count(&content);

            if full_count <= per_file {
                remaining_budget = remaining_budget.saturating_sub(full_count);
                entries.push((file.clone(), content, false, full_count));
            } else {
                let (truncated, trunc_count) =
                    truncate_file(&content, per_file, &self.counter);
                remaining_budget = remaining_budget.saturating_sub(trunc_count);
                // Only include if we got some content
                if trunc_count > 0 {
                    entries.push((file.clone(), truncated, true, trunc_count));
                }
                // If truncation produced nothing useful, the file is omitted
            }

            // Mark the file position as used (handled implicitly by loop)
            let _ = idx;
        }

        let total_used = entries.iter().map(|(_, _, _, c)| c).sum::<usize>()
            + overhead_per_file.saturating_mul(entries.len());

        Ok((entries, total_used))
    }
}

/// Returns `true` when `path` looks like a test file or lives inside a test
/// directory.
fn is_test_file(path: &Utf8Path) -> bool {
    let file_name = path.file_name().unwrap_or("");
    // Filename patterns
    if file_name.ends_with("_test.rs")
        || file_name.ends_with("_test.ts")
        || file_name.ends_with("_test.tsx")
        || file_name.ends_with("_test.js")
        || file_name.ends_with("_test.jsx")
        || file_name.ends_with("_test.py")
        || file_name.ends_with("_spec.ts")
        || file_name.ends_with("_spec.js")
        || file_name.ends_with("test.py")
    {
        return true;
    }
    // Directory patterns
    let path_str = path.as_str();
    if path_str.contains("/test/")
        || path_str.contains("/tests/")
        || path_str.starts_with("test/")
        || path_str.starts_with("tests/")
        || path_str.contains("/__tests__/")
        || path_str.contains("/spec/")
    {
        return true;
    }
    false
}

/// Returns `true` when `path` refers to a documentation file (currently only
/// `.md` files).
fn is_doc_file(path: &Utf8Path) -> bool {
    path.extension() == Some("md")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct TestContext {
        files: HashMap<Utf8PathBuf, (String, usize)>,
        mtimes: HashMap<Utf8PathBuf, SystemTime>,
        in_edges: HashMap<Utf8PathBuf, usize>,
    }

    impl TestContext {
        fn with_content_files(files: HashMap<Utf8PathBuf, String>) -> Self {
            let file_map = files
                .into_iter()
                .map(|(k, v)| (k, (v, 0)))
                .collect();
            Self {
                files: file_map,
                mtimes: HashMap::new(),
                in_edges: HashMap::new(),
            }
        }
    }

    impl PackContext for TestContext {
        fn list_files(&self, _scope: &PackScope) -> Vec<Utf8PathBuf> {
            let mut paths: Vec<_> = self.files.keys().cloned().collect();
            paths.sort();
            paths
        }

        fn read(&self, file: &Utf8Path) -> Result<String> {
            self.files
                .get(file)
                .map(|(content, _)| content.clone())
                .ok_or_else(|| PackError::Io(format!("file not found: {file}")))
        }

        fn modified(&self, file: &Utf8Path) -> Option<SystemTime> {
            self.mtimes.get(file).copied()
        }

        fn in_edges(&self, file: &Utf8Path) -> Result<usize> {
            Ok(self.in_edges.get(file).copied().unwrap_or(0))
        }
    }

    fn path(s: &str) -> Utf8PathBuf {
        Utf8PathBuf::from(s)
    }

    // ── types + errors ───────────────────────────────────────────────────

    #[test]
    fn empty_scope_errors() {
        let packer = DefaultPacker::new().unwrap();
        let ctx = TestContext::with_content_files(HashMap::new());
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 1000,
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let result = packer.pack(&req, &ctx);
        assert!(result.is_err());
        match result.unwrap_err() {
            PackError::EmptyScope => {}
            other => panic!("expected EmptyScope, got {other:?}"),
        }
    }

    #[test]
    fn budget_too_small_errors() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(
            path("src/main.rs"),
            "fn main() { println!(\"hello\"); }".to_string(),
        );
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 10, // far too small for overhead
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let result = packer.pack(&req, &ctx);
        assert!(result.is_err());
    }

    // ── basic packing ────────────────────────────────────────────────────

    #[test]
    fn pack_single_file_in_full() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(
            path("src/main.rs"),
            "fn main() {}".to_string(),
        );
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 500,
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        assert_eq!(result.files_included.len(), 1);
        assert_eq!(result.files_truncated.len(), 0);
        assert_eq!(result.files_omitted.len(), 0);
        assert!(result.token_count > 0);
        assert!(result.content.contains("fn main() {}"));
    }

    #[test]
    fn pack_multiple_files_orders_by_priority() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(path("src/utils.rs"), "// utils".to_string());
        files.insert(path("src/lib.rs"), "// lib".to_string());
        files.insert(path("README.md"), "# Readme".to_string());
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 2000,
            include: PackInclude {
                tests: true,
                docs: true,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        // lib.rs (entry point) should appear before README.md (top-level doc)
        // which should appear before utils.rs
        let lib_pos = result.files_included.iter().position(|p| p.as_str() == "src/lib.rs").unwrap();
        let readme_pos = result.files_included.iter().position(|p| p.as_str() == "README.md").unwrap();
        let utils_pos = result.files_included.iter().position(|p| p.as_str() == "src/utils.rs").unwrap();
        assert!(lib_pos < readme_pos);
        assert!(readme_pos < utils_pos);
    }

    #[test]
    fn pack_excludes_tests_when_flag_false() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(path("src/lib.rs"), "// lib".to_string());
        files.insert(path("src/lib_test.rs"), "// test".to_string());
        files.insert(path("tests/integration.rs"), "// integration".to_string());
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 2000,
            include: PackInclude {
                tests: false,
                docs: true,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        assert_eq!(result.files_included.len(), 1);
        assert_eq!(result.files_included[0].as_str(), "src/lib.rs");
    }

    #[test]
    fn pack_includes_tests_when_flag_true() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(path("src/lib.rs"), "// lib".to_string());
        files.insert(path("src/lib_test.rs"), "// test".to_string());
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 2000,
            include: PackInclude {
                tests: true,
                docs: true,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        assert_eq!(result.files_included.len(), 2);
    }

    #[test]
    fn pack_truncates_when_budget_tight() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        // A large file that can't fit under a small budget — use varied
        // words so the BPE tokenizer does not compress them into few tokens.
        let big_content: String = std::iter::repeat("fn unique_word_")
            .take(200)
            .collect::<Vec<_>>()
            .join("\n");
        files.insert(path("src/big.rs"), big_content);
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 200,
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        // The large file should not be included in full — it is either
        // truncated or omitted entirely depending on budget allocation.
        assert_eq!(result.files_included.len(), 0);
        assert!(result.files_truncated.len() + result.files_omitted.len() == 1);
        if !result.files_truncated.is_empty() {
            assert!(result.content.contains("[truncated"));
        }
    }

    #[test]
    fn markdown_format_output() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(path("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string());
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Markdown,
            token_budget: 2000,
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        assert!(result.content.starts_with("# Repository:"));
        assert!(result.content.contains("```rust"));
        assert!(result.content.contains("## File:"));
    }

    #[test]
    fn pack_result_is_deterministic() {
        let packer = DefaultPacker::new().unwrap();
        let mut files = HashMap::new();
        files.insert(path("a.rs"), "// a".to_string());
        files.insert(path("b.rs"), "// b".to_string());
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Xml,
            token_budget: 2000,
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let r1 = packer.pack(&req, &ctx).unwrap();
        let r2 = packer.pack(&req, &ctx).unwrap();
        assert_eq!(r1.content, r2.content);
        assert_eq!(r1.token_count, r2.token_count);
    }

    #[test]
    fn file_content_is_preserved_in_output() {
        let packer = DefaultPacker::new().unwrap();
        let content = "fn hello() -> &'static str { \"world\" }";
        let mut files = HashMap::new();
        files.insert(path("src/greeting.rs"), content.to_string());
        let ctx = TestContext::with_content_files(files);
        let req = PackRequest {
            scope: PackScope::All,
            format: PackFormat::Markdown,
            token_budget: 2000,
            include: PackInclude {
                tests: false,
                docs: false,
            },
        };
        let result = packer.pack(&req, &ctx).unwrap();
        assert!(result.content.contains(content));
    }
}
