use camino::Utf8Path;

pub mod python;
pub mod rust;
pub mod typescript;

pub trait ImportResolver {
    fn resolve_import(
        &self,
        source_file: &Utf8Path,
        module_path: &[String],
        raw: &str,
    ) -> Option<ModuleTarget>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTarget {
    pub file_path: String,
    pub confidence: super::edge::Confidence,
}

pub(crate) fn normalize_path(path: &str) -> String {
    let is_absolute = path.starts_with('/');
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let mut out: Vec<&str> = Vec::new();
    for part in parts {
        if part == "." {
            continue;
        }
        if part == ".." {
            out.pop();
        } else {
            out.push(part);
        }
    }
    let result = out.join("/");
    if is_absolute {
        format!("/{result}")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dotdot() {
        assert_eq!(normalize_path("a/b/../c"), "a/c");
    }

    #[test]
    fn strips_dots() {
        assert_eq!(normalize_path("a/./b"), "a/b");
    }

    #[test]
    fn handles_multiple_dotdots() {
        assert_eq!(normalize_path("a/b/../../c"), "c");
    }

    #[test]
    fn preserves_absolute_path() {
        assert_eq!(normalize_path("/a/b/../c"), "/a/c");
    }
}
