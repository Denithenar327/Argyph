#![allow(clippy::unwrap_used, clippy::expect_used)]

use argyph_fs::{FileEntry, IgnoreWalker, Language, Walker};
use argyph_parse::{DefaultParser, Parser};
use camino::Utf8PathBuf;

fn fixture_path(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "{}/../../examples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn parse_fixture(fixture_name: &str) -> Vec<(FileEntry, argyph_parse::ParsedFile)> {
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
        results.push((entry, parsed));
    }
    results
}

fn symbols_for_lang(
    results: &[(FileEntry, argyph_parse::ParsedFile)],
    lang: Language,
) -> Vec<&argyph_parse::Symbol> {
    results
        .iter()
        .filter(|(e, _)| e.language == Some(lang))
        .flat_map(|(_, p)| &p.symbols)
        .collect()
}

#[test]
fn parse_rust_fixture() {
    let results = parse_fixture("tiny-rust-app");
    let symbols = symbols_for_lang(&results, Language::Rust);
    assert!(!symbols.is_empty(), "no Rust symbols extracted");
}

#[test]
fn rust_fixture_extracts_main_fn() {
    let results = parse_fixture("tiny-rust-app");
    let symbols = symbols_for_lang(&results, Language::Rust);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"main"),
        "expected 'main' in symbols, got: {names:?}"
    );
    assert!(
        names.contains(&"add"),
        "expected 'add' in symbols, got: {names:?}"
    );
    assert!(
        names.contains(&"tests"),
        "expected 'tests' module in symbols, got: {names:?}"
    );
}

#[test]
fn parse_typescript_fixture() {
    let results = parse_fixture("tiny-ts-app");
    let symbols = symbols_for_lang(&results, Language::TypeScript);
    assert!(!symbols.is_empty(), "no TypeScript symbols extracted");
}

#[test]
fn ts_fixture_extracts_key_symbols() {
    let results = parse_fixture("tiny-ts-app");
    let symbols = symbols_for_lang(&results, Language::TypeScript);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

    let expected = &[
        "Person",
        "Greeter",
        "greet",
        "createGreeter",
        "add",
        "multiply",
        "compute",
        "Operation",
        "User",
        "Role",
        "Status",
        "formatUser",
    ];

    let found = expected.iter().filter(|e| names.contains(e)).count();
    let pct = (found as f64 / expected.len() as f64) * 100.0;
    assert!(
        pct >= 95.0,
        "symbol coverage {pct:.1}% < 95% for TS fixture. Found: {found}/{}, Expected: {expected:?}, Got: {names:?}",
        expected.len()
    );
}

#[test]
fn parse_python_fixture() {
    let results = parse_fixture("tiny-py-app");
    let symbols = symbols_for_lang(&results, Language::Python);
    assert!(!symbols.is_empty(), "no Python symbols extracted");
}

#[test]
fn py_fixture_extracts_key_symbols() {
    let results = parse_fixture("tiny-py-app");
    let symbols = symbols_for_lang(&results, Language::Python);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

    let expected = &[
        "Greeter",
        "__init__",
        "greet",
        "create_greeter",
        "main",
        "add",
        "multiply",
        "compute",
        "User",
        "Status",
        "format_user",
    ];

    let found = expected.iter().filter(|e| names.contains(e)).count();
    let pct = (found as f64 / expected.len() as f64) * 100.0;
    assert!(
        pct >= 95.0,
        "symbol coverage {pct:.1}% < 95% for Python fixture. Found: {found}/{}, Expected: {expected:?}, Got: {names:?}",
        expected.len()
    );
}

#[test]
fn all_fixtures_produce_chunks() {
    for name in &["tiny-rust-app", "tiny-ts-app", "tiny-py-app"] {
        let results = parse_fixture(name);
        let total_chunks: usize = results.iter().map(|(_, p)| p.chunks.len()).sum();
        assert!(total_chunks > 0, "{name}: no chunks produced");
    }
}

#[test]
fn all_import_present_fixtures_produce_imports() {
    for name in &["tiny-ts-app", "tiny-py-app"] {
        let results = parse_fixture(name);
        let total_imports: usize = results.iter().map(|(_, p)| p.imports.len()).sum();
        assert!(total_imports > 0, "{name}: expected imports, found none");
    }
}

#[test]
fn fixture_chunks_have_valid_ranges() {
    let results = parse_fixture("tiny-rust-app");
    let root = fixture_path("tiny-rust-app");
    for (entry, parsed) in &results {
        let abs = root.join(&entry.path);
        let source = std::fs::read_to_string(abs.as_std_path()).unwrap();
        for chunk in &parsed.chunks {
            assert!(
                chunk.range.end <= source.len(),
                "chunk range {:?} exceeds source length {} in {}",
                chunk.range,
                source.len(),
                entry.path
            );
        }
    }
}
