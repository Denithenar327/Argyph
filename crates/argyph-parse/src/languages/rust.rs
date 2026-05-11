use crate::chunker::ast_chunks;
use crate::error::{ParseError, Result};
use crate::types::{ByteRange, ChunkKind, Import, ParsedFile, Symbol, SymbolId, SymbolKind};
use argyph_fs::{FileEntry, Language};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

static QUERY_SRC: &str = include_str!("../../queries/rust.scm");

pub fn parse_rust(file: &FileEntry, source: &str, max_chunk_size: usize) -> Result<ParsedFile> {
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();

    let mut parser = Parser::new();
    parser.set_language(&lang)?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError::Parse("tree-sitter returned None".into()))?;

    let root = tree.root_node();
    let source_bytes = source.as_bytes();

    let symbols = extract_symbols(file, &lang, &root, source_bytes)?;
    let imports = extract_imports(&root, source_bytes);
    let chunks = ast_chunks(
        &file.path,
        &root,
        source,
        Language::Rust,
        max_chunk_size,
        chunk_kind_for_node,
        is_chunk_boundary_rust,
    )?;

    Ok(ParsedFile {
        symbols,
        chunks,
        imports,
    })
}

fn extract_symbols(
    file: &FileEntry,
    lang: &tree_sitter::Language,
    root: &tree_sitter::Node,
    source: &[u8],
) -> Result<Vec<Symbol>> {
    let query = Query::new(lang, QUERY_SRC)?;
    let mut cursor = QueryCursor::new();
    let mut matches_iter = cursor.matches(&query, *root, source);
    let mut symbols = Vec::new();

    loop {
        matches_iter.advance();
        let Some(m) = matches_iter.get() else { break };

        let mut def_node: Option<tree_sitter::Node> = None;
        let mut name_node: Option<tree_sitter::Node> = None;

        for cap in m.captures {
            let cap_name = query.capture_names()[cap.index as usize];
            match cap_name {
                "def" => def_node = Some(cap.node),
                "name" => name_node = Some(cap.node),
                _ => {}
            }
        }

        let Some(def) = def_node else { continue };
        let name = name_node
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }

        let kind = match def.kind() {
            "function_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Struct,
            "enum_item" => SymbolKind::Enum,
            "trait_item" => SymbolKind::Trait,
            "impl_item" => SymbolKind::Impl,
            "mod_item" => SymbolKind::Module,
            "macro_definition" => SymbolKind::Macro,
            "const_item" => SymbolKind::Constant,
            "static_item" => SymbolKind::Static,
            "type_item" => SymbolKind::TypeAlias,
            _ => continue,
        };

        let sig = signature_node(&def, source);
        let id = SymbolId::new(&file.path, name, def.start_byte());

        symbols.push(Symbol {
            id,
            name: name.to_string(),
            kind,
            file: file.path.clone(),
            range: ByteRange::new(def.start_byte(), def.end_byte()),
            signature: sig,
            parent: None,
        });
    }

    Ok(symbols)
}

fn extract_imports(root: &tree_sitter::Node, source: &[u8]) -> Vec<Import> {
    let mut imports = Vec::new();
    collect_imports(*root, source, &mut imports);
    imports
}

fn collect_imports(node: tree_sitter::Node, source: &[u8], out: &mut Vec<Import>) {
    match node.kind() {
        "use_declaration" => {
            if let Ok(raw) = node.utf8_text(source) {
                let (mod_path, items) = parse_rust_use(raw);
                out.push(Import {
                    raw: raw.to_string(),
                    module_path: mod_path,
                    items,
                    range: ByteRange::new(node.start_byte(), node.end_byte()),
                });
            }
            return;
        }
        "extern_crate_declaration" => {
            if let Ok(raw) = node.utf8_text(source) {
                let mod_path = raw
                    .strip_prefix("extern crate ")
                    .unwrap_or("")
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                out.push(Import {
                    raw: raw.to_string(),
                    module_path: if mod_path.is_empty() {
                        vec![]
                    } else {
                        vec![mod_path]
                    },
                    items: vec![],
                    range: ByteRange::new(node.start_byte(), node.end_byte()),
                });
            }
            return;
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_imports(child, source, out);
        }
    }
}

