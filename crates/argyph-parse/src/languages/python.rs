use crate::chunker::ast_chunks;
use crate::error::{ParseError, Result};
use crate::types::{ByteRange, ChunkKind, Import, ParsedFile, Symbol, SymbolId, SymbolKind};
use argyph_fs::{FileEntry, Language};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

static QUERY_SRC: &str = include_str!("../../queries/python.scm");

pub fn parse_python(file: &FileEntry, source: &str, max_chunk_size: usize) -> Result<ParsedFile> {
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();

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
        Language::Python,
        max_chunk_size,
        chunk_kind_for_node,
        is_chunk_boundary_py,
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
            "function_definition" => {
                if is_method_py(&def) {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                }
            }
            "class_definition" => SymbolKind::Class,
            "decorated_definition" => {
                let inner = find_inner_def(&def);
                match inner.map(|n| n.kind().to_string()).as_deref() {
                    Some("class_definition") => SymbolKind::Class,
                    Some("function_definition") => {
                        if inner.is_some_and(|n| is_method_py(&n)) {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        }
                    }
                    _ => SymbolKind::Function,
                }
            }
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

fn is_method_py(node: &tree_sitter::Node) -> bool {
    node.parent()
        .is_some_and(|p| p.kind() == "block" || p.kind() == "class_body")
}

fn find_inner_def<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            match child.kind() {
                "function_definition" | "class_definition" => return Some(child),
                _ => continue,
            }
        }
    }
    None
}

fn extract_imports(root: &tree_sitter::Node, source: &[u8]) -> Vec<Import> {
    let mut imports = Vec::new();
    collect_imports(*root, source, &mut imports);
    imports
}

fn collect_imports(node: tree_sitter::Node, source: &[u8], out: &mut Vec<Import>) {
    match node.kind() {
        "import_statement" => {
            if let Ok(raw) = node.utf8_text(source) {
                let (mod_path, items) = parse_py_import(raw);
                out.push(Import {
                    raw: raw.to_string(),
                    module_path: mod_path,
                    items,
                    range: ByteRange::new(node.start_byte(), node.end_byte()),
                });
            }
            return;
        }
        "import_from_statement" => {
            if let Ok(raw) = node.utf8_text(source) {
                let (mod_path, items) = parse_py_from_import(raw);
                out.push(Import {
                    raw: raw.to_string(),
                    module_path: mod_path,
                    items,
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

fn parse_py_import(raw: &str) -> (Vec<String>, Vec<String>) {
    let trimmed = raw.trim().trim_start_matches("import ").trim();
    let mut mod_path = Vec::new();
    let mut items = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if let Some((module, alias)) = part.split_once(" as ") {
            mod_path.push(module.trim().to_string());
            items.push(alias.trim().to_string());
        } else {
            let name = part.trim();
            if !name.is_empty() {
                mod_path.push(name.to_string());
            }
        }
    }
    (mod_path, items)
}

fn parse_py_from_import(raw: &str) -> (Vec<String>, Vec<String>) {
    let trimmed = raw.trim().trim_start_matches("from ").trim();
    if let Some((module_part, items_part)) = trimmed.split_once(" import ") {
        let module_str = module_part.trim();
        let mut mod_path = Vec::new();
        for part in module_str.split('.') {
            let p = part.trim();
            if !p.is_empty() {
                mod_path.push(p.to_string());
            }
        }

        let mut items = Vec::new();
        for item in items_part.split(',') {
            let item = item.trim();
            let item = if let Some((name, _)) = item.split_once(" as ") {
                name.trim()
            } else {
                item
            };
            if !item.is_empty() {
                items.push(item.to_string());
            }
        }
        (mod_path, items)
    } else {
        (vec![], vec![])
    }
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
        "function_definition" | "decorated_definition" => ChunkKind::FunctionBody,
        "class_definition" => ChunkKind::TypeDef,
        _ => ChunkKind::TopLevel,
    }
}

fn is_chunk_boundary_py(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition" | "class_definition" | "decorated_definition"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::time::UNIX_EPOCH;

    fn make_file(path: &str) -> FileEntry {
        FileEntry {
            path: Utf8PathBuf::from(path),
            hash: argyph_fs::Blake3Hash::from([0u8; 32]),
            language: Some(Language::Python),
            size: 0,
            modified: UNIX_EPOCH,
        }
    }

    fn symbols_contain(symbols: &[Symbol], names: &[&str]) -> bool {
        let got: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        names.iter().all(|n| got.contains(n))
    }

    #[test]
    fn parse_py_function() {
        let source = "def add(a: int, b: int) -> int:\n    return a + b\n";
        let file = make_file("src/math.py");
        let result = parse_python(&file, source, 4096).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "add");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn parse_py_class_and_method() {
        let source = r#"class Greeter:
    """A friendly greeter."""

    greeting: str

    def __init__(self, message: str) -> None:
        self.greeting = message

    def greet(self, user: str) -> str:
        return f"{self.greeting}, {user}!"
"#;
        let file = make_file("src/greeter.py");
        let result = parse_python(&file, source, 4096).unwrap();
        assert!(symbols_contain(
            &result.symbols,
            &["Greeter", "__init__", "greet"]
        ));
    }

    #[test]
    fn parse_py_dataclass() {
        let source = r#"from dataclasses import dataclass

@dataclass
class User:
    name: str
    age: int
"#;
        let file = make_file("src/types.py");
        let result = parse_python(&file, source, 4096).unwrap();
        assert!(symbols_contain(&result.symbols, &["User"]));
    }

    #[test]
    fn parse_py_imports() {
        let source = r#"import os
from .math import add, multiply
from typing import List, Optional

def f(): pass
"#;
        let file = make_file("src/main.py");
        let result = parse_python(&file, source, 4096).unwrap();
        assert_eq!(result.imports.len(), 3);
    }

    #[test]
    fn parse_py_chunks_produced() {
        let source = "def a(): pass\ndef b(): pass\nclass C: pass\n";
        let file = make_file("src/app.py");
        let result = parse_python(&file, source, 4096).unwrap();
        assert!(!result.chunks.is_empty());
    }

    #[test]
    fn parse_py_enum() {
        let source = r#"from enum import Enum

class Status(Enum):
    ACTIVE = "ACTIVE"
    INACTIVE = "INACTIVE"
"#;
        let file = make_file("src/status.py");
        let result = parse_python(&file, source, 4096).unwrap();
        assert!(symbols_contain(&result.symbols, &["Status"]));
    }
}
