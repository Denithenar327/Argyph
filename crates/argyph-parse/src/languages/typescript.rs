use crate::chunker::ast_chunks;
use crate::error::{ParseError, Result};
use crate::types::{ByteRange, ChunkKind, Import, ParsedFile, Symbol, SymbolId, SymbolKind};
use argyph_fs::{FileEntry, Language};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

static QUERY_SRC: &str = include_str!("../../queries/typescript.scm");

pub fn parse_typescript(
    file: &FileEntry,
    source: &str,
    max_chunk_size: usize,
) -> Result<ParsedFile> {
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();

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
        Language::TypeScript,
        max_chunk_size,
        chunk_kind_for_node,
        is_chunk_boundary_ts,
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
            "function_declaration" | "generator_function_declaration" => {
                if is_method_ts(&def) {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                }
            }
            "method_definition" => SymbolKind::Method,
            "class_declaration" => SymbolKind::Class,
            "interface_declaration" => SymbolKind::Interface,
            "type_alias_declaration" => SymbolKind::TypeAlias,
            "enum_declaration" => SymbolKind::Enum,
            "lexical_declaration" | "export_statement" | "variable_declarator" => {
                SymbolKind::Variable
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

fn is_method_ts(node: &tree_sitter::Node) -> bool {
    node.parent().is_some_and(|p| p.kind() == "class_body")
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
                let (mod_path, items) = parse_ts_import(raw);
                out.push(Import {
                    raw: raw.to_string(),
                    module_path: mod_path,
                    items,
                    range: ByteRange::new(node.start_byte(), node.end_byte()),
                });
            }
            return;
        }
        "export_statement" => {
            if let Ok(raw) = node.utf8_text(source) {
                if raw.contains("from") {
                    let (mod_path, items) = parse_ts_import(raw);
                    out.push(Import {
                        raw: raw.to_string(),
                        module_path: mod_path,
                        items,
                        range: ByteRange::new(node.start_byte(), node.end_byte()),
                    });
                }
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

fn parse_ts_import(raw: &str) -> (Vec<String>, Vec<String>) {
    let trimmed = raw.trim_end_matches(';').trim();
    let mut mod_path = Vec::new();
    let mut items = Vec::new();

    if let Some(from_idx) = trimmed.find(" from ") {
        let before_from = &trimmed[..from_idx];
        let after_from = &trimmed[from_idx + 6..];

        let module_str = after_from
            .trim_matches(|c: char| c == '\'' || c == '"')
            .trim();
        for part in module_str.split('/') {
            let p = part.trim();
            if !p.is_empty() {
                mod_path.push(p.to_string());
            }
        }

        let specifier_str: String = if let Some(rest) = before_from.strip_prefix("import type ") {
            rest.trim().to_string()
        } else if let Some(rest) = before_from.strip_prefix("import ") {
            rest.trim().to_string()
        } else if before_from.starts_with("export") {
            let inner = before_from.trim_start_matches("export").trim();
            inner.to_string()
        } else {
            before_from.to_string()
        };

        let specifier_str = specifier_str.trim();
        let specifier_str = specifier_str.strip_prefix("type ").unwrap_or(specifier_str);
        if specifier_str.starts_with('{') {
            let inner = specifier_str
                .trim_start_matches('{')
                .trim_end_matches('}')
                .trim();
            for item in inner.split(',') {
                let item = item.trim();
                let item = item.strip_prefix("type ").unwrap_or(item);
                let item = if let Some((a, _)) = item.split_once(" as ") {
                    a.trim()
                } else {
                    item
                };
                if !item.is_empty() {
                    items.push(item.to_string());
                }
            }
        } else if !specifier_str.is_empty() && !specifier_str.starts_with('{') {
            items.push(specifier_str.to_string());
        }
    } else if let Some(rest) = trimmed.strip_prefix("import ") {
        for item in rest.split(',') {
            let item = item.trim().trim_matches(|c: char| c == '\'' || c == '"');
            if !item.is_empty() {
                mod_path.push(item.to_string());
            }
        }
    }

    (mod_path, items)
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
        "function_declaration" | "generator_function_declaration" | "method_definition" => {
            ChunkKind::FunctionBody
        }
        "class_declaration"
        | "interface_declaration"
        | "type_alias_declaration"
        | "enum_declaration" => ChunkKind::TypeDef,
        _ => ChunkKind::TopLevel,
    }
}

fn is_chunk_boundary_ts(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration"
            | "lexical_declaration"
            | "export_statement"
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
            language: Some(Language::TypeScript),
            size: 0,
            modified: UNIX_EPOCH,
        }
    }

    fn symbols_contain(symbols: &[Symbol], names: &[&str]) -> bool {
        let got: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        names.iter().all(|n| got.contains(n))
    }

    #[test]
    fn parse_ts_function() {
        let source = "export function add(a: number, b: number): number {\n  return a + b;\n}\n";
        let file = make_file("src/math.ts");
        let result = parse_typescript(&file, source, 4096).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "add");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn parse_ts_class_and_method() {
        let source = r#"export class Greeter {
    greeting: string;
    greet(user: string): string {
        return `${this.greeting}, ${user}`;
    }
}
"#;
        let file = make_file("src/greeter.ts");
        let result = parse_typescript(&file, source, 4096).unwrap();
        assert!(symbols_contain(&result.symbols, &["Greeter", "greet"]));
    }

    #[test]
    fn parse_ts_interface_and_type() {
        let source = r#"export interface User {
    name: string;
    age: number;
}

export type Role = "admin" | "user";
"#;
        let file = make_file("src/types.ts");
        let result = parse_typescript(&file, source, 4096).unwrap();
        assert!(symbols_contain(&result.symbols, &["User", "Role"]));
    }

    #[test]
    fn parse_ts_import() {
        let source = "import { add, multiply } from './math';\n\nfunction f() {}\n";
        let file = make_file("src/index.ts");
        let result = parse_typescript(&file, source, 4096).unwrap();
        assert_eq!(result.imports.len(), 1);
    }

    #[test]
    fn parse_ts_chunks_produced() {
        let source = "function a() {}\nfunction b() {}\nclass C {}\n";
        let file = make_file("src/app.ts");
        let result = parse_typescript(&file, source, 4096).unwrap();
        assert!(!result.chunks.is_empty());
    }

    #[test]
    fn parse_ts_enum() {
        let source = "export enum Status { Active, Inactive }\n";
        let file = make_file("src/status.ts");
        let result = parse_typescript(&file, source, 4096).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "Status");
    }
}
