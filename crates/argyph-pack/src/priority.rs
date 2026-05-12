use crate::PackContext;
use camino::Utf8PathBuf;
use std::collections::HashSet;
use std::time::{Duration, SystemTime};

const ENTRY_POINTS: &[&str] = &[
    "main.rs", "lib.rs", "index.ts", "index.tsx", "__main__.py", "main.py",
];

const TOP_LEVEL_DOCS: &[&str] = &[
    "README.md",
    "README.rst",
    "CONTRIBUTING.md",
    "ARCHITECTURE.md",
    "LICENSE",
];

const RECENT_WINDOW: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Order files by the priority heuristic:
///
/// 1. Entry points (`main.rs`, `lib.rs`, `index.ts`, `index.tsx`, `__main__.py`,
///    `main.py`) — matched by filename only.
/// 2. Top-level docs (`README.md`, `README.rst`, `CONTRIBUTING.md`,
///    `ARCHITECTURE.md`, `LICENSE`) — top-level only, no directory prefix.
/// 3. Recently modified (within the last 7 days), lexicographic within group.
/// 4. High in-edge count (descending), excluding files already placed.
/// 5. Remaining files in lexicographic order.
///
/// Files already placed in an earlier group are excluded from subsequent groups.
pub fn prioritize(files: &[Utf8PathBuf], ctx: &dyn PackContext) -> Vec<Utf8PathBuf> {
    let mut placed: HashSet<usize> = HashSet::new();
    let mut result: Vec<Utf8PathBuf> = Vec::with_capacity(files.len());

    let now = SystemTime::now();

    // 1. Entry points
    for (i, f) in files.iter().enumerate() {
        if placed.contains(&i) {
            continue;
        }
        let file_name = f.file_name().unwrap_or("");
        if ENTRY_POINTS.contains(&file_name) {
            result.push(f.clone());
            placed.insert(i);
        }
    }

    // 2. Top-level docs
    for (i, f) in files.iter().enumerate() {
        if placed.contains(&i) {
            continue;
        }
        let file_name = f.file_name().unwrap_or("");
        if TOP_LEVEL_DOCS.contains(&file_name) && is_top_level(f) {
            result.push(f.clone());
            placed.insert(i);
        }
    }

    // 3. Recently modified (lexicographic within group)
    let mut recent: Vec<(usize, &Utf8PathBuf)> = files
        .iter()
        .enumerate()
        .filter(|(i, f)| {
            if placed.contains(i) {
                return false;
            }
            if let Some(mtime) = ctx.modified(f) {
                if let Ok(age) = now.duration_since(mtime) {
                    return age <= RECENT_WINDOW;
                }
            }
            false
        })
        .collect();
    recent.sort_by(|a, b| a.1.cmp(b.1));
    for (i, f) in recent {
        if !placed.contains(&i) {
            result.push(f.clone());
            placed.insert(i);
        }
    }

    // 4. High in-edge count (descending)
    let mut high_edge: Vec<(usize, &Utf8PathBuf, usize)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            if placed.contains(&i) {
                return None;
            }
            let count = ctx.in_edges(f).unwrap_or(0);
            Some((i, f, count))
        })
        .collect();
    high_edge.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(b.1)));
    for (i, f, _count) in high_edge {
        if !placed.contains(&i) {
            result.push(f.clone());
            placed.insert(i);
        }
    }

    // 5. Remaining files (lexicographic)
    let mut remaining: Vec<(usize, &Utf8PathBuf)> = files
        .iter()
        .enumerate()
        .filter(|(i, _)| !placed.contains(i))
        .collect();
    remaining.sort_by(|a, b| a.1.cmp(b.1));
    for (_i, f) in remaining {
        result.push(f.clone());
    }

    result
}

