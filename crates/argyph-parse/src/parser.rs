use crate::error::Result;
use crate::types::ParsedFile;
use argyph_fs::{FileEntry, Language};
use std::str;

/// Parses a single source file with tree-sitter, extracting symbols, AST-aware
/// chunks, and raw import statements.
pub trait Parser: Send + Sync {
    fn parse(&self, file: &FileEntry, source: &str) -> Result<ParsedFile>;
}

/// The production parser implementation.
///
/// Dispatches to the correct language-specific parser based on the file's
/// detected [`Language`]. Files with unrecognized languages receive a
/// chunk-only parse (no symbols, no imports).
pub struct DefaultParser {
    max_chunk_size: usize,
}

impl DefaultParser {
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_chunk_size: 4096,
        }
    }

    pub fn set_max_chunk_size(&mut self, size: usize) {
        self.max_chunk_size = size;
    }
}

impl Parser for DefaultParser {
    fn parse(&self, file: &FileEntry, source: &str) -> Result<ParsedFile> {
        match file.language {
            Some(Language::Rust) => crate::languages::parse_rust(file, source, self.max_chunk_size),
            Some(Language::TypeScript) => {
                crate::languages::parse_typescript(file, source, self.max_chunk_size)
            }
            Some(Language::Python) => {
                crate::languages::parse_python(file, source, self.max_chunk_size)
            }
            _ => {
                let path = file.path.clone();
                let chunks = if !source.is_empty() {
                    crate::chunker::fallback_chunks(
                        &path,
                        source,
                        self.max_chunk_size,
                        file.language.unwrap_or(Language::Markdown),
                    )
                } else {
                    Vec::new()
                };
                Ok(ParsedFile {
                    symbols: Vec::new(),
                    chunks,
                    imports: Vec::new(),
                })
            }
        }
    }
}

impl Default for DefaultParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn default_parser_construction() {
        let parser = DefaultParser::new();
        assert_eq!(parser.max_chunk_size, 4096);
    }

    #[test]
    fn set_max_chunk_size() {
        let mut parser = DefaultParser::new();
        parser.set_max_chunk_size(1024);
        assert_eq!(parser.max_chunk_size, 1024);
    }
}
