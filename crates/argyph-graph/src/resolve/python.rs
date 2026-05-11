use camino::Utf8Path;

use super::{ImportResolver, ModuleTarget};
use crate::edge::Confidence;

pub struct PythonResolver;

impl ImportResolver for PythonResolver {
    fn resolve_import(
        &self,
        source_file: &Utf8Path,
        _module_path: &[String],
        raw: &str,
    ) -> Option<ModuleTarget> {
        let trimmed = raw.trim();
        if !trimmed.starts_with("from .") {
            return None;
        }

        let after_from = trimmed.strip_prefix("from ")?;
        let module_str = after_from.split(" import ").next()?.trim();

        let source_dir = source_file.parent()?;
        let mut file = source_dir.to_path_buf();

        let depth = module_str.chars().take_while(|c| *c == '.').count();
        for _ in 0..depth.saturating_sub(1) {
            if !file.pop() {}
        }

        let remaining = &module_str[depth..];
        if remaining.is_empty() {
            file.push("__init__");
        } else {
            for seg in remaining.split('.') {
                if !seg.is_empty() {
                    file.push(seg);
                }
            }
        }

        let candidate = format!("{file}.py");
        Some(ModuleTarget {
            file_path: super::normalize_path(&candidate),
            confidence: Confidence::Heuristic,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    #[test]
    fn resolve_relative_dot_import() {
        let resolver = PythonResolver;
        let path: Vec<String> = ["math"].iter().map(|s| s.to_string()).collect();
        let tgt = resolver
            .resolve_import(Utf8Path::new("src/main.py"), &path, "from .math import add")
            .expect("should resolve from .math import add");

        assert!(tgt.file_path.contains("src/math"), "got {}", tgt.file_path);
        assert!(tgt.file_path.ends_with(".py"));
    }

    #[test]
    fn resolve_parent_import() {
        let resolver = PythonResolver;
        let tgt = resolver
            .resolve_import(
                Utf8Path::new("src/subpkg/module.py"),
                &[],
                "from ..types import User",
            )
            .expect("should resolve from ..types import User");

        assert!(tgt.file_path.contains("src/types"), "got {}", tgt.file_path);
    }

    #[test]
    fn bare_import_returns_none() {
        let resolver = PythonResolver;
        let path: Vec<String> = ["os"].iter().map(|s| s.to_string()).collect();
        assert!(resolver
            .resolve_import(Utf8Path::new("src/main.py"), &path, "import os",)
            .is_none());
    }
}