fn is_top_level(path: &camino::Utf8PathBuf) -> bool {
    path.parent().is_none() || path.parent() == Some(camino::Utf8Path::new(""))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use camino::Utf8Path;
    use std::collections::HashMap;
    use std::time::SystemTime;

    struct TestContext {
        files: HashMap<Utf8PathBuf, (String, usize)>,
        mtimes: HashMap<Utf8PathBuf, SystemTime>,
        in_edges: HashMap<Utf8PathBuf, usize>,
    }

    impl TestContext {
        fn new(
            files: HashMap<Utf8PathBuf, (String, usize)>,
            mtimes: HashMap<Utf8PathBuf, SystemTime>,
            in_edges: HashMap<Utf8PathBuf, usize>,
        ) -> Self {
            Self {
                files,
                mtimes,
                in_edges,
            }
        }
    }

    impl PackContext for TestContext {
        fn list_files(&self, _scope: &crate::PackScope) -> Vec<Utf8PathBuf> {
            let mut paths: Vec<_> = self.files.keys().cloned().collect();
            paths.sort();
            paths
        }

        fn read(&self, file: &Utf8Path) -> crate::Result<String> {
            self.files
                .get(file)
                .map(|(content, _)| content.clone())
                .ok_or_else(|| crate::PackError::Io(format!("file not found: {file}")))
        }

        fn modified(&self, file: &Utf8Path) -> Option<SystemTime> {
            self.mtimes.get(file).copied()
        }

        fn in_edges(&self, file: &Utf8Path) -> crate::Result<usize> {
            Ok(self.in_edges.get(file).copied().unwrap_or(0))
        }
    }

    fn path(s: &str) -> Utf8PathBuf {
        Utf8PathBuf::from(s)
    }

    #[test]
    fn entry_points_come_first() {
        let files = vec![
            path("src/utils.rs"),
            path("src/main.rs"),
            path("src/lib.rs"),
            path("src/foo.rs"),
        ];
        let ctx = TestContext::new(
            files
                .iter()
                .map(|p| (p.clone(), (String::new(), 0)))
                .collect(),
            HashMap::new(),
            HashMap::new(),
        );
        let ordered = prioritize(&files, &ctx);
        // Entry points first, then lexicographic
        assert!(ordered[0].file_name().unwrap() == "lib.rs" || ordered[0].file_name().unwrap() == "main.rs");
        assert!(ordered[1].file_name().unwrap() == "lib.rs" || ordered[1].file_name().unwrap() == "main.rs");
    }

    #[test]
    fn top_level_readme_before_other_files() {
        let files = vec![
            path("src/foo.rs"),
            path("README.md"),
            path("src/bar.rs"),
        ];
        let ctx = TestContext::new(
            files
                .iter()
                .map(|p| (p.clone(), (String::new(), 0)))
                .collect(),
            HashMap::new(),
            HashMap::new(),
        );
        let ordered = prioritize(&files, &ctx);
        assert_eq!(ordered[0].file_name().unwrap(), "README.md");
    }

    #[test]
    fn nested_readme_not_prioritized_as_top_level() {
        let files = vec![
            path("src/foo.rs"),
            path("docs/README.md"),
            path("src/bar.rs"),
        ];
        let ctx = TestContext::new(
            files
                .iter()
                .map(|p| (p.clone(), (String::new(), 0)))
                .collect(),
            HashMap::new(),
            HashMap::new(),
        );
        let ordered = prioritize(&files, &ctx);
        // "docs/README.md" is not top-level, so it should not precede ordinary
        // files by filename
        let readme_pos = ordered
            .iter()
            .position(|p| p.as_str() == "docs/README.md")
            .unwrap();
        let foo_pos = ordered
            .iter()
            .position(|p| p.as_str() == "src/foo.rs")
            .unwrap();
        // Since no entry points exist, both go to lexicographic group
        // "docs/README.md" < "src/bar.rs" < "src/foo.rs" lexicographically
        assert!(readme_pos < foo_pos);
    }

    #[test]
    fn recently_modified_comes_before_lexicographic() {
        let now = SystemTime::now();
        let recent = now - Duration::from_secs(3600); // 1 hour ago
        let files = vec![
            path("src/old.rs"),
            path("src/recent.rs"),
            path("src/zebra.rs"),
        ];
        let mut mtimes = HashMap::new();
        mtimes.insert(path("src/recent.rs"), recent);
        let ctx = TestContext::new(
            files
                .iter()
                .map(|p| (p.clone(), (String::new(), 0)))
                .collect(),
            mtimes,
            HashMap::new(),
        );
        let ordered = prioritize(&files, &ctx);
        // recent.rs should appear before the other two lexicographic files
        let recent_pos = ordered
            .iter()
            .position(|p| p.as_str() == "src/recent.rs")
            .unwrap();
        let old_pos = ordered
            .iter()
            .position(|p| p.as_str() == "src/old.rs")
            .unwrap();
        assert!(recent_pos < old_pos);
    }

    #[test]
    fn high_in_edges_before_lexicographic() {
        let files = vec![
            path("src/low.rs"),
            path("src/high.rs"),
            path("src/medium.rs"),
        ];
        let mut in_edges = HashMap::new();
        in_edges.insert(path("src/high.rs"), 10);
        in_edges.insert(path("src/medium.rs"), 5);
        in_edges.insert(path("src/low.rs"), 0);
        let ctx = TestContext::new(
            files
                .iter()
                .map(|p| (p.clone(), (String::new(), 0)))
                .collect(),
            HashMap::new(),
            in_edges,
        );
        let ordered = prioritize(&files, &ctx);
        let high_pos = ordered
            .iter()
            .position(|p| p.as_str() == "src/high.rs")
            .unwrap();
        let medium_pos = ordered
            .iter()
            .position(|p| p.as_str() == "src/medium.rs")
            .unwrap();
        let low_pos = ordered
            .iter()
            .position(|p| p.as_str() == "src/low.rs")
            .unwrap();
        assert!(high_pos < medium_pos);
        assert!(medium_pos < low_pos);
    }

    #[test]
    fn entry_point_not_duplicated() {
        let files = vec![
            path("src/main.rs"),
            path("src/utils.rs"),
        ];
        let mut in_edges = HashMap::new();
        in_edges.insert(path("src/main.rs"), 100);
        let ctx = TestContext::new(
            files
                .iter()
                .map(|p| (p.clone(), (String::new(), 0)))
                .collect(),
            HashMap::new(),
            in_edges,
        );
        let ordered = prioritize(&files, &ctx);
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].file_name().unwrap(), "main.rs");
        // main.rs appears exactly once
        assert_eq!(ordered.iter().filter(|p| p.file_name().unwrap() == "main.rs").count(), 1);
    }
}
