#![allow(clippy::unwrap_used, clippy::expect_used)]

use argyph_fs::{FileEntry, IgnoreWalker, Walker};
use argyph_graph::{
    Confidence, DefaultGraphBuilder, EdgeKind, Graph, GraphBuilder, SymbolSelector,
};
use argyph_parse::{DefaultParser, Parser};
use camino::Utf8PathBuf;

fn fixture_path(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "{}/../../examples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn parse_fixture(fixture_name: &str) -> Vec<(Utf8PathBuf, argyph_parse::ParsedFile)> {
    let root = fixture_path(fixture_name);
    let walker = IgnoreWalker::new();
    let entries: Vec<FileEntry> = walker.walk(&root).collect();
    let parser = DefaultParser::new();

    let mut results = Vec::new();
    for entry in entries {
        let abs = root.join(&entry.path);
        let source =
            std::fs::read_to_string(abs.as_std_path()).expect("failed to read fixture file");
        let parsed = parser.parse(&entry, &source).expect("parse failed");
        results.push((root.join(&entry.path), parsed));
    }
    results
}

fn build_graph(fixture_name: &str) -> Graph {
    let files = parse_fixture(fixture_name);
    let builder = DefaultGraphBuilder;
    let edges = builder.build_edges(&files).expect("build_edges failed");
    Graph::new(edges)
}

fn source_files(fixture_name: &str) -> Vec<Utf8PathBuf> {
    parse_fixture(fixture_name)
        .into_iter()
        .filter(|(_, pf)| !pf.symbols.is_empty())
        .map(|(p, _)| p)
        .collect()
}

#[test]
fn rust_fixture_produces_defines_edges() {
    let graph = build_graph("tiny-rust-app");
    let count = graph.count_by_kind(EdgeKind::Defines);
    assert!(count > 0, "expected at least one Defines edge");
}

#[test]
fn rust_fixture_produces_reference_or_call_edges() {
    let graph = build_graph("tiny-rust-app");
    let refs = graph.count_by_kind(EdgeKind::References);
    let calls = graph.count_by_kind(EdgeKind::Calls);
    assert!(
        refs + calls > 0,
        "expected at least one References or Calls edge, got refs={refs} calls={calls}"
    );
}

#[test]
fn ts_fixture_produces_all_core_edge_types() {
    let graph = build_graph("tiny-ts-app");

    assert!(
        graph.count_by_kind(EdgeKind::Defines) > 0,
        "missing Defines"
    );
    assert!(
        graph.count_by_kind(EdgeKind::References) > 0,
        "missing References"
    );
}

#[test]
fn ts_fixture_produces_import_edges() {
    let graph = build_graph("tiny-ts-app");

    let imports = graph.count_by_kind(EdgeKind::Imports);
    let imported_by = graph.count_by_kind(EdgeKind::ImportedBy);

    assert!(
        imports > 0,
        "expected Imports edges from index.ts importing add/multiply/User"
    );
    assert_eq!(
        imports, imported_by,
        "Imports and ImportedBy should be symmetric"
    );
}

#[test]
fn ts_fixture_imports_of_returns_edges() {
    let graph = build_graph("tiny-ts-app");
    let files = parse_fixture("tiny-ts-app");

    let index_files: Vec<_> = files
        .iter()
        .filter(|(p, parsed)| p.as_str().ends_with("index.ts") && !parsed.imports.is_empty())
        .collect();

    if let Some((index_path, _)) = index_files.first() {
        let edges = graph.imports_of(index_path);
        assert!(
            !edges.is_empty(),
            "index.ts should have import edges, got 0"
        );
    }
}

#[test]
fn py_fixture_produces_all_core_edge_types() {
    let graph = build_graph("tiny-py-app");

    assert!(
        graph.count_by_kind(EdgeKind::Defines) > 0,
        "missing Defines"
    );
    assert!(
        graph.count_by_kind(EdgeKind::References) > 0,
        "missing References"
    );
}

