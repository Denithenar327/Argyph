use camino::Utf8Path;

use super::{ImportResolver, ModuleTarget};
use crate::edge::Confidence;

pub struct TypeScriptResolver;

impl ImportResolver for TypeScriptResolver {
    fn resolve_import(
        &self,
        source_file: &Utf8Path,
        module_path: &[String],
        _raw: &str,
    ) -> Option<ModuleTarget> {
        if module_path.is_empty() {
            return None;
        }

        let first = &module_path[0];
        if first != "." && first != ".." {
            return None;
        }

        let source_dir = source_file.parent()?;
        let joined = module_path.join("/");
        let resolved = source_dir.join(&joined);

        let candidate = format!("{resolved}.ts");
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
    fn resolve_relative_import() {
        let resolver = TypeScriptResolver;
        let path: Vec<String> = [".", "math"].iter().map(|s| s.to_string()).collect();
        let tgt = resolver
            .resolve_import(
                Utf8Path::new("src/components/App.ts"),
                &path,
                "import { add } from './math'",
            )
            .expect("should resolve ./math");

        assert!(
            tgt.file_path.contains("components/math"),
            "got {}",
            tgt.file_path
        );
    }

    #[test]
    fn resolve_parent_import() {
        let resolver = TypeScriptResolver;
        let path: Vec<String> = ["..", "types"].iter().map(|s| s.to_string()).collect();
        let tgt = resolver
            .resolve_import(
                Utf8Path::new("src/deep/nested/file.ts"),
                &path,
                "import { User } from '../types'",
            )
            .expect("should resolve ../types");

        assert!(
            tgt.file_path.contains("src/deep/types"),
            "got {}",
            tgt.file_path
        );
    }

    #[test]
    fn bare_specifier_returns_none() {
        let resolver = TypeScriptResolver;
        let path: Vec<String> = ["react"].iter().map(|s| s.to_string()).collect();
        assert!(resolver
            .resolve_import(
                Utf8Path::new("src/index.ts"),
                &path,
                "import { useState } from 'react'",
            )
            .is_none());
    }
}
