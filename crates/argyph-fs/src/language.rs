/// Language detected from a file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
    JavaScript,
    Markdown,
}

impl Language {
    /// Detect language from a file extension (without the leading dot).
    ///
    /// Matching is case-sensitive and lowercased. Returns `None` for
    /// unrecognized extensions.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::TypeScript),
            "js" => Some(Self::JavaScript),
            "jsx" => Some(Self::JavaScript),
            "py" => Some(Self::Python),
            "md" => Some(Self::Markdown),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("jsx"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("md"), Some(Language::Markdown));
    }

    #[test]
    fn unknown_extension() {
        assert_eq!(Language::from_extension("toml"), None);
        assert_eq!(Language::from_extension("json"), None);
        assert_eq!(Language::from_extension(""), None);
    }
}
