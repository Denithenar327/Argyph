/// A parsed structural path locator.
///
/// - `/doc/Design` → `Locator::Heading(vec!["Design"])`
/// - `#Summary` → `Locator::Heading(vec!["Summary"])`
/// - `file://src/lib.rs/doc/Design` → `Locator::FilePlusHeading { file: "src/lib.rs", path: ["Design"] }`
/// - `any` / `*` → `Locator::Any`
/// - bare string → `Locator::Name(String)`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Heading(Vec<String>),
    FilePlusHeading { file: String, path: Vec<String> },
    Any,
    Name(String),
}

impl std::fmt::Display for Locator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Locator::Heading(parts) => {
                if parts.is_empty() {
                    write!(f, "/")
                } else {
                    write!(f, "/{}", parts.join("/"))
                }
            }
            Locator::FilePlusHeading { file, path } => {
                if path.is_empty() {
                    write!(f, "file://{file}")
                } else {
                    write!(f, "file://{}/{}", file, path.join("/"))
                }
            }
            Locator::Any => write!(f, "*"),
            Locator::Name(name) => write!(f, "{name}"),
        }
    }
}

/// Parse a path string into a [`Locator`].
///
/// - `"/"` → `Heading(vec![]`)` (root)
/// - `"/doc/Design"` → `Heading(vec!["doc", "Design"])`
/// - `"/Introduction"` → `Heading(vec!["Introduction"])`
/// - `"#Summary"` → `Heading(vec!["Summary"])`
/// - `"file://src/lib.rs/doc/Design"` → `FilePlusHeading`
/// - `"*"` / `"any"` → `Any`
/// - bare word → `Name`
pub fn parse(raw: &str) -> Locator {
    let trimmed = raw.trim();

    if trimmed == "*" || trimmed == "any" {
        return Locator::Any;
    }

    if trimmed.starts_with('/') || trimmed.starts_with('#') {
        let stripped = if let Some(rest) = trimmed.strip_prefix('#') {
            rest
        } else {
            trimmed.strip_prefix('/').unwrap_or(trimmed)
        };
        if stripped.is_empty() {
            return Locator::Heading(vec![]);
        }
        let parts: Vec<String> = stripped.split('/').map(|s| s.to_string()).collect();
        return Locator::Heading(parts);
    }

    if let Some(rest) = trimmed.strip_prefix("file://") {
        let parts: Vec<&str> = rest.split('/').collect();
        let last_file_idx = parts.iter().rposition(|p| p.contains('.')).unwrap_or(0);
        let file = parts[..=last_file_idx].join("/");
        let path: Vec<String> = parts[last_file_idx + 1..]
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        return Locator::FilePlusHeading { file, path };
    }

    Locator::Name(trimmed.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn root_slash_is_empty_heading() {
        assert_eq!(parse("/"), Locator::Heading(vec![]));
    }

    #[test]
    fn slash_doc_design_is_heading() {
        assert_eq!(
            parse("/doc/Design"),
            Locator::Heading(vec!["doc".into(), "Design".into()])
        );
    }

    #[test]
    fn hash_summary_is_heading() {
        assert_eq!(parse("#Summary"), Locator::Heading(vec!["Summary".into()]));
    }

    #[test]
    fn file_plus_heading() {
        assert_eq!(
            parse("file://src/lib.rs/doc/Design"),
            Locator::FilePlusHeading {
                file: "src/lib.rs".into(),
                path: vec!["doc".into(), "Design".into()]
            }
        );
    }

    #[test]
    fn file_only_no_path() {
        assert_eq!(
            parse("file://src/lib.rs"),
            Locator::FilePlusHeading {
                file: "src/lib.rs".into(),
                path: vec![]
            }
        );
    }

    #[test]
    fn star_or_any_is_any() {
        assert_eq!(parse("*"), Locator::Any);
        assert_eq!(parse("any"), Locator::Any);
    }

    #[test]
    fn bare_word_is_name() {
        assert_eq!(parse("hello"), Locator::Name("hello".into()));
    }

    #[test]
    fn display_roundtrips() {
        let cases = vec![
            Locator::Heading(vec!["Design".into()]),
            Locator::FilePlusHeading {
                file: "src/main.rs".into(),
                path: vec!["Main".into()],
            },
            Locator::Any,
            Locator::Name("foo".into()),
        ];
        for loc in cases {
            let s = loc.to_string();
            let reparsed = parse(&s);
            assert_eq!(reparsed, loc, "roundtrip failed for {s}");
        }
    }
}
