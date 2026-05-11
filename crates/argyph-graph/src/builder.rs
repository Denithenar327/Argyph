use std::collections::HashMap;

use argyph_parse::{Import, ParsedFile, Symbol, SymbolKind};
use camino::Utf8PathBuf;

use crate::edge::{Confidence, Edge, EdgeKind};
use crate::resolve::{
    python::PythonResolver, rust::RustResolver, typescript::TypeScriptResolver, ImportResolver,
};

pub trait GraphBuilder {
    fn build_edges(
        &self,
        files: &[(Utf8PathBuf, ParsedFile)],
    ) -> Result<Vec<Edge>, crate::error::GraphError>;
}

pub struct DefaultGraphBuilder;

fn normalized(path: &Utf8PathBuf) -> String {
    super::resolve::normalize_path(path.as_str())
}

impl GraphBuilder for DefaultGraphBuilder {
    fn build_edges(
        &self,
        files: &[(Utf8PathBuf, ParsedFile)],
    ) -> Result<Vec<Edge>, crate::error::GraphError> {
        let mut edges = Vec::new();
        let mut all_symbols: HashMap<String, Vec<&Symbol>> = HashMap::new();

        for (file_path, parsed) in files {
            let key = normalized(file_path);
            for sym in &parsed.symbols {
                all_symbols.entry(key.clone()).or_default().push(sym);
            }
        }

        for (file_path, parsed) in files {
            if parsed.symbols.is_empty() {
                continue;
            }

            for sym in &parsed.symbols {
                edges.push(Edge {
                    from: sym.id.clone(),
                    to: sym.id.clone(),
                    kind: EdgeKind::Defines,
                    confidence: Confidence::Resolved,
                });
            }

            let file_symbols: Vec<&Symbol> = parsed.symbols.iter().collect();
            build_within_file_references(
                &file_symbols,
                &parsed.chunks,
                &parsed.symbols,
                &mut edges,
            );

            let resolver = resolver_for(file_path);
            if let Some(resolver) = resolver {
                build_import_edges(
                    file_path,
                    &parsed.imports,
                    &all_symbols,
                    &*resolver,
                    &parsed.symbols,
                    &mut edges,
                );
            }

            build_cross_file_references(file_path, &parsed.imports, &all_symbols, &mut edges);
        }

        Ok(edges)
    }
}

fn resolver_for(file_path: &Utf8PathBuf) -> Option<Box<dyn ImportResolver>> {
    let s = file_path.as_str();
    if s.ends_with(".rs") {
        Some(Box::new(RustResolver))
    } else if s.ends_with(".ts") || s.ends_with(".tsx") {
        Some(Box::new(TypeScriptResolver))
    } else if s.ends_with(".py") {
        Some(Box::new(PythonResolver))
    } else {
        None
    }
}

fn build_within_file_references(
    symbols: &[&Symbol],
    chunks: &[argyph_parse::Chunk],
    all_file_symbols: &[Symbol],
    edges: &mut Vec<Edge>,
) {
    for sym in symbols {
        if sym.kind == SymbolKind::Variable || sym.kind == SymbolKind::Constant {
            continue;
        }

        let owning_chunks: Vec<&argyph_parse::Chunk> = chunks
            .iter()
            .filter(|c| range_overlap(&sym.range, &c.range))
            .collect();

        for other in all_file_symbols {
            if other.id == sym.id {
                continue;
            }

            let mention_found = owning_chunks
                .iter()
                .any(|c| is_word_in_text(&c.text, &other.name));

            if mention_found {
                edges.push(Edge {
                    from: sym.id.clone(),
                    to: other.id.clone(),
                    kind: EdgeKind::References,
                    confidence: Confidence::Heuristic,
                });
            }

            let is_callable = matches!(other.kind, SymbolKind::Function | SymbolKind::Method);
            if is_callable {
                let call_found = owning_chunks
                    .iter()
                    .any(|c| is_call_in_text(&c.text, &other.name));

                if call_found {
                    edges.push(Edge {
                        from: sym.id.clone(),
                        to: other.id.clone(),
                        kind: EdgeKind::Calls,
                        confidence: Confidence::Heuristic,
                    });
                }
            }
        }
    }
}