#[test]
fn py_fixture_produces_import_edges() {
    let graph = build_graph("tiny-py-app");

    let imports = graph.count_by_kind(EdgeKind::Imports);
    let imported_by = graph.count_by_kind(EdgeKind::ImportedBy);

    assert!(
        imports > 0,
        "expected Imports edges from main.py importing add/multiply/User/Status"
    );
    assert_eq!(
        imports, imported_by,
        "Imports and ImportedBy should be symmetric"
    );
}

#[test]
fn py_fixture_produces_call_edges() {
    let graph = build_graph("tiny-py-app");
    let calls = graph.count_by_kind(EdgeKind::Calls);
    assert!(
        calls > 0,
        "expected at least one Calls edge (compute calls add/multiply, main calls greeter)"
    );
}

#[test]
fn ts_fixture_call_edges_within_math() {
    let graph = build_graph("tiny-ts-app");
    let edges = graph.edges();

    let call_edges: Vec<_> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();

    assert!(
        !call_edges.is_empty(),
        "expected Calls edges within the TS fixture (compute → add/multiply)"
    );

    for edge in &call_edges {
        assert_eq!(
            edge.confidence,
            Confidence::Heuristic,
            "call edges should be Heuristic"
        );
    }
}

#[test]
fn find_references_by_symbol_selector() {
    let graph = build_graph("tiny-ts-app");
    let edges = graph.edges();

    let ref_edge = edges
        .iter()
        .find(|e| e.kind == EdgeKind::References)
        .expect("expected at least one References edge in TS fixture");

    let results = graph.find_references(&SymbolSelector::ById(ref_edge.to.clone()));
    assert!(
        !results.is_empty(),
        "find_references by Id returned no results"
    );
}

#[test]
fn callers_and_callees_are_found() {
    let graph = build_graph("tiny-py-app");
    let edges = graph.edges();

    let call_edge = edges
        .iter()
        .find(|e| e.kind == EdgeKind::Calls)
        .expect("expected at least one Calls edge in Python fixture");

    let callers = graph.callers(&SymbolSelector::ById(call_edge.to.clone()));
    assert!(!callers.is_empty(), "callers returned no results");

    let callees = graph.callees(&SymbolSelector::ById(call_edge.from.clone()));
    assert!(!callees.is_empty(), "callees returned no results");
}

#[test]
fn imported_by_shows_reverse_imports() {
    let graph = build_graph("tiny-py-app");
    let edges = graph.edges();

    let import_edge = edges
        .iter()
        .find(|e| e.kind == EdgeKind::Imports)
        .expect("expected at least one Imports edge");

    let reverse = graph.imported_by(&SymbolSelector::ById(import_edge.to.clone()));
    assert!(!reverse.is_empty(), "imported_by returned no results");
}

#[test]
fn outlines_contain_symbols() {
    let graph = build_graph("tiny-rust-app");
    let files = source_files("tiny-rust-app");

    if let Some(path) = files.first() {
        let outline = graph.outline(path);
        assert!(
            !outline.is_empty(),
            "outline should contain symbols for {path}"
        );
    }
}

#[test]
fn find_definition_looks_up_ra_defines() {
    let graph = build_graph("tiny-rust-app");
    let defs = graph.find_definition("main", None);
    assert!(
        !defs.is_empty(),
        "find_definition('main') returned no results"
    );
}

#[test]
fn cross_file_confidence_is_heuristic() {
    let graph = build_graph("tiny-ts-app");
    let edges = graph.edges();

    let import_edges: Vec<_> = edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .collect();

    assert!(!import_edges.is_empty(), "no import edges in TS fixture");

    for edge in &import_edges {
        assert_eq!(
            edge.confidence,
            Confidence::Heuristic,
            "cross-file edges should be Heuristic, not Resolved"
        );
    }
}

#[test]
fn no_edges_claim_resolved_on_cross_file() {
    for fixture in &["tiny-rust-app", "tiny-ts-app", "tiny-py-app"] {
        let graph = build_graph(fixture);
        let edges = graph.edges();

        for edge in edges {
            if edge.kind == EdgeKind::Imports || edge.kind == EdgeKind::ImportedBy {
                assert_ne!(
                    edge.confidence,
                    Confidence::Resolved,
                    "{fixture}: {edge:?} should not be Resolved"
                );
            }
        }
    }
}
