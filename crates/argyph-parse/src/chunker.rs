use crate::error::Result;
use crate::types::{ByteRange, Chunk, ChunkId, ChunkKind};
use argyph_fs::Language;
use camino::Utf8PathBuf;
use tree_sitter::Node;

/// Build AST-aware chunks around named definition nodes.
///
/// Named constructs (functions, classes, structs, etc.) become their own
/// chunks when they fit within `max_chunk_size`. Text between constructs
/// becomes `TopLevel` chunks. Any node exceeding `max_chunk_size` is split
/// into `Fallback` character-based chunks.
pub fn ast_chunks<F, G>(
    path: &Utf8PathBuf,
    root: &Node,
    source: &str,
    language: Language,
    max_chunk_size: usize,
    kind_for_node: F,
    is_boundary: G,
) -> Result<Vec<Chunk>>
where
    F: Fn(&str) -> ChunkKind,
    G: Fn(&str) -> bool,
{
    let source_len = source.len();
    if source_len == 0 {
        return Ok(Vec::new());
    }

    let mut boundaries: Vec<(usize, usize)> = Vec::new();
    collect_boundaries(*root, &is_boundary, &mut boundaries);
    boundaries.sort_by_key(|b| b.0);

    let mut chunks = Vec::new();
    let mut cursor: usize = 0;

    for &(start, end) in &boundaries {
        if start > cursor {
            let gap_text = &source[cursor..start];
            if !gap_text.trim().is_empty() {
                for chunk in char_split(path, gap_text, cursor, language, max_chunk_size) {
                    chunks.push(chunk);
                }
            }
        }

        let node_text = &source[start..end];
        if node_text.len() <= max_chunk_size {
            let node = find_node_at(*root, start, end);
            let kind = node
                .map(|n| kind_for_node(n.kind()))
                .unwrap_or(ChunkKind::TopLevel);
            let id = ChunkId::from_text(node_text);
            chunks.push(Chunk {
                id,
                file: path.clone(),
                range: ByteRange::new(start, end),
                text: node_text.to_string(),
                kind,
                language,
            });
        } else {
            for chunk in char_split(path, node_text, start, language, max_chunk_size) {
                chunks.push(chunk);
            }
        }

        cursor = end;
    }

    if cursor < source_len {
        let remaining = &source[cursor..];
        if !remaining.trim().is_empty() {
            for chunk in char_split(path, remaining, cursor, language, max_chunk_size) {
                chunks.push(chunk);
            }
        }
    }

    Ok(chunks)
}

fn collect_boundaries<F>(node: Node, is_boundary: &F, out: &mut Vec<(usize, usize)>)
where
    F: Fn(&str) -> bool,
{
    if is_boundary(node.kind()) {
        let start = node.start_byte();
        let end = node.end_byte();
        if !out.iter().any(|&(s, e)| s <= start && e >= end) {
            out.push((start, end));
            return;
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_boundaries(child, is_boundary, out);
        }
    }
}

fn find_node_at<'a>(root: Node<'a>, start: usize, end: usize) -> Option<Node<'a>> {
    if root.start_byte() == start && root.end_byte() == end {
        return Some(root);
    }
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i as u32) {
            if child.start_byte() <= start && child.end_byte() >= end {
                if let Some(found) = find_node_at(child, start, end) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Character-based fallback chunks for text that has no AST structure or
/// for oversized nodes.
pub fn char_split(
    path: &Utf8PathBuf,
    text: &str,
    offset: usize,
    language: Language,
    max_size: usize,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut pos = 0;

    while pos < text.len() {
        let mut end = (pos + max_size).min(text.len());
        // Walk `end` back to the nearest char boundary so we never
        // slice through a multi-byte UTF-8 codepoint. Files with
        // non-ASCII content (e.g. CSVs with smart quotes or
        // identifiers in Cyrillic, CJK, etc.) used to panic here.
        while end > pos && !text.is_char_boundary(end) {
            end -= 1;
        }

        let slice_end = if end < text.len() {
            find_good_split(&text[pos..end]).unwrap_or(end - pos) + pos
        } else {
            end
        };

        let slice = &text[pos..slice_end];
        let id = ChunkId::from_text(slice);
        chunks.push(Chunk {
            id,
            file: path.clone(),
            range: ByteRange::new(offset + pos, offset + slice_end),
            text: slice.to_string(),
            kind: ChunkKind::Fallback,
            language,
        });

        pos = slice_end;
    }

    chunks
}

fn find_good_split(window: &str) -> Option<usize> {
    for (i, ch) in window.char_indices().rev() {
        if ch == '\n' && i > window.len() / 2 {
            return Some(i + 1);
        }
    }
    for (i, ch) in window.char_indices().rev() {
        if ch == ' ' && i > window.len() / 2 {
            return Some(i + 1);
        }
    }
    None
}

/// Build fallback-only chunks for files with no recognized language.
pub fn fallback_chunks(
    path: &Utf8PathBuf,
    source: &str,
    max_size: usize,
    language: Language,
) -> Vec<Chunk> {
    char_split(path, source, 0, language, max_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_split_produces_multiple_chunks() {
        let path = Utf8PathBuf::from("test.txt");
        let text = "a".repeat(5000);
        let chunks = char_split(&path, &text, 0, Language::Markdown, 1024);
        assert!(chunks.len() >= 5);
        for c in &chunks {
            assert!(c.text.len() <= 1024 + 100);
            assert_eq!(c.kind, ChunkKind::Fallback);
        }
    }

    #[test]
    fn char_split_empty_input() {
        let path = Utf8PathBuf::from("empty.txt");
        let chunks = char_split(&path, "", 0, Language::Markdown, 1024);
        assert!(chunks.is_empty());
    }

    #[test]
    fn char_split_splits_at_newline() {
        let path = Utf8PathBuf::from("test.txt");
        let text = "line one\nline two\nline three\nline four\n";
        let chunks = char_split(&path, text, 0, Language::Markdown, 20);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn chunk_id_deterministic() {
        let a = ChunkId::from_text("hello world");
        let b = ChunkId::from_text("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn chunk_id_different_for_different_text() {
        let a = ChunkId::from_text("hello world");
        let b = ChunkId::from_text("goodbye world");
        assert_ne!(a, b);
    }

    #[test]
    fn char_split_handles_multibyte_utf8_at_window_edge() {
        // Repro for the panic where `end` would land in the middle of
        // a multi-byte UTF-8 character (e.g. Cyrillic letters used in
        // ripgrep's benchsuite data files). The chunker must not
        // panic — it must walk back to a char boundary.
        let path = Utf8PathBuf::from("test.txt");
        // 1024-byte target window deliberately straddled by 'т' (2 bytes).
        let prefix = "a".repeat(1023);
        let text = format!("{prefix}тbcdefgh");
        let chunks = char_split(&path, &text, 0, Language::Markdown, 1024);
        assert!(!chunks.is_empty());
        for c in &chunks {
            // Round-tripping through &str proves every slice landed
            // on a char boundary.
            let _ = c.text.as_str();
        }
    }

    #[test]
    fn chunk_id_whitespace_normalized() {
        let a = ChunkId::from_text("hello   world");
        let b = ChunkId::from_text("hello world");
        assert_eq!(a, b);
    }
}