fn parse_rust_use(raw: &str) -> (Vec<String>, Vec<String>) {
    let trimmed = raw.trim_start_matches("use ").trim_end_matches(';').trim();
    let trimmed = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
    let trimmed = trimmed.strip_prefix("crate::").unwrap_or(trimmed);
    let trimmed = trimmed.strip_prefix("self::").unwrap_or(trimmed);

    let mut mod_parts: Vec<String> = Vec::new();
    let mut items: Vec<String> = Vec::new();

    if let Some(brace_pos) = trimmed.find('{') {
        let path_part = trimmed[..brace_pos].trim();
        let items_part = &trimmed[brace_pos..];

        for segment in path_part.split("::") {
            let seg = segment.trim();
            if !seg.is_empty() {
                mod_parts.push(seg.to_string());
            }
        }

        let inner = items_part
            .trim_start_matches('{')
            .trim_end_matches('}')
            .trim();
        for item in inner.split(',') {
            let item = item.trim();
            if !item.is_empty() {
                if let Some((alias, _)) = item.split_once(" as ") {
                    items.push(alias.trim().to_string());
                } else if let Some((first, _rest)) = item.split_once("::") {
                    items.push(first.trim().to_string());
                } else {
                    items.push(item.to_string());
                }
            }
        }
    } else {
        for segment in trimmed.split("::") {
            let seg = segment.trim();
            if !seg.is_empty() {
                mod_parts.push(seg.to_string());
            }
        }
    }

    (mod_parts, items)
}

fn signature_node(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let sig_end = node
        .child_by_field_name("body")
        .map(|b| b.start_byte())
        .unwrap_or(node.end_byte());

    let sig_bytes = &source[node.start_byte()..sig_end];
    let sig = std::str::from_utf8(sig_bytes).unwrap_or("").to_string();
    let sig = sig.trim().to_string();
    if sig.is_empty() {
        None
    } else {
        Some(sig)
    }
}

fn chunk_kind_for_node(kind: &str) -> ChunkKind {
    match kind {
        "function_item" | "impl_item" => ChunkKind::FunctionBody,
        "struct_item" | "enum_item" | "trait_item" | "mod_item" | "macro_definition"
        | "const_item" | "static_item" | "type_item" => ChunkKind::TypeDef,
        _ => ChunkKind::TopLevel,
    }
}

fn is_chunk_boundary_rust(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "impl_item"
            | "mod_item"
            | "macro_definition"
            | "const_item"
            | "static_item"
            | "type_item"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::time::UNIX_EPOCH;

    fn make_file(path: &str, lang: Language) -> FileEntry {
        FileEntry {
            path: Utf8PathBuf::from(path),
            hash: argyph_fs::Blake3Hash::from([0u8; 32]),
            language: Some(lang),
            size: 0,
            modified: UNIX_EPOCH,
        }
    }

    fn count_expected(symbols: &[Symbol], expected: &[&str]) -> bool {
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        expected.iter().all(|e| names.contains(e))
    }

    #[test]
    fn parse_rust_main_fn() {
        let source = "fn main() {\n    println!(\"hello\");\n}\n";
        let file = make_file("src/main.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "main");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn parse_rust_struct_and_fn() {
        let source = r#"pub struct Foo {
    x: i32,
}

impl Foo {
    pub fn new(x: i32) -> Self {
        Self { x }
    }
}

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let file = make_file("src/lib.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert!(
            count_expected(&result.symbols, &["Foo", "new", "add"]),
            "expected Foo, new, add; got: {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn parse_rust_use_import() {
        let source = "use std::collections::HashMap;\n\nfn f() {}\n";
        let file = make_file("src/lib.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert_eq!(result.imports.len(), 1);
    }

    #[test]
    fn parse_rust_extern_crate() {
        let source = "extern crate serde;\n\nfn f() {}\n";
        let file = make_file("src/lib.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert_eq!(result.imports.len(), 1);
    }

    #[test]
    fn parse_rust_trait_and_enum() {
        let source = r#"pub trait Summary {
    fn summarize(&self) -> String;
}

pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let file = make_file("src/lib.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert!(count_expected(&result.symbols, &["Summary", "Color"]));
    }

    #[test]
    fn parse_rust_chunks_produced() {
        let source = "fn one() {}\nfn two() {}\nfn three() {}\n";
        let file = make_file("src/lib.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert!(!result.chunks.is_empty(), "should produce chunks");
    }

    #[test]
    fn all_symbols_have_valid_ranges_rust() {
        let source = r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub struct Point {
    x: f64,
    y: f64,
}
"#;
        let file = make_file("src/lib.rs", Language::Rust);
        let result = parse_rust(&file, source, 4096).unwrap();
        assert!(result.symbols.len() >= 2);
        for s in &result.symbols {
            assert!(
                s.range.end <= source.len(),
                "range {:?} exceeds source length {}",
                s.range,
                source.len()
            );
        }
    }
}
