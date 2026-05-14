#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::test_util::*;
use std::collections::HashMap;

fn standard_test_context() -> TestContext {
    let mut files = HashMap::new();
    files.insert(
        path("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}".to_string(),
    );
    files.insert(
        path("src/utils.rs"),
        "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}".to_string(),
    );
    files.insert(
        path("src/models/user.rs"),
        "pub struct User {\n    pub name: String,\n    pub age: u32,\n}".to_string(),
    );
    files.insert(
        path("README.md"),
        "# My Project\n\nA cool project.".to_string(),
    );
    files.insert(
        path("CONTRIBUTING.md"),
        "## Contributing\n\nPlease submit PRs.".to_string(),
    );
    files.insert(
        path("src/lib_test.rs"),
        "#[test]\nfn test_add() {\n    assert_eq!(add(1, 2), 3);\n}".to_string(),
    );
    files.insert(
        path("tests/integration.rs"),
        "#[test]\nfn test_integration() {\n    assert!(true);\n}".to_string(),
    );
    TestContext::with_content_files(files)
}

#[test]
fn snapshot_xml_renderer() {
    let packer = DefaultPacker::new().unwrap();
    let ctx = standard_test_context();
    let req = PackRequest {
        scope: PackScope::All,
        format: PackFormat::Xml,
        token_budget: 5000,
        include: PackInclude {
            tests: true,
            docs: true,
        },
    };
    let result = packer.pack(&req, &ctx).unwrap();
    insta::assert_snapshot!("xml_renderer", result.content);
}

#[test]
fn snapshot_markdown_renderer() {
    let packer = DefaultPacker::new().unwrap();
    let ctx = standard_test_context();
    let req = PackRequest {
        scope: PackScope::All,
        format: PackFormat::Markdown,
        token_budget: 5000,
        include: PackInclude {
            tests: true,
            docs: true,
        },
    };
    let result = packer.pack(&req, &ctx).unwrap();
    insta::assert_snapshot!("markdown_renderer", result.content);
}

#[test]
fn snapshot_truncation_behavior() {
    let packer = DefaultPacker::new().unwrap();
    let ctx = standard_test_context();
    let req = PackRequest {
        scope: PackScope::All,
        format: PackFormat::Xml,
        token_budget: 1000,
        include: PackInclude {
            tests: true,
            docs: true,
        },
    };
    let result = packer
        .pack(&req, &ctx)
        .expect("budget should cover overhead for 7 files");
    insta::assert_snapshot!("truncation_behavior", result.content);
}

#[test]
fn snapshot_test_file_exclusion() {
    let packer = DefaultPacker::new().unwrap();
    let ctx = standard_test_context();
    let req = PackRequest {
        scope: PackScope::All,
        format: PackFormat::Xml,
        token_budget: 5000,
        include: PackInclude {
            tests: false,
            docs: true,
        },
    };
    let result = packer.pack(&req, &ctx).unwrap();
    assert!(
        !result.content.contains("lib_test.rs"),
        "test files should be excluded"
    );
    assert!(
        !result.content.contains("integration.rs"),
        "test files should be excluded"
    );
    insta::assert_snapshot!("test_file_exclusion", result.content);
}

#[test]
fn snapshot_doc_file_exclusion() {
    let packer = DefaultPacker::new().unwrap();
    let ctx = standard_test_context();
    let req = PackRequest {
        scope: PackScope::All,
        format: PackFormat::Markdown,
        token_budget: 5000,
        include: PackInclude {
            tests: true,
            docs: false,
        },
    };
    let result = packer.pack(&req, &ctx).unwrap();
    assert!(
        !result.content.contains("README.md"),
        "doc files should be excluded"
    );
    assert!(
        !result.content.contains("CONTRIBUTING.md"),
        "doc files should be excluded"
    );
    insta::assert_snapshot!("doc_file_exclusion", result.content);
}