fn build_import_edges(
    source_file: &Utf8PathBuf,
    imports: &[Import],
    all_symbols: &HashMap<String, Vec<&Symbol>>,
    resolver: &dyn ImportResolver,
    source_symbols: &[Symbol],
    edges: &mut Vec<Edge>,
) {
    for import in imports {
        let resolved = resolver.resolve_import(source_file, &import.module_path, &import.raw);
        let target_file = match resolved {
            Some(t) => super::resolve::normalize_path(&t.file_path),
            None => continue,
        };

        let target_symbols = all_symbols.get(&target_file);
        let Some(target_symbols) = target_symbols else {
            continue;
        };

        for source_sym in source_symbols {
            for item_name in &import.items {
                if let Some(target_sym) = target_symbols
                    .iter()
                    .find(|s| s.name.as_str() == item_name.as_str())
                {
                    edges.push(Edge {
                        from: source_sym.id.clone(),
                        to: target_sym.id.clone(),
                        kind: EdgeKind::Imports,
                        confidence: Confidence::Heuristic,
                    });

                    edges.push(Edge {
                        from: target_sym.id.clone(),
                        to: source_sym.id.clone(),
                        kind: EdgeKind::ImportedBy,
                        confidence: Confidence::Heuristic,
                    });
                }
            }
        }
    }
}

fn build_cross_file_references(
    _source_file: &Utf8PathBuf,
    _imports: &[Import],
    _all_symbols: &HashMap<String, Vec<&Symbol>>,
    _edges: &mut [Edge],
) {
}

fn range_overlap(a: &argyph_parse::ByteRange, b: &argyph_parse::ByteRange) -> bool {
    a.start < b.end && b.start < a.end
}

fn is_word_in_text(text: &str, word: &str) -> bool {
    if word.len() > text.len() {
        return false;
    }
    for (idx, _) in text.match_indices(word) {
        let before = text[..idx]
            .chars()
            .last()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after = text[idx + word.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if before && after {
            return true;
        }
    }
    false
}

fn is_call_in_text(text: &str, name: &str) -> bool {
    let pattern = format!("{name}(");
    text.contains(&pattern)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use argyph_parse::SymbolId;

    fn make_symbol(name: &str, kind: SymbolKind, file: &str, start: usize, end: usize) -> Symbol {
        use argyph_parse::ByteRange;
        Symbol {
            id: SymbolId::new(&Utf8PathBuf::from(file), name, start),
            name: name.to_string(),
            kind,
            file: Utf8PathBuf::from(file),
            range: ByteRange::new(start, end),
            signature: None,
            parent: None,
        }
    }

    fn make_chunk(text: &str, file: &str, start: usize, end: usize) -> argyph_parse::Chunk {
        use argyph_parse::{ByteRange, Chunk, ChunkId, ChunkKind};
        Chunk {
            id: ChunkId::from_text(text),
            file: Utf8PathBuf::from(file),
            range: ByteRange::new(start, end),
            text: text.to_string(),
            kind: ChunkKind::FunctionBody,
            language: argyph_fs::Language::Rust,
        }
    }

    #[test]
    fn every_symbol_gets_defines_edge() {
        let sym = make_symbol("foo", SymbolKind::Function, "src/lib.rs", 0, 10);
        let parsed = ParsedFile {
            symbols: vec![sym],
            chunks: vec![],
            imports: vec![],
        };
        let builder = DefaultGraphBuilder;
        let edges = builder
            .build_edges(&[(Utf8PathBuf::from("src/lib.rs"), parsed)])
            .expect("build_edges");

        let defines: Vec<&Edge> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].from, defines[0].to);
        assert_eq!(defines[0].confidence, Confidence::Resolved);
    }

    #[test]
    fn word_boundary_matches_identifiers() {
        assert!(is_word_in_text("let x = foo + 1", "foo"));
        assert!(!is_word_in_text("let x = foobar + 1", "foo"));
        assert!(is_word_in_text("foo()", "foo"));
        assert!(!is_word_in_text("snafoo()", "foo"));
        assert!(is_word_in_text("  foo  ", "foo"));
    }

    #[test]
    fn call_detection() {
        assert!(is_call_in_text("void foo(a, b)", "foo"));
        assert!(is_call_in_text("let x = foo(1, 2)", "foo"));
        assert!(!is_call_in_text("let x = foo", "foo"));
        assert!(!is_call_in_text("foo_bar()", "foo"));
    }

    #[test]
    fn detect_within_file_reference() {
        let sym_a = make_symbol("helper", SymbolKind::Function, "src/lib.rs", 0, 50);
        let sym_b = make_symbol("main_func", SymbolKind::Function, "src/lib.rs", 60, 200);
        let chunk = make_chunk("let x = helper(1);", "src/lib.rs", 70, 190);
        let parsed = ParsedFile {
            symbols: vec![sym_a, sym_b],
            chunks: vec![chunk],
            imports: vec![],
        };
        let builder = DefaultGraphBuilder;
        let edges = builder
            .build_edges(&[(Utf8PathBuf::from("src/lib.rs"), parsed)])
            .expect("build_edges");

        let refs: Vec<&Edge> = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::References)
            .collect();
        assert!(!refs.is_empty(), "expected at least one reference edge");
    }
}
