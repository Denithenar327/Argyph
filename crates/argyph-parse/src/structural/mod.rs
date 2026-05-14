//! Structural parsers for non-code file formats (markdown, JSON, YAML, TOML, CSV).

use serde::{Deserialize, Serialize};

pub mod csv;
pub mod json;
pub mod markdown;
pub mod toml_parser;
pub mod yaml;

/// Stable identifier for a structural node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The kind of a structural node in a non-code file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    MdSection,
    MdParagraph,
    MdCodeBlock,
    MdTable,
    JsonKey,
    YamlKey,
    TomlKey,
    CsvHeader,
    CsvRow,
}

/// A structural node extracted from a non-code file (markdown, JSON, YAML, TOML, CSV).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralNode {
    pub id: NodeId,
    pub file_id: u64,
    pub kind: NodeKind,
    pub label: String,
    pub path: Vec<String>,
    /// Half-open `[start, end)` byte range in the source text.
    pub byte_range: (usize, usize),
    /// 1-based `(first_line, last_line)` inclusive.
    pub line_range: (u32, u32),
    pub parent: Option<NodeId>,
    pub depth: u32,
}

impl StructuralNode {
    /// Create a stable, repeatable `NodeId` from the triple `(file_id, kind, path)`.
    ///
    /// Uses BLAKE3 for determinism — the same inputs always produce the same ID.
    #[must_use]
    pub fn make_id(file_id: u64, kind: NodeKind, path: &[String]) -> NodeId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&file_id.to_le_bytes());
        hasher.update(&[(kind as u8)]);
        for segment in path {
            hasher.update(segment.as_bytes());
            hasher.update(&[0u8]);
        }
        let hash = hasher.finalize();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash.as_bytes()[..8]);
        NodeId(u64::from_le_bytes(bytes))
    }
}

/// Pre-compute the byte offset of the start of each line in `source`.
#[must_use]
pub(crate) fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Convert a half-open byte range `[start, end)` into a 1-based line range
/// `(first_line, last_line)`, inclusive.
///
/// `line_starts` must have been produced by [`line_starts`].
#[must_use]
pub(crate) fn byte_to_line_range(line_starts: &[usize], start: usize, end: usize) -> (u32, u32) {
    let first = match line_starts.binary_search(&start) {
        Ok(idx) => idx + 1,
        Err(idx) => idx,
    };
    let end = std::cmp::max(start, end);
    let last = match line_starts.binary_search(&end.saturating_sub(1)) {
        Ok(idx) => idx + 1,
        Err(idx) => idx,
    };
    (first as u32, last as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_id_is_stable_across_calls() {
        let a = StructuralNode::make_id(1, NodeKind::MdSection, &["top".into()]);
        let b = StructuralNode::make_id(1, NodeKind::MdSection, &["top".into()]);
        assert_eq!(a, b);
    }

    #[test]
    fn make_id_differs_across_paths() {
        let a = StructuralNode::make_id(1, NodeKind::MdSection, &["a".into()]);
        let b = StructuralNode::make_id(1, NodeKind::MdSection, &["b".into()]);
        assert_ne!(a, b);
    }
}
