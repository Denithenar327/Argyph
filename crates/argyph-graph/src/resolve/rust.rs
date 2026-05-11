use camino::Utf8Path;

use super::{ImportResolver, ModuleTarget};
use crate::edge::Confidence;

pub struct RustResolver;

impl ImportResolver for RustResolver {
    fn resolve_import(
        &self,
        _source_file: &Utf8Path,
        module_path: &[String],
        _raw: &str,
    ) -> Option<ModuleTarget> {
        if module_path.is_empty() {
            return None;
        }

        let path_str = module_path.join("/");
        let mut replaced = path_str.replace("::", "/");

        if let Some(rest) = replaced.strip_prefix("crate/") {
            replaced = rest.to_string();
        }

        let candidate = format!("src/{replaced}.rs");
        Some(ModuleTarget {
            file_path: candidate,
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
    fn resolve_crate_module() {
        let resolver = RustResolver;
        let tgt = resolver
            .resolve_import(
                Utf8Path::new("src/main.rs"),
                &["crate", "math", "add"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                "use crate::math::add;",
            )
            .expect("should resolve crate::math::add");
        assert_eq!(tgt.file_path, "src/math/add.rs");
        assert_eq!(tgt.confidence, Confidence::Heuristic);
    }

    #[test]
    fn resolve_rust_module_path() {
        let resolver = RustResolver;
        let tgt = resolver
            .resolve_import(
                Utf8Path::new("src/lib.rs"),
                &["foo", "bar", "Baz"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                "use foo::bar::Baz;",
            )
            .expect("should resolve foo::bar::Baz");
        assert_eq!(tgt.file_path, "src/foo/bar/Baz.rs");
    }
}
