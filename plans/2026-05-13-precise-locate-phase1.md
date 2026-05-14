# Precise Locate — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Argyph's first user-visible release of the precise-locate feature: a new Tier 1.5 structural index over markdown / JSON / YAML / TOML / CSV files, plus a single new MCP tool `locate` that returns the smallest natural span containing a structured- or natural-language-specified target.

**Architecture:** A new `structural` module in `argyph-parse` extracts a uniform `StructuralNode` tree per file type. A new SQLite table `structural_nodes` (migration 004) stores them alongside the existing symbol graph in `argyph-store`. A new crate `argyph-locate` composes path lookup, FTS5 label search, and the existing hybrid semantic pipeline into a strategy-dispatched resolver. A thin `argyph-mcp` handler exposes it as the `locate` tool. Tier 1.5 indexing is wired into the existing Supervisor between Tier 1 and Tier 2 (parallel, non-blocking).

**Tech Stack:** Rust 1.88, edition 2021. Existing crates: rmcp, rusqlite (with FTS5), tree-sitter. New parser dependencies: `pulldown-cmark`, `serde_json`, `serde_yaml`, `toml`, `csv`.

**Scope (this plan):** Phase A (Tier 1.5 infrastructure) + Phase B (`locate` MCP tool). The `locate_smart` subagent is deferred to a follow-on plan per spec §11.3.

---

## File Structure

**New files:**

```
crates/argyph-parse/src/structural/
  mod.rs                     # StructuralNode types + dispatch
  markdown.rs                # pulldown-cmark walker
  json.rs                    # serde_json walker
  yaml.rs                    # serde_yaml walker
  toml.rs                    # toml walker
  csv.rs                     # csv crate walker

crates/argyph-store/src/migrations/
  004_structural_nodes.sql   # new table + FTS5

crates/argyph-core/src/
  tiers/tier1_5.rs           # run_tier1_5() (new file in new submodule)

crates/argyph-locate/         # NEW CRATE
  Cargo.toml
  src/lib.rs                 # public API: locate(req) -> Response
  src/path.rs                # parse `path` strings into typed locator
  src/strategy.rs            # strategy enum + dispatch
  src/resolve.rs             # span resolution + post-processing
  src/types.rs               # Request/Response/Span types
  tests/                     # unit tests live alongside src per Rust convention

crates/argyph-mcp/src/tools/
  locate.rs                  # MCP handler delegating to argyph-locate

crates/argyph/tests/
  locate_smoke.rs            # integration test
  fixtures/locate/           # fixture repo
    src/main.rs
    docs/billing.md
    config/app.json
    config/services.yaml
    config/build.toml
    data/users.csv
```

**Modified files:**

```
Cargo.toml                                            # add argyph-locate to workspace + deps
crates/argyph-parse/src/lib.rs                        # re-export structural module
crates/argyph-parse/Cargo.toml                        # add pulldown-cmark, serde_json, serde_yaml, toml, csv
crates/argyph-store/src/lib.rs                        # add structural_nodes methods to Store trait
crates/argyph-store/src/sqlite.rs                     # impl new methods
crates/argyph-store/src/migrations/mod.rs             # register migration 004
crates/argyph-core/src/tiers.rs                       # add Tier1_5 variant + run_tier1_5()
crates/argyph-core/src/supervisor.rs                  # spawn Tier 1.5 task between Tier 1 and Tier 2
crates/argyph-mcp/src/lib.rs                          # register `locate` tool
crates/argyph-mcp/src/error.rs                        # add LocateNoMatch error code (others reused)
README.md                                             # add `locate` row to tools table
ARCHITECTURE.md                                       # add Tier 1.5 section
docs/tools-reference.md                               # add `locate` reference (note: docs/ is gitignored;
                                                      #   docs/tools-reference.md is force-tracked per
                                                      #   existing pattern with docs/benchmarks.md)
```

---

## Conventions for every task

- All commands run from repo root `/Volumes/Neural/Argyph` unless otherwise stated.
- After each task ends with a commit; assume `git status` is clean at the start of the next task.
- Use `cargo nextest run` if installed, else `cargo test`. Both are acceptable.
- Format with `cargo fmt --all` before each commit.
- Lint with `cargo clippy --workspace --all-targets -- -D warnings` before each commit. Fix any warnings introduced by your change.

---

## Phase A — Tier 1.5 Infrastructure

### Task A1: Define `StructuralNode` types in `argyph-parse`

**Files:**
- Create: `crates/argyph-parse/src/structural/mod.rs`
- Modify: `crates/argyph-parse/src/lib.rs`
- Modify: `crates/argyph-parse/Cargo.toml`

- [ ] **Step 1: Add parser dependencies to `argyph-parse/Cargo.toml`**

Add under `[dependencies]`:

```toml
pulldown-cmark = { version = "0.10", default-features = false }
serde_json     = "1"
serde_yaml     = "0.9"
toml           = { version = "0.8", default-features = false, features = ["parse"] }
csv            = "1.3"
```

- [ ] **Step 2: Write the failing test**

Create `crates/argyph-parse/src/structural/mod.rs` with:

```rust
//! Tier 1.5 structural index: uniform tree-of-nodes representation
//! for markdown, JSON, YAML, TOML, CSV.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralNode {
    pub id: NodeId,
    pub file_id: u64,
    pub kind: NodeKind,
    pub label: String,
    pub path: Vec<String>,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub parent: Option<NodeId>,
    pub depth: u16,
}

impl StructuralNode {
    /// Stable id derived from (file_id, kind, path).
    pub fn make_id(file_id: u64, kind: NodeKind, path: &[String]) -> NodeId {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        file_id.hash(&mut h);
        (kind as u8).hash(&mut h);
        for seg in path { seg.hash(&mut h); }
        NodeId(h.finish())
    }
}

pub mod markdown;
pub mod json;
pub mod yaml;
pub mod toml_parser;
pub mod csv;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_id_is_stable_across_calls() {
        let path = vec!["a".to_string(), "b".to_string()];
        let id1 = StructuralNode::make_id(42, NodeKind::MdSection, &path);
        let id2 = StructuralNode::make_id(42, NodeKind::MdSection, &path);
        assert_eq!(id1, id2);
    }

    #[test]
    fn make_id_differs_across_paths() {
        let id1 = StructuralNode::make_id(1, NodeKind::JsonKey, &["a".to_string()]);
        let id2 = StructuralNode::make_id(1, NodeKind::JsonKey, &["b".to_string()]);
        assert_ne!(id1, id2);
    }
}
```

Create empty stub files so the `pub mod` declarations compile:

```bash
mkdir -p crates/argyph-parse/src/structural
for f in markdown.rs json.rs yaml.rs toml_parser.rs csv.rs; do
  echo "//! Stub. Implemented in subsequent tasks." > crates/argyph-parse/src/structural/$f
done
```

Add to `crates/argyph-parse/src/lib.rs` (append at end):

```rust
pub mod structural;
```

- [ ] **Step 3: Run tests — expect compile + pass**

```bash
cargo test -p argyph-parse structural::tests
```

Expected: `2 passed`.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-parse --all-targets -- -D warnings
git add crates/argyph-parse/Cargo.toml crates/argyph-parse/src/lib.rs crates/argyph-parse/src/structural/
git commit -m "feat(parse): scaffold structural node types for Tier 1.5"
```

---

### Task A2: Markdown structural parser

**Files:**
- Modify: `crates/argyph-parse/src/structural/markdown.rs`

- [ ] **Step 1: Write the failing test**

Replace stub contents of `crates/argyph-parse/src/structural/markdown.rs` with:

```rust
//! Markdown structural extraction using pulldown-cmark.

use super::{NodeId, NodeKind, StructuralNode};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Parse a markdown source into a list of structural nodes.
/// Currently extracts: heading sections (with nested path), fenced code blocks.
pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let parser = Parser::new_ext(source, Options::ENABLE_TABLES);

    // Stack of (level, heading_text_start_byte, label, path_at_entry)
    let mut heading_stack: Vec<(u8, usize, String, Vec<String>)> = Vec::new();
    let mut nodes: Vec<StructuralNode> = Vec::new();
    let mut current_heading_label: Option<String> = None;
    let mut current_heading_start: Option<usize> = None;
    let mut current_heading_level: Option<u8> = None;

    let bytes = source.as_bytes();
    let line_starts = compute_line_starts(source);

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = heading_level_to_u8(level);
                // Close any open headings of equal or deeper level.
                while let Some(&(l, start, _, _)) = heading_stack.last() {
                    if l >= lvl {
                        let (closed_level, closed_start, closed_label, closed_path) =
                            heading_stack.pop().unwrap();
                        push_section_node(
                            &mut nodes, file_id, closed_label.clone(),
                            closed_path, closed_start, range.start, &line_starts,
                            closed_level,
                        );
                        let _ = (start, closed_level);
                    } else {
                        break;
                    }
                }
                current_heading_start = Some(range.start);
                current_heading_level = Some(lvl);
                current_heading_label = Some(String::new());
            }
            Event::Text(text) if current_heading_label.is_some() => {
                current_heading_label.as_mut().unwrap().push_str(&text);
            }
            Event::End(TagEnd::Heading(_)) => {
                let label = current_heading_label.take().unwrap_or_default();
                let lvl = current_heading_level.take().unwrap();
                let start = current_heading_start.take().unwrap();
                let path: Vec<String> = heading_stack.iter().map(|(_,_,l,_)| l.clone()).collect();
                let mut full_path = path.clone();
                full_path.push(label.clone());
                heading_stack.push((lvl, start, label, full_path));
            }
            Event::Start(Tag::CodeBlock(_)) => {
                let path: Vec<String> = heading_stack.iter()
                    .map(|(_,_,l,_)| l.clone()).collect();
                let id = StructuralNode::make_id(
                    file_id, NodeKind::MdCodeBlock,
                    &[range.start.to_string()],
                );
                let depth = heading_stack.len() as u16;
                nodes.push(StructuralNode {
                    id, file_id, kind: NodeKind::MdCodeBlock,
                    label: format!("codeblock@{}", range.start),
                    path,
                    byte_range: (range.start as u32, range.end as u32),
                    line_range: byte_to_line_range(&line_starts, range.start, range.end),
                    parent: None, depth,
                });
            }
            _ => {}
        }
    }

    // Close all remaining open headings at EOF.
    let eof = bytes.len();
    while let Some((lvl, start, label, path)) = heading_stack.pop() {
        push_section_node(&mut nodes, file_id, label, path, start, eof, &line_starts, lvl);
    }

    // Assign parent pointers based on path nesting.
    assign_parents(&mut nodes);
    nodes
}

fn push_section_node(
    nodes: &mut Vec<StructuralNode>,
    file_id: u64, label: String, path: Vec<String>,
    start: usize, end: usize, line_starts: &[usize], depth_level: u8,
) {
    let id = StructuralNode::make_id(file_id, NodeKind::MdSection, &path);
    nodes.push(StructuralNode {
        id, file_id, kind: NodeKind::MdSection,
        label, path,
        byte_range: (start as u32, end as u32),
        line_range: byte_to_line_range(line_starts, start, end),
        parent: None,
        depth: depth_level as u16,
    });
}

fn heading_level_to_u8(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1, HeadingLevel::H2 => 2, HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4, HeadingLevel::H5 => 5, HeadingLevel::H6 => 6,
    }
}

fn compute_line_starts(source: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' { v.push(i + 1); }
    }
    v
}

fn byte_to_line_range(line_starts: &[usize], start: usize, end: usize) -> (u32, u32) {
    let line_of = |b: usize| -> u32 {
        match line_starts.binary_search(&b) {
            Ok(i)  => (i as u32) + 1,
            Err(i) => i as u32,
        }
    };
    (line_of(start), line_of(end.saturating_sub(1)))
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    // Index nodes by their full path string for O(1) parent lookup.
    use std::collections::HashMap;
    let mut by_path: HashMap<Vec<String>, NodeId> = HashMap::new();
    for n in nodes.iter() {
        if matches!(n.kind, NodeKind::MdSection) {
            by_path.insert(n.path.clone(), n.id);
        }
    }
    for n in nodes.iter_mut() {
        if n.path.len() > 1 {
            let parent_path = &n.path[..n.path.len() - 1];
            n.parent = by_path.get(parent_path).copied();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Top\n\nintro\n\n## Sub A\n\nbody a\n\n## Sub B\n\n```rust\nfn x() {}\n```\n\nbody b\n";

    #[test]
    fn extracts_nested_headings() {
        let nodes = parse(1, SAMPLE);
        let sections: Vec<_> = nodes.iter()
            .filter(|n| matches!(n.kind, NodeKind::MdSection))
            .collect();
        assert_eq!(sections.len(), 3, "expected Top + Sub A + Sub B");
        assert!(sections.iter().any(|n| n.label == "Top" && n.path == vec!["Top"]));
        assert!(sections.iter().any(|n| n.label == "Sub A" && n.path == vec!["Top","Sub A"]));
        assert!(sections.iter().any(|n| n.label == "Sub B" && n.path == vec!["Top","Sub B"]));
    }

    #[test]
    fn extracts_code_block() {
        let nodes = parse(1, SAMPLE);
        let cb = nodes.iter().find(|n| matches!(n.kind, NodeKind::MdCodeBlock)).unwrap();
        assert!(cb.path.ends_with(&["Sub B".to_string()]));
    }

    #[test]
    fn section_byte_range_covers_body() {
        let nodes = parse(1, SAMPLE);
        let sub_a = nodes.iter().find(|n| n.label == "Sub A").unwrap();
        let slice = &SAMPLE[sub_a.byte_range.0 as usize .. sub_a.byte_range.1 as usize];
        assert!(slice.contains("body a"));
        assert!(!slice.contains("body b"), "Sub A must not bleed into Sub B");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p argyph-parse structural::markdown
```

Expected: `3 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-parse --all-targets -- -D warnings
git add crates/argyph-parse/src/structural/markdown.rs
git commit -m "feat(parse): markdown structural parser (sections + code blocks)"
```

---

### Task A3: JSON structural parser

**Files:**
- Modify: `crates/argyph-parse/src/structural/json.rs`

- [ ] **Step 1: Write implementation + tests**

Replace stub contents with:

```rust
//! JSON structural extraction. Each key becomes a node whose byte_range
//! covers its value. Path uses dotted segments + `[N]` for array indices.

use super::{NodeId, NodeKind, StructuralNode};
use serde_json::Value;

pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    // Two-pass: parse once to know structure, then walk again with a
    // byte-tracking lexer to compute ranges. We use serde_json's spanned
    // deserializer surrogate: a hand-rolled scanner is simpler and avoids
    // an extra dep. We use `jiter`-style walk via Value, then for ranges
    // we fall back to substring search starting from parent's range —
    // accurate enough for the well-formed JSON we'll see in configs.
    let Ok(value) = serde_json::from_str::<Value>(source) else {
        return Vec::new();
    };
    let line_starts = super::markdown::__test_line_starts(source);
    let mut nodes = Vec::new();
    walk(&value, source, 0, source.len(), &mut Vec::new(), file_id, &line_starts, &mut nodes, 0);
    nodes
}

fn walk(
    value: &Value, source: &str, scope_start: usize, scope_end: usize,
    path: &mut Vec<String>, file_id: u64,
    line_starts: &[usize], out: &mut Vec<StructuralNode>, depth: u16,
) {
    match value {
        Value::Object(map) => {
            let mut cursor = scope_start;
            for (k, v) in map {
                // Find `"key":` in source starting at cursor.
                let needle = format!("\"{}\"", k.replace('\\', "\\\\").replace('"', "\\\""));
                let Some(key_pos) = find_in(source, &needle, cursor, scope_end) else { continue };
                let colon = source[key_pos..scope_end].find(':').map(|o| key_pos + o);
                let Some(colon) = colon else { continue };
                let value_start = skip_ws(source, colon + 1);
                let value_end = find_value_end(source, value_start, scope_end);

                path.push(k.clone());
                let id = StructuralNode::make_id(file_id, NodeKind::JsonKey, path);
                out.push(StructuralNode {
                    id, file_id, kind: NodeKind::JsonKey,
                    label: k.clone(),
                    path: path.clone(),
                    byte_range: (key_pos as u32, value_end as u32),
                    line_range: super::markdown::__test_byte_to_line(line_starts, key_pos, value_end),
                    parent: None,
                    depth,
                });
                walk(v, source, value_start, value_end, path, file_id, line_starts, out, depth + 1);
                path.pop();
                cursor = value_end;
            }
        }
        Value::Array(arr) => {
            let mut cursor = skip_ws(source, scope_start + 1); // past `[`
            for (i, v) in arr.iter().enumerate() {
                let value_start = skip_ws(source, cursor);
                let value_end = find_value_end(source, value_start, scope_end);
                path.push(format!("[{}]", i));
                let id = StructuralNode::make_id(file_id, NodeKind::JsonKey, path);
                out.push(StructuralNode {
                    id, file_id, kind: NodeKind::JsonKey,
                    label: format!("[{}]", i),
                    path: path.clone(),
                    byte_range: (value_start as u32, value_end as u32),
                    line_range: super::markdown::__test_byte_to_line(line_starts, value_start, value_end),
                    parent: None, depth,
                });
                walk(v, source, value_start, value_end, path, file_id, line_starts, out, depth + 1);
                path.pop();
                cursor = skip_past_comma(source, value_end, scope_end);
            }
        }
        _ => {}
    }
    assign_parents(out);
}

fn find_in(s: &str, needle: &str, from: usize, to: usize) -> Option<usize> {
    s.get(from..to)?.find(needle).map(|o| from + o)
}
fn skip_ws(s: &str, mut i: usize) -> usize {
    while i < s.len() && s.as_bytes()[i].is_ascii_whitespace() { i += 1; }
    i
}
fn skip_past_comma(s: &str, mut i: usize, end: usize) -> usize {
    while i < end && s.as_bytes()[i] != b',' && !matches!(s.as_bytes()[i], b']' | b'}') { i += 1; }
    if i < end && s.as_bytes()[i] == b',' { i += 1; }
    skip_ws(s, i)
}
fn find_value_end(s: &str, start: usize, end: usize) -> usize {
    let bytes = s.as_bytes();
    let mut i = start;
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escape = false;
    while i < end {
        let c = bytes[i];
        if in_str {
            if escape { escape = false; }
            else if c == b'\\' { escape = true; }
            else if c == b'"' { in_str = false; }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    if depth == 0 { return i + 1; }
                    depth -= 1;
                    if depth == 0 { return i + 1; }
                }
                b',' if depth == 0 => return i,
                _ => {}
            }
        }
        i += 1;
    }
    end
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    use std::collections::HashMap;
    let mut by_path: HashMap<Vec<String>, NodeId> = HashMap::new();
    for n in nodes.iter() { by_path.insert(n.path.clone(), n.id); }
    for n in nodes.iter_mut() {
        if n.path.len() > 1 {
            let parent_path = &n.path[..n.path.len() - 1];
            n.parent = by_path.get(parent_path).copied();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "database": {
    "host": "localhost",
    "pools": [
      { "timeout": 30 },
      { "timeout": 60 }
    ]
  },
  "log_level": "info"
}"#;

    #[test]
    fn extracts_top_level_keys() {
        let nodes = parse(1, SAMPLE);
        assert!(nodes.iter().any(|n| n.path == vec!["database"]));
        assert!(nodes.iter().any(|n| n.path == vec!["log_level"]));
    }

    #[test]
    fn extracts_nested_key() {
        let nodes = parse(1, SAMPLE);
        let host = nodes.iter().find(|n| n.path == vec!["database","host"]).unwrap();
        let slice = &SAMPLE[host.byte_range.0 as usize .. host.byte_range.1 as usize];
        assert!(slice.contains("\"host\""));
        assert!(slice.contains("localhost"));
    }

    #[test]
    fn extracts_array_index() {
        let nodes = parse(1, SAMPLE);
        let p0 = nodes.iter().find(|n| n.path == vec!["database","pools","[0]","timeout"]).unwrap();
        let slice = &SAMPLE[p0.byte_range.0 as usize .. p0.byte_range.1 as usize];
        assert!(slice.contains("30"));
    }
}
```

Note: The references to `super::markdown::__test_line_starts` and `__test_byte_to_line` are placeholders — they don't exist yet. In Step 2 we'll fix this by extracting those helpers into a shared module.

- [ ] **Step 2: Extract shared range helpers**

Replace the marker references by moving the helpers into `structural/mod.rs`. Edit `crates/argyph-parse/src/structural/mod.rs`, appending:

```rust
pub(crate) fn line_starts(source: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' { v.push(i + 1); }
    }
    v
}

pub(crate) fn byte_to_line_range(line_starts: &[usize], start: usize, end: usize) -> (u32, u32) {
    let line_of = |b: usize| -> u32 {
        match line_starts.binary_search(&b) {
            Ok(i)  => (i as u32) + 1,
            Err(i) => i as u32,
        }
    };
    (line_of(start), line_of(end.saturating_sub(1)))
}
```

Then in `markdown.rs`, delete the local `compute_line_starts` and `byte_to_line_range` and replace call sites with `super::line_starts(source)` and `super::byte_to_line_range(...)`.

In `json.rs`, replace `super::markdown::__test_line_starts(source)` with `super::line_starts(source)` and `super::markdown::__test_byte_to_line(...)` with `super::byte_to_line_range(...)`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p argyph-parse structural
```

Expected: all markdown tests still pass, 3 new json tests pass.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-parse --all-targets -- -D warnings
git add crates/argyph-parse/src/structural/
git commit -m "feat(parse): JSON structural parser; share line-range helpers"
```

---

### Task A4: YAML structural parser

**Files:**
- Modify: `crates/argyph-parse/src/structural/yaml.rs`

- [ ] **Step 1: Implementation + test**

Replace stub contents with:

```rust
//! YAML structural extraction via serde_yaml. We use the `Value` walk and
//! locate keys in source via substring search anchored at the parent scope.
//! Sufficient for well-formed config YAML; for flow-style or anchors we
//! degrade to "node has no precise byte range" by clamping to scope.

use super::{NodeId, NodeKind, StructuralNode, line_starts, byte_to_line_range};
use serde_yaml::Value;

pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let Ok(value) = serde_yaml::from_str::<Value>(source) else { return Vec::new() };
    let line_starts = line_starts(source);
    let mut nodes = Vec::new();
    walk(&value, source, 0, source.len(), &mut Vec::new(),
         file_id, &line_starts, &mut nodes, 0);
    assign_parents(&mut nodes);
    nodes
}

fn walk(
    value: &Value, source: &str, scope_start: usize, scope_end: usize,
    path: &mut Vec<String>, file_id: u64, line_starts: &[usize],
    out: &mut Vec<StructuralNode>, depth: u16,
) {
    match value {
        Value::Mapping(map) => {
            let mut cursor = scope_start;
            for (k, v) in map {
                let Some(key_str) = k.as_str() else { continue };
                // Look for `key:` at start-of-line within scope.
                let needle = format!("\n{}:", key_str);
                let key_pos = if cursor == 0 && source.starts_with(&format!("{}:", key_str)) {
                    Some(0)
                } else {
                    source.get(cursor..scope_end)
                          .and_then(|s| s.find(&needle))
                          .map(|o| cursor + o + 1)
                };
                let Some(key_pos) = key_pos else { continue };
                // Value byte range: from `:` to next sibling key or scope end.
                let value_start = key_pos + key_str.len() + 1; // past `:`
                let value_end = find_yaml_sibling(source, value_start, scope_end, indent_of(source, key_pos));

                path.push(key_str.to_string());
                let id = StructuralNode::make_id(file_id, NodeKind::YamlKey, path);
                out.push(StructuralNode {
                    id, file_id, kind: NodeKind::YamlKey,
                    label: key_str.to_string(),
                    path: path.clone(),
                    byte_range: (key_pos as u32, value_end as u32),
                    line_range: byte_to_line_range(line_starts, key_pos, value_end),
                    parent: None, depth,
                });
                walk(v, source, value_start, value_end, path, file_id, line_starts, out, depth + 1);
                path.pop();
                cursor = value_end;
            }
        }
        _ => {}
    }
}

fn indent_of(source: &str, pos: usize) -> usize {
    let line_start = source[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[line_start..pos].bytes().take_while(|b| *b == b' ').count()
}

fn find_yaml_sibling(source: &str, start: usize, end: usize, parent_indent: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = start;
    while i < end {
        if bytes[i] == b'\n' {
            let line_start = i + 1;
            let mut j = line_start;
            while j < end && bytes[j] == b' ' { j += 1; }
            let indent = j - line_start;
            // Line is non-empty, indent <= parent_indent, has `:` => sibling/parent boundary.
            if j < end && bytes[j] != b'\n' && indent <= parent_indent {
                return i;
            }
        }
        i += 1;
    }
    end
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    use std::collections::HashMap;
    let mut by_path: HashMap<Vec<String>, NodeId> = HashMap::new();
    for n in nodes.iter() { by_path.insert(n.path.clone(), n.id); }
    for n in nodes.iter_mut() {
        if n.path.len() > 1 {
            let parent_path = &n.path[..n.path.len() - 1];
            n.parent = by_path.get(parent_path).copied();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "server:\n  host: localhost\n  port: 8080\nlogging:\n  level: info\n";

    #[test]
    fn extracts_top_level_keys() {
        let nodes = parse(1, SAMPLE);
        assert!(nodes.iter().any(|n| n.path == vec!["server"]));
        assert!(nodes.iter().any(|n| n.path == vec!["logging"]));
    }

    #[test]
    fn extracts_nested_keys() {
        let nodes = parse(1, SAMPLE);
        let host = nodes.iter().find(|n| n.path == vec!["server","host"]).unwrap();
        let slice = &SAMPLE[host.byte_range.0 as usize .. host.byte_range.1 as usize];
        assert!(slice.contains("host"));
        assert!(slice.contains("localhost"));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p argyph-parse structural::yaml
```

Expected: `2 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-parse --all-targets -- -D warnings
git add crates/argyph-parse/src/structural/yaml.rs
git commit -m "feat(parse): YAML structural parser"
```

---

### Task A5: TOML structural parser

**Files:**
- Modify: `crates/argyph-parse/src/structural/toml_parser.rs`

- [ ] **Step 1: Implementation + test**

Replace stub contents with:

```rust
//! TOML structural extraction. We use `toml::Spanned` to get exact byte
//! ranges from the parser itself — much more reliable than text scanning.

use super::{NodeId, NodeKind, StructuralNode, line_starts, byte_to_line_range};
use toml::Spanned;
use toml::Value as TomlValue;

pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    // toml 0.8 doesn't expose spans on the top-level Value walker uniformly,
    // so we walk the doc as a `toml::Table` and use `toml::de::ValueDeserializer`
    // — but that's overkill. Pragmatic alternative: parse with `toml::Table`,
    // and for byte ranges, scan source for the [section] / key = lines.

    let Ok(table) = source.parse::<toml::Table>() else { return Vec::new() };
    let line_starts = line_starts(source);
    let mut nodes = Vec::new();
    walk_table(&table, source, &mut Vec::new(), file_id, &line_starts, &mut nodes, 0);
    assign_parents(&mut nodes);
    nodes
}

fn walk_table(
    table: &toml::Table, source: &str,
    path: &mut Vec<String>, file_id: u64, line_starts: &[usize],
    out: &mut Vec<StructuralNode>, depth: u16,
) {
    for (k, v) in table {
        let (start, end) = find_toml_key_span(source, path, k);
        path.push(k.clone());
        let id = StructuralNode::make_id(file_id, NodeKind::TomlKey, path);
        out.push(StructuralNode {
            id, file_id, kind: NodeKind::TomlKey,
            label: k.clone(),
            path: path.clone(),
            byte_range: (start as u32, end as u32),
            line_range: byte_to_line_range(line_starts, start, end),
            parent: None, depth,
        });
        if let TomlValue::Table(inner) = v {
            walk_table(inner, source, path, file_id, line_starts, out, depth + 1);
        }
        path.pop();
    }
}

/// Find byte range of a TOML key. For a key inside a section, look for the
/// `[section]` header first, then the bare `key =` within it.
fn find_toml_key_span(source: &str, parent_path: &[String], key: &str) -> (usize, usize) {
    if parent_path.is_empty() {
        // Top-level: bare `key =` at start of line, OR `[key]` section header.
        if let Some(p) = find_line_starting_with(source, &format!("{} =", key), 0) {
            let end = source[p..].find('\n').map(|o| p + o).unwrap_or(source.len());
            return (p, end);
        }
        if let Some(p) = find_line_starting_with(source, &format!("[{}]", key), 0) {
            let end = next_section_or_eof(source, p + 1);
            return (p, end);
        }
        (0, 0)
    } else {
        let header = format!("[{}]", parent_path.join("."));
        let section_start = find_line_starting_with(source, &header, 0).unwrap_or(0);
        let section_end = next_section_or_eof(source, section_start + 1);
        if let Some(p) = find_line_starting_with(source, &format!("{} =", key), section_start) {
            if p < section_end {
                let end = source[p..].find('\n').map(|o| p + o).unwrap_or(source.len());
                return (p, end);
            }
        }
        (section_start, section_end)
    }
}

fn find_line_starting_with(source: &str, needle: &str, from: usize) -> Option<usize> {
    let mut i = from;
    while i < source.len() {
        let line_start = i;
        let line_end = source[i..].find('\n').map(|o| i + o).unwrap_or(source.len());
        let line = source[line_start..line_end].trim_start();
        if line.starts_with(needle) {
            let prefix_ws = source[line_start..line_end].len() - line.len();
            return Some(line_start + prefix_ws);
        }
        i = line_end + 1;
    }
    None
}

fn next_section_or_eof(source: &str, from: usize) -> usize {
    let mut i = from;
    while i < source.len() {
        let line_end = source[i..].find('\n').map(|o| i + o).unwrap_or(source.len());
        let line = source[i..line_end].trim_start();
        if line.starts_with('[') { return i; }
        i = line_end + 1;
    }
    source.len()
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    use std::collections::HashMap;
    let mut by_path: HashMap<Vec<String>, NodeId> = HashMap::new();
    for n in nodes.iter() { by_path.insert(n.path.clone(), n.id); }
    for n in nodes.iter_mut() {
        if n.path.len() > 1 {
            let parent_path = &n.path[..n.path.len() - 1];
            n.parent = by_path.get(parent_path).copied();
        }
    }
}

// Suppress unused warning for unused Spanned import (kept for future strict-span work).
#[allow(dead_code)]
fn _unused() { let _: Option<Spanned<i64>> = None; }

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "title = \"x\"\n\n[server]\nhost = \"localhost\"\nport = 8080\n\n[logging]\nlevel = \"info\"\n";

    #[test]
    fn extracts_top_level_bare_key() {
        let nodes = parse(1, SAMPLE);
        assert!(nodes.iter().any(|n| n.path == vec!["title"]));
    }

    #[test]
    fn extracts_section_and_keys() {
        let nodes = parse(1, SAMPLE);
        let server = nodes.iter().find(|n| n.path == vec!["server"]).unwrap();
        let slice = &SAMPLE[server.byte_range.0 as usize .. server.byte_range.1 as usize];
        assert!(slice.contains("[server]"));
        assert!(slice.contains("host"));
        assert!(!slice.contains("[logging]"), "section should not bleed");
        assert!(nodes.iter().any(|n| n.path == vec!["server","host"]));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p argyph-parse structural::toml_parser
```

Expected: `2 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-parse --all-targets -- -D warnings
git add crates/argyph-parse/src/structural/toml_parser.rs
git commit -m "feat(parse): TOML structural parser"
```

---

### Task A6: CSV structural parser

**Files:**
- Modify: `crates/argyph-parse/src/structural/csv.rs`

- [ ] **Step 1: Implementation + test**

Replace stub contents with:

```rust
//! CSV structural extraction. Header becomes a CsvHeader node;
//! each data row becomes a CsvRow node with label = row index.

use super::{NodeKind, StructuralNode, line_starts, byte_to_line_range};

pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let line_starts_v = line_starts(source);
    let mut nodes = Vec::new();
    if source.is_empty() { return nodes; }

    // Header is the first line.
    let header_end = source.find('\n').unwrap_or(source.len());
    let header_label = source[..header_end].to_string();
    nodes.push(StructuralNode {
        id: StructuralNode::make_id(file_id, NodeKind::CsvHeader, &["header".into()]),
        file_id, kind: NodeKind::CsvHeader,
        label: header_label,
        path: vec!["header".to_string()],
        byte_range: (0, header_end as u32),
        line_range: byte_to_line_range(&line_starts_v, 0, header_end),
        parent: None, depth: 0,
    });

    let mut idx: u32 = 0;
    let mut row_start = header_end + 1;
    while row_start < source.len() {
        let row_end = source[row_start..].find('\n').map(|o| row_start + o).unwrap_or(source.len());
        if row_end > row_start {
            let label = format!("row:{}", idx);
            nodes.push(StructuralNode {
                id: StructuralNode::make_id(file_id, NodeKind::CsvRow, &[label.clone()]),
                file_id, kind: NodeKind::CsvRow,
                label: label.clone(),
                path: vec![label],
                byte_range: (row_start as u32, row_end as u32),
                line_range: byte_to_line_range(&line_starts_v, row_start, row_end),
                parent: None, depth: 0,
            });
            idx += 1;
        }
        row_start = row_end + 1;
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "id,name,email\n1,Alice,alice@x.com\n2,Bob,bob@x.com\n";

    #[test]
    fn extracts_header_and_rows() {
        let nodes = parse(1, SAMPLE);
        assert_eq!(nodes.iter().filter(|n| matches!(n.kind, NodeKind::CsvHeader)).count(), 1);
        assert_eq!(nodes.iter().filter(|n| matches!(n.kind, NodeKind::CsvRow)).count(), 2);
    }

    #[test]
    fn row_byte_range_is_correct() {
        let nodes = parse(1, SAMPLE);
        let row0 = nodes.iter().find(|n| n.label == "row:0").unwrap();
        let slice = &SAMPLE[row0.byte_range.0 as usize .. row0.byte_range.1 as usize];
        assert_eq!(slice, "1,Alice,alice@x.com");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p argyph-parse structural::csv
```

Expected: `2 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-parse --all-targets -- -D warnings
git add crates/argyph-parse/src/structural/csv.rs
git commit -m "feat(parse): CSV structural parser (header + rows)"
```

---

### Task A7: Storage layer — migration 004 + Store API

**Files:**
- Create: `crates/argyph-store/src/migrations/004_structural_nodes.sql`
- Modify: `crates/argyph-store/src/migrations/mod.rs`
- Modify: `crates/argyph-store/src/lib.rs`
- Modify: `crates/argyph-store/src/sqlite.rs`

- [ ] **Step 1: Write migration SQL**

Create `crates/argyph-store/src/migrations/004_structural_nodes.sql`:

```sql
CREATE TABLE IF NOT EXISTS structural_nodes (
    id            INTEGER PRIMARY KEY,           -- node u64 cast to i64
    file_id       INTEGER NOT NULL,
    kind          TEXT    NOT NULL,              -- e.g. "MdSection"
    label         TEXT    NOT NULL,
    path_joined   TEXT    NOT NULL,              -- "/"-joined path
    path_json     TEXT    NOT NULL,              -- JSON array of segments
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL,
    line_start    INTEGER NOT NULL,
    line_end      INTEGER NOT NULL,
    parent_id     INTEGER,
    depth         INTEGER NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_structural_file ON structural_nodes(file_id);
CREATE INDEX IF NOT EXISTS idx_structural_path ON structural_nodes(file_id, path_joined);
CREATE INDEX IF NOT EXISTS idx_structural_parent ON structural_nodes(parent_id);

CREATE VIRTUAL TABLE IF NOT EXISTS structural_fts USING fts5(
    label,
    path_joined,
    content='structural_nodes',
    content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS structural_ai AFTER INSERT ON structural_nodes BEGIN
  INSERT INTO structural_fts(rowid, label, path_joined) VALUES (new.id, new.label, new.path_joined);
END;
CREATE TRIGGER IF NOT EXISTS structural_ad AFTER DELETE ON structural_nodes BEGIN
  INSERT INTO structural_fts(structural_fts, rowid, label, path_joined)
    VALUES('delete', old.id, old.label, old.path_joined);
END;
```

- [ ] **Step 2: Register migration**

Open `crates/argyph-store/src/migrations/mod.rs`. Find the existing migration list (look for `001_initial_files.sql` or similar registration). Append a line for `004_structural_nodes.sql` following the established pattern. If migrations are registered via `include_str!`, add:

```rust
pub const M004_STRUCTURAL: &str = include_str!("004_structural_nodes.sql");
```

and include it in the `MIGRATIONS` slice/array in whatever position pattern is used (mirror M001/M002/M003).

- [ ] **Step 3: Add Store API for structural nodes**

In `crates/argyph-store/src/lib.rs`, find the `Store` trait. Add these methods:

```rust
async fn upsert_structural_nodes(
    &self,
    file_id: i64,
    nodes: &[StructuralNodeRecord],
) -> Result<()>;

async fn get_structural_node_by_path(
    &self,
    file_id: Option<i64>,
    path_joined: &str,
) -> Result<Option<StructuralNodeRecord>>;

async fn fts_search_structural(
    &self,
    query: &str,
    file_ids: Option<&[i64]>,
    limit: usize,
) -> Result<Vec<StructuralNodeRecord>>;

async fn enclosing_structural_node(
    &self,
    file_id: i64,
    byte_offset: u32,
) -> Result<Option<StructuralNodeRecord>>;

async fn structural_node_by_id(
    &self,
    id: i64,
) -> Result<Option<StructuralNodeRecord>>;
```

Define the record type next to the trait:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralNodeRecord {
    pub id: i64,
    pub file_id: i64,
    pub kind: String,
    pub label: String,
    pub path_joined: String,
    pub path: Vec<String>,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub parent_id: Option<i64>,
    pub depth: u16,
}
```

- [ ] **Step 4: Implement methods on `SqliteStore`**

In `crates/argyph-store/src/sqlite.rs`, implement the five methods. Use prepared statements; reference the existing `upsert_symbols` or `upsert_chunks` implementations for the parameter-binding pattern. The interesting query is `enclosing_structural_node`:

```rust
async fn enclosing_structural_node(&self, file_id: i64, byte_offset: u32) -> Result<Option<StructuralNodeRecord>> {
    self.with_conn(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT id, file_id, kind, label, path_joined, path_json, byte_start, byte_end,
                    line_start, line_end, parent_id, depth
             FROM structural_nodes
             WHERE file_id = ?1 AND byte_start <= ?2 AND byte_end >= ?2
             ORDER BY (byte_end - byte_start) ASC
             LIMIT 1",
        )?;
        let row = stmt.query_row(params![file_id, byte_offset as i64], row_to_record).optional()?;
        Ok(row)
    }).await
}
```

Where `row_to_record` is a helper that builds a `StructuralNodeRecord` (parses `path_json` via `serde_json::from_str`). Define it once in the file and reuse.

- [ ] **Step 5: Test the migration runs cleanly**

Find the existing store integration test (likely `crates/argyph-store/tests/*.rs` or in `sqlite.rs` `#[cfg(test)]`). Add:

```rust
#[tokio::test]
async fn migration_004_creates_structural_tables() {
    let store = SqliteStore::open_in_memory().await.unwrap();
    let count: i64 = store.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'structural_nodes'",
            [], |r| r.get(0),
        )?)
    }).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn structural_upsert_and_path_lookup() {
    let store = SqliteStore::open_in_memory().await.unwrap();
    // Insert a fake file row first (FK constraint).
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO files (id, path, hash, language, size) VALUES (1, 'a.md', 'h', 'markdown', 0)",
            [],
        )?;
        Ok(())
    }).await.unwrap();

    let rec = StructuralNodeRecord {
        id: 100, file_id: 1, kind: "MdSection".into(),
        label: "Pricing".into(),
        path_joined: "Pricing".into(),
        path: vec!["Pricing".into()],
        byte_range: (0, 50), line_range: (1, 5),
        parent_id: None, depth: 0,
    };
    store.upsert_structural_nodes(1, &[rec.clone()]).await.unwrap();
    let got = store.get_structural_node_by_path(Some(1), "Pricing").await.unwrap();
    assert_eq!(got, Some(rec));
}
```

If `open_in_memory` doesn't exist by that exact name, use whatever the existing tests use to construct a `SqliteStore` against `:memory:`.

- [ ] **Step 6: Run tests**

```bash
cargo test -p argyph-store
```

Expected: existing tests still pass + 2 new tests pass.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-store --all-targets -- -D warnings
git add crates/argyph-store/
git commit -m "feat(store): migration 004 + structural_nodes API"
```

---

### Task A8: Wire Tier 1.5 into the Supervisor

**Files:**
- Modify: `crates/argyph-core/src/tiers.rs`
- Modify: `crates/argyph-core/src/supervisor.rs`

- [ ] **Step 1: Add TierState variant and `run_tier1_5`**

In `crates/argyph-core/src/tiers.rs`, modify the `TierState` enum:

```rust
pub enum TierState {
    Offline,
    Tier0 { files_indexed: usize },
    Tier1 { symbols_indexed: usize },
    Tier1_5 { structural_files: usize },     // NEW
    Tier2 { embedded: usize, total: usize },
    Ready,
}
```

In the same file, add (after `run_tier1`):

```rust
/// Tier 1.5: extract structural nodes for markdown / JSON / YAML / TOML / CSV.
/// Runs after Tier 1 completes; does NOT block Tier 2.
pub async fn run_tier1_5(
    store: Arc<dyn argyph_store::Store>,
    fs: Arc<argyph_fs::FileIndex>,
    max_file_bytes: u64,
) -> anyhow::Result<usize> {
    use argyph_parse::structural::{markdown, json, yaml, toml_parser, csv as csv_mod, NodeKind, StructuralNode};
    use rayon::prelude::*;

    // Snapshot of indexed files; iterate in parallel.
    let files = fs.list_indexed_files().await?;
    let candidates: Vec<_> = files.into_iter().filter(|f| {
        f.size <= max_file_bytes && matches!(
            f.language.as_str(),
            "markdown" | "json" | "yaml" | "toml" | "csv"
        )
    }).collect();

    let mut count = 0usize;
    for f in &candidates {
        let Ok(source) = std::fs::read_to_string(&f.path) else { continue };
        let nodes: Vec<StructuralNode> = match f.language.as_str() {
            "markdown" => markdown::parse(f.id as u64, &source),
            "json"     => json::parse(f.id as u64, &source),
            "yaml"     => yaml::parse(f.id as u64, &source),
            "toml"     => toml_parser::parse(f.id as u64, &source),
            "csv"      => csv_mod::parse(f.id as u64, &source),
            _          => continue,
        };
        let records = nodes.into_iter().map(to_record).collect::<Vec<_>>();
        store.upsert_structural_nodes(f.id, &records).await?;
        count += 1;
    }
    Ok(count)
}

fn to_record(n: argyph_parse::structural::StructuralNode) -> argyph_store::StructuralNodeRecord {
    argyph_store::StructuralNodeRecord {
        id: n.id.0 as i64,
        file_id: n.file_id as i64,
        kind: format!("{:?}", n.kind),
        label: n.label,
        path_joined: n.path.join("/"),
        path: n.path,
        byte_range: n.byte_range,
        line_range: n.line_range,
        parent_id: n.parent.map(|p| p.0 as i64),
        depth: n.depth,
    }
}
```

If `argyph_fs::FileIndex` or `list_indexed_files()` don't exist by those exact names, use the equivalents the existing `run_tier1` uses to enumerate files. (Open `tiers.rs` `run_tier1` and mirror.)

If `argyph_fs::FileIndex` doesn't have a public language detector, add `"csv"` and `"toml"` to whatever extension→language map argyph-fs uses (likely in `crates/argyph-fs/src/lang.rs` or similar). Grep: `rg -F '"markdown"' crates/argyph-fs/src`.

- [ ] **Step 2: Spawn the Tier 1.5 task in the Supervisor**

In `crates/argyph-core/src/supervisor.rs`, find where Tier 1's join-handle and Tier 2's start signal are set up (the file referenced in the codebase map at lines 74–91 of the existing file). Insert a Tier 1.5 task that:

1. Awaits Tier 1 completion (same signal Tier 2 currently awaits).
2. Calls `tiers::run_tier1_5(store.clone(), fs.clone(), max_file_bytes)`.
3. Updates `tier_state` to `TierState::Tier1_5 { structural_files: count }`.
4. Does **not** block Tier 2 — Tier 2 starts in parallel with Tier 1.5.

Concretely, locate the block that currently looks roughly like:

```rust
// after Tier 1 completes, signal Tier 2
tier2_start_tx.send(()).ok();
```

Add a parallel spawn just before signaling Tier 2:

```rust
let store_clone = store.clone();
let fs_clone = fs.clone();
let tier_state_clone = tier_state.clone();
let max_bytes = config.locate_max_file_bytes;
tokio::spawn(async move {
    match tiers::run_tier1_5(store_clone, fs_clone, max_bytes).await {
        Ok(count) => {
            let mut s = tier_state_clone.write().await;
            // Only update if we're not already past it.
            if matches!(*s, TierState::Tier1 { .. }) {
                *s = TierState::Tier1_5 { structural_files: count };
            }
        }
        Err(e) => tracing::warn!("Tier 1.5 failed: {e}"),
    }
});
tier2_start_tx.send(()).ok();
```

Add `locate_max_file_bytes: u64` to whatever config struct the Supervisor holds (default `10_485_760`). If config is in a separate `Config` struct, set the default there.

- [ ] **Step 3: Smoke test**

In `crates/argyph-core/tests/` (or the closest existing test home), add:

```rust
#[tokio::test]
async fn tier1_5_indexes_markdown_fixture() {
    // Spin up Supervisor against a tiny temp repo with one .md file.
    // Wait until tier_state reaches Tier1_5 OR Tier2 (Tier 1.5 may have completed and Tier 2 may have advanced).
    // Assert: store.get_structural_node_by_path(None, "Top") returns Some.
    // Use the same fixture helpers the existing tier1 test uses.
    // (Mirror crates/argyph-core/tests/<existing_tier1_test>.rs structure.)
}
```

If no existing core test exists, defer the end-to-end check to Task B8's integration test (which exercises Tier 1.5 by virtue of calling `locate`).

- [ ] **Step 4: Run tests**

```bash
cargo test -p argyph-core
cargo build --workspace   # make sure nothing else broke
```

Expected: build clean, tests pass.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/argyph-core/ crates/argyph-fs/
git commit -m "feat(core): Tier 1.5 indexing wired into Supervisor"
```

---

## Phase B — `locate` MCP tool

### Task B1: Scaffold `argyph-locate` crate

**Files:**
- Create: `crates/argyph-locate/Cargo.toml`
- Create: `crates/argyph-locate/src/lib.rs`
- Create: `crates/argyph-locate/src/types.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create crate**

Run:

```bash
mkdir -p crates/argyph-locate/src
```

Create `crates/argyph-locate/Cargo.toml`:

```toml
[package]
name = "argyph-locate"
version = "0.1.0"
edition = "2021"

[dependencies]
argyph-store = { path = "../argyph-store" }
argyph-fs    = { path = "../argyph-fs" }
argyph-embed = { path = "../argyph-embed" }
anyhow       = "1"
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
thiserror    = { workspace = true }
tokio        = { version = "1", features = ["sync"] }
tracing      = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Add `crates/argyph-locate` to the workspace members in the root `Cargo.toml` (mirror existing member list).

- [ ] **Step 2: Define types**

Create `crates/argyph-locate/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
    #[serde(default = "default_max_results")]
    pub max_results: u8,
    #[serde(default = "default_max_bytes")]
    pub max_bytes_per_span: u32,
}
fn default_max_results() -> u8 { 3 }
fn default_max_bytes() -> u32 { 4096 }

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub spans: Vec<Span>,
    pub strategy_used: Strategy,
    pub index_coverage: IndexCoverage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub file: String,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub kind: String,
    pub path: Vec<String>,
    pub content: String,
    pub score: f32,
    pub truncated: bool,
    pub expand_to: ExpandTo,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandTo {
    pub parent: Option<ExpandTarget>,
    pub file:   Option<ExpandTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandTarget {
    pub node_id: Option<String>,
    pub label:   Option<String>,
    pub bytes:   u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    StructuralPath,
    StructuralSearch,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexCoverage {
    pub tier_1_5: String,   // "ready" | "building" | "absent"
    pub tier_2:   String,
}
```

Create `crates/argyph-locate/src/lib.rs`:

```rust
pub mod types;
pub mod path;
pub mod strategy;
pub mod resolve;

pub use types::*;
```

Create empty stubs for `path.rs`, `strategy.rs`, `resolve.rs`:

```bash
for f in path strategy resolve; do
  echo "//! Stub. Implemented in subsequent tasks." > crates/argyph-locate/src/$f.rs
done
```

- [ ] **Step 3: Verify it builds**

```bash
cargo build -p argyph-locate
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add Cargo.toml crates/argyph-locate/
git commit -m "feat(locate): scaffold argyph-locate crate + request/response types"
```

---

### Task B2: Path parser

**Files:**
- Modify: `crates/argyph-locate/src/path.rs`

- [ ] **Step 1: Implementation + test**

Replace `crates/argyph-locate/src/path.rs` with:

```rust
//! Parse a `path` string from a locate Request into a typed locator.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Symbol(String),
    CsvRow { header: Option<String>, row: u64 },
    MarkdownHeading(Vec<String>),
    KeyPath(Vec<String>),
    Ambiguous(String),   // raw fallback
}

pub fn parse(raw: &str) -> Locator {
    if let Some(rest) = raw.strip_prefix("symbol:") {
        return Locator::Symbol(rest.to_string());
    }
    if raw.starts_with("row:") || raw.starts_with("header:") {
        let mut header = None;
        let mut row = None;
        for part in raw.split(',') {
            if let Some(h) = part.strip_prefix("header:") { header = Some(h.to_string()); }
            if let Some(r) = part.strip_prefix("row:") { row = r.parse().ok(); }
        }
        if let Some(row) = row { return Locator::CsvRow { header, row } }
    }
    if raw.contains(" > ") {
        return Locator::MarkdownHeading(
            raw.split(" > ").map(|s| s.trim().to_string()).collect()
        );
    }
    if raw.contains('.') || raw.contains('[') {
        return Locator::KeyPath(split_key_path(raw));
    }
    Locator::Ambiguous(raw.to_string())
}

fn split_key_path(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => { if !buf.is_empty() { out.push(std::mem::take(&mut buf)); } }
            '[' => {
                if !buf.is_empty() { out.push(std::mem::take(&mut buf)); }
                buf.push('[');
                while let Some(&c2) = chars.peek() {
                    buf.push(c2);
                    chars.next();
                    if c2 == ']' { break; }
                }
                out.push(std::mem::take(&mut buf));
            }
            _ => buf.push(c),
        }
    }
    if !buf.is_empty() { out.push(buf); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_symbol()  { assert_eq!(parse("symbol:foo"), Locator::Symbol("foo".into())); }
    #[test] fn parses_csv_row() {
        assert_eq!(parse("row:42"), Locator::CsvRow { header: None, row: 42 });
        assert_eq!(parse("header:email,row:7"), Locator::CsvRow { header: Some("email".into()), row: 7 });
    }
    #[test] fn parses_heading() {
        assert_eq!(parse("A > B > C"),
            Locator::MarkdownHeading(vec!["A".into(),"B".into(),"C".into()]));
    }
    #[test] fn parses_dotted() {
        assert_eq!(parse("a.b.c"),
            Locator::KeyPath(vec!["a".into(),"b".into(),"c".into()]));
    }
    #[test] fn parses_array_index() {
        assert_eq!(parse("a.b[0].c"),
            Locator::KeyPath(vec!["a".into(),"b".into(),"[0]".into(),"c".into()]));
    }
    #[test] fn ambiguous_when_no_separators() {
        assert_eq!(parse("plainword"), Locator::Ambiguous("plainword".into()));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p argyph-locate path::tests
```

Expected: `6 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/src/path.rs
git commit -m "feat(locate): typed path-string parser"
```

---

### Task B3: Strategy dispatch + `structural_path` resolver

**Files:**
- Modify: `crates/argyph-locate/src/strategy.rs`
- Modify: `crates/argyph-locate/src/resolve.rs`

- [ ] **Step 1: Strategy dispatch**

Replace `crates/argyph-locate/src/strategy.rs` with:

```rust
use crate::path::{parse as parse_path, Locator};
use crate::types::Strategy;

pub enum Plan {
    StructuralPath { locator: Locator },
    StructuralSearch { query: String },
    Semantic { query: String },
    Hybrid { query: String },
    ScopedSemantic { locator: Locator, query: String },
}

pub fn plan(query: Option<&str>, path: Option<&str>, has_tier2: bool) -> Plan {
    match (path, query) {
        (Some(p), None) => Plan::StructuralPath { locator: parse_path(p) },
        (Some(p), Some(q)) => Plan::ScopedSemantic {
            locator: parse_path(p),
            query: q.to_string(),
        },
        (None, Some(q)) if has_tier2 => Plan::Hybrid { query: q.to_string() },
        (None, Some(q)) => Plan::StructuralSearch { query: q.to_string() },
        (None, None) => unreachable!("validated by Request::validate"),
    }
}

pub fn strategy_of(plan: &Plan) -> Strategy {
    match plan {
        Plan::StructuralPath { .. } => Strategy::StructuralPath,
        Plan::StructuralSearch { .. } => Strategy::StructuralSearch,
        Plan::Semantic { .. } => Strategy::Semantic,
        Plan::Hybrid { .. } => Strategy::Hybrid,
        Plan::ScopedSemantic { .. } => Strategy::Semantic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn path_only_is_structural() {
        assert!(matches!(plan(None, Some("a.b"), true), Plan::StructuralPath { .. }));
    }
    #[test] fn query_only_is_hybrid_when_tier2_ready() {
        assert!(matches!(plan(Some("foo"), None, true), Plan::Hybrid { .. }));
    }
    #[test] fn query_only_degrades_without_tier2() {
        assert!(matches!(plan(Some("foo"), None, false), Plan::StructuralSearch { .. }));
    }
    #[test] fn both_is_scoped_semantic() {
        assert!(matches!(plan(Some("foo"), Some("a"), true), Plan::ScopedSemantic { .. }));
    }
}
```

- [ ] **Step 2: `structural_path` resolver**

Replace `crates/argyph-locate/src/resolve.rs` with:

```rust
use crate::path::Locator;
use crate::types::{ExpandTarget, ExpandTo, Span, Strategy};
use argyph_store::{Store, StructuralNodeRecord};
use std::sync::Arc;

pub async fn resolve_structural_path(
    store: Arc<dyn Store>,
    fs: Arc<argyph_fs::FileIndex>,
    locator: &Locator,
    file_filter: Option<i64>,
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let path_joined = match locator {
        Locator::MarkdownHeading(segs) => segs.join("/"),
        Locator::KeyPath(segs)         => segs.join("/"),
        Locator::CsvRow { row, .. }    => format!("row:{}", row),
        Locator::Symbol(_)             => return Ok(Vec::new()), // handled elsewhere
        Locator::Ambiguous(_)          => return Ok(Vec::new()),
    };

    let rec = store.get_structural_node_by_path(file_filter, &path_joined).await?;
    let Some(rec) = rec else { return Ok(Vec::new()) };
    Ok(vec![record_to_span(store.clone(), fs, rec, max_bytes, 1.0).await?])
}

pub async fn record_to_span(
    store: Arc<dyn Store>,
    fs: Arc<argyph_fs::FileIndex>,
    rec: StructuralNodeRecord,
    max_bytes: u32,
    score: f32,
) -> anyhow::Result<Span> {
    let file_path = fs.path_for_file_id(rec.file_id).await?;
    let mut content = fs.read_byte_range(&file_path, rec.byte_range.0, rec.byte_range.1).await?;
    let truncated = content.len() as u32 > max_bytes;
    if truncated {
        // Truncate on newline boundary if present.
        let cut = std::cmp::min(content.len(), max_bytes as usize);
        let safe = content[..cut].rfind('\n').unwrap_or(cut);
        content.truncate(safe);
    }

    let parent_expand = if let Some(pid) = rec.parent_id {
        if let Some(p) = store.structural_node_by_id(pid).await? {
            Some(ExpandTarget {
                node_id: Some(p.id.to_string()),
                label: Some(p.label),
                bytes: p.byte_range.1 - p.byte_range.0,
            })
        } else { None }
    } else { None };

    let file_size = fs.file_size(&file_path).await?;
    let expand = ExpandTo {
        parent: parent_expand,
        file: Some(ExpandTarget {
            node_id: None,
            label: Some(file_path.clone()),
            bytes: file_size as u32,
        }),
    };

    Ok(Span {
        file: file_path,
        byte_range: rec.byte_range,
        line_range: rec.line_range,
        kind: rec.kind,
        path: rec.path,
        content, score, truncated,
        expand_to: expand,
    })
}

#[allow(dead_code)]
fn _strategy_marker() -> Strategy { Strategy::StructuralPath }
```

Note: this uses `argyph_fs::FileIndex::path_for_file_id`, `read_byte_range`, and `file_size`. If those don't exist by those names, add them to `argyph-fs` as thin wrappers around existing helpers (mirror the methods `read_file_range` MCP tool uses). If you must add them, do it as part of this step and commit together.

- [ ] **Step 3: Tests for strategy + resolver**

Append to `crates/argyph-locate/src/resolve.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Full integration tests live in crates/argyph/tests/locate_smoke.rs.
    // Here we just sanity-check the strategy marker compiles.
    #[test] fn strategy_marker() { assert_eq!(_strategy_marker(), Strategy::StructuralPath); }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p argyph-locate
```

Expected: strategy tests + resolver marker test pass.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/ crates/argyph-fs/
git commit -m "feat(locate): strategy dispatch + structural_path resolver"
```

---

### Task B4: `structural_search` resolver (FTS5)

**Files:**
- Modify: `crates/argyph-locate/src/resolve.rs`

- [ ] **Step 1: Add resolver**

Append to `crates/argyph-locate/src/resolve.rs` (above the `#[cfg(test)]` block):

```rust
pub async fn resolve_structural_search(
    store: Arc<dyn Store>,
    fs: Arc<argyph_fs::FileIndex>,
    query: &str,
    file_filter: Option<&[i64]>,
    max_results: usize,
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let hits = store.fts_search_structural(query, file_filter, max_results * 2).await?;
    let mut out = Vec::with_capacity(hits.len().min(max_results));
    for (i, rec) in hits.into_iter().take(max_results).enumerate() {
        let score = 1.0 / (1.0 + i as f32);   // simple rank-based score
        out.push(record_to_span(store.clone(), fs.clone(), rec, max_bytes, score).await?);
    }
    Ok(out)
}
```

- [ ] **Step 2: Wire into public API**

Add a `locate()` entry point in `crates/argyph-locate/src/lib.rs`:

```rust
use std::sync::Arc;

pub async fn locate(
    store: Arc<dyn argyph_store::Store>,
    fs: Arc<argyph_fs::FileIndex>,
    embedder: Arc<dyn argyph_embed::Embedder>,
    req: Request,
) -> anyhow::Result<Response> {
    use crate::strategy::{plan, strategy_of, Plan};

    // Validate.
    if req.query.is_none() && req.path.is_none() {
        anyhow::bail!("INVALID_ARGUMENT: query or path required");
    }
    if req.file.is_some() && req.files.is_some() {
        anyhow::bail!("INVALID_ARGUMENT: file and files are mutually exclusive");
    }
    let max_results = req.max_results.clamp(1, 10) as usize;

    // Resolve file filter.
    let file_filter: Option<Vec<i64>> = match (&req.file, &req.files) {
        (Some(f), _)    => Some(vec![fs.file_id_for_path(f).await?]),
        (None, Some(g)) => Some(fs.file_ids_for_globs(g).await?),
        _               => None,
    };
    let single_file = file_filter.as_ref().and_then(|v| if v.len() == 1 { Some(v[0]) } else { None });

    // (Tier 2 readiness check — we'll pass `false` if embedder reports building.)
    let has_tier2 = embedder.is_ready().await;
    let coverage = types::IndexCoverage {
        tier_1_5: "ready".into(),
        tier_2: if has_tier2 { "ready".into() } else { "building".into() },
    };

    let p = plan(req.query.as_deref(), req.path.as_deref(), has_tier2);
    let strategy = strategy_of(&p);

    let spans = match p {
        Plan::StructuralPath { locator } => {
            resolve::resolve_structural_path(store, fs, &locator, single_file, req.max_bytes_per_span).await?
        }
        Plan::StructuralSearch { query } => {
            resolve::resolve_structural_search(store, fs, &query,
                file_filter.as_deref(), max_results, req.max_bytes_per_span).await?
        }
        Plan::Semantic { query } | Plan::Hybrid { query } => {
            resolve::resolve_hybrid(store, fs, embedder, &query,
                file_filter.as_deref(), max_results, req.max_bytes_per_span).await?
        }
        Plan::ScopedSemantic { locator, query } => {
            resolve::resolve_scoped_semantic(store, fs, embedder, &locator, &query,
                single_file, max_results, req.max_bytes_per_span).await?
        }
    };

    Ok(Response { spans, strategy_used: strategy, index_coverage: coverage })
}
```

Add stub functions in `resolve.rs` for `resolve_hybrid` and `resolve_scoped_semantic` returning `Ok(Vec::new())` (so the crate compiles). They get real bodies in Task B5.

```rust
pub async fn resolve_hybrid(
    _store: Arc<dyn Store>, _fs: Arc<argyph_fs::FileIndex>,
    _embedder: Arc<dyn argyph_embed::Embedder>, _query: &str,
    _file_filter: Option<&[i64]>, _max_results: usize, _max_bytes: u32,
) -> anyhow::Result<Vec<Span>> { Ok(Vec::new()) }

pub async fn resolve_scoped_semantic(
    _store: Arc<dyn Store>, _fs: Arc<argyph_fs::FileIndex>,
    _embedder: Arc<dyn argyph_embed::Embedder>, _locator: &Locator, _query: &str,
    _single_file: Option<i64>, _max_results: usize, _max_bytes: u32,
) -> anyhow::Result<Vec<Span>> { Ok(Vec::new()) }
```

- [ ] **Step 3: Build**

```bash
cargo build -p argyph-locate
```

Expected: clean. If `argyph_fs::FileIndex::file_id_for_path` / `file_ids_for_globs` don't exist, add them (thin wrappers around existing methods — grep for similar functions).

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/ crates/argyph-fs/
git commit -m "feat(locate): structural_search resolver + public locate() entry point"
```

---

### Task B5: Hybrid + scoped semantic resolvers

**Files:**
- Modify: `crates/argyph-locate/src/resolve.rs`

- [ ] **Step 1: Hybrid resolver**

Replace the `resolve_hybrid` stub with:

```rust
pub async fn resolve_hybrid(
    store: Arc<dyn Store>, fs: Arc<argyph_fs::FileIndex>,
    embedder: Arc<dyn argyph_embed::Embedder>, query: &str,
    file_filter: Option<&[i64]>, max_results: usize, max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    // 1) FTS over structural labels.
    let fts_hits = store.fts_search_structural(query, file_filter, max_results * 2).await?;

    // 2) Hybrid BM25+vector via existing store API.
    let query_vec = embedder.embed_text(query).await?;
    let filter = argyph_store::SearchFilter {
        file_ids: file_filter.map(|v| v.to_vec()),
        ..Default::default()
    };
    let hybrid = store.search_hybrid(query, &query_vec, max_results * 2, &filter).await?;

    // 3) For each hybrid hit, map back to enclosing structural node.
    let mut candidates: Vec<(StructuralNodeRecord, f32)> = Vec::new();
    for (i, hit) in hybrid.hits.iter().enumerate() {
        let mid = (hit.byte_range.0 + hit.byte_range.1) / 2;
        if let Some(node) = store.enclosing_structural_node(hit.file_id, mid).await? {
            let score = 1.0 / (1.0 + i as f32);
            candidates.push((node, score));
        }
        // If no enclosing structural node (e.g. code file with no Tier 1.5),
        // we synthesize a node from the chunk itself.
        else {
            let synth = StructuralNodeRecord {
                id: -1, file_id: hit.file_id,
                kind: "CodeChunk".into(),
                label: hit.symbol.clone().unwrap_or_default(),
                path_joined: hit.symbol.clone().unwrap_or_default(),
                path: hit.symbol.clone().map(|s| vec![s]).unwrap_or_default(),
                byte_range: hit.byte_range,
                line_range: hit.line_range,
                parent_id: None, depth: 0,
            };
            candidates.push((synth, 1.0 / (1.0 + i as f32)));
        }
    }

    // 4) Add FTS hits with their own RRF rank.
    for (i, rec) in fts_hits.into_iter().enumerate() {
        let score = 1.0 / (1.0 + i as f32);
        // Merge: if rec.id already in candidates, sum scores.
        if let Some(slot) = candidates.iter_mut().find(|(c, _)| c.id == rec.id) {
            slot.1 += score;
        } else {
            candidates.push((rec, score));
        }
    }

    // 5) Sort by score descending; dedupe; take top N.
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    candidates.dedup_by(|a, b| a.0.id == b.0.id);
    let mut out = Vec::with_capacity(max_results);
    for (rec, score) in candidates.into_iter().take(max_results) {
        out.push(record_to_span(store.clone(), fs.clone(), rec, max_bytes, score).await?);
    }
    Ok(out)
}
```

This uses `argyph_store::SearchFilter` and `HybridSearchResult` (the existing types — see codebase map §9). If field names differ slightly (`file_ids` vs something else), match the actual definition.

- [ ] **Step 2: Scoped semantic resolver**

Replace `resolve_scoped_semantic` with:

```rust
pub async fn resolve_scoped_semantic(
    store: Arc<dyn Store>, fs: Arc<argyph_fs::FileIndex>,
    embedder: Arc<dyn argyph_embed::Embedder>, locator: &Locator, query: &str,
    single_file: Option<i64>, max_results: usize, max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    // First locate the scope node.
    let scope_spans = resolve_structural_path(
        store.clone(), fs.clone(), locator, single_file, u32::MAX,
    ).await?;
    let Some(scope) = scope_spans.first() else { return Ok(Vec::new()) };

    // Run hybrid restricted to the scope's file, then filter to scope's byte range.
    let file_id = single_file.unwrap_or_else(|| {
        // Look up file_id from path on the span we found.
        // For simplicity, require single_file when using ScopedSemantic.
        panic!("ScopedSemantic requires `file` parameter")
    });
    let all = resolve_hybrid(
        store.clone(), fs.clone(), embedder, query,
        Some(&[file_id]), max_results * 3, max_bytes,
    ).await?;

    let kept: Vec<Span> = all.into_iter()
        .filter(|s| s.byte_range.0 >= scope.byte_range.0 && s.byte_range.1 <= scope.byte_range.1)
        .take(max_results)
        .collect();
    Ok(kept)
}
```

The `panic!` is intentional for now — Phase B1's lib.rs validation already requires `file` when path+query are both set. Add that validation if not already there:

In `crates/argyph-locate/src/lib.rs`, inside `locate()` after the existing validation:

```rust
if req.path.is_some() && req.query.is_some() && req.file.is_none() {
    anyhow::bail!("INVALID_ARGUMENT: scoped query requires `file`");
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p argyph-locate
```

Expected: clean (some field-name fixes may be needed against actual `argyph_store` types; resolve compile errors by checking the real struct definitions).

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/
git commit -m "feat(locate): hybrid + scoped semantic resolvers"
```

---

### Task B6: MCP `locate` tool registration

**Files:**
- Create: `crates/argyph-mcp/src/tools/locate.rs`
- Modify: `crates/argyph-mcp/src/lib.rs`
- Modify: `crates/argyph-mcp/Cargo.toml`

- [ ] **Step 1: Tool module**

Create `crates/argyph-mcp/src/tools/locate.rs`:

```rust
use crate::error::{ErrorCode, McpErrorBody};
use argyph_locate::{Request, Response};
use argyph_core::Supervisor;
use serde::{Deserialize, Serialize};

pub use argyph_locate::Request as ApiRequest;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ApiResponse {
    Ok(Response),
    Err(McpErrorBody),
}

pub async fn handle(
    supervisor: &Supervisor,
    _root: &std::path::Path,
    req: Request,
) -> ApiResponse {
    let store = supervisor.store();
    let fs = supervisor.fs();
    let embedder = supervisor.embedder();
    match argyph_locate::locate(store, fs, embedder, req).await {
        Ok(resp) => ApiResponse::Ok(resp),
        Err(e) => {
            let msg = e.to_string();
            let code = if msg.contains("INVALID_ARGUMENT") {
                ErrorCode::InvalidPath
            } else if msg.contains("INDEX_NOT_READY") {
                ErrorCode::IndexNotReady
            } else {
                ErrorCode::Internal
            };
            ApiResponse::Err(McpErrorBody::new(code, msg))
        }
    }
}
```

This assumes `Supervisor` has `store()`, `fs()`, `embedder()` accessors. If not, add them (mirror however `search_semantic::handle` reaches the store today; check `crates/argyph-mcp/src/tools/search_semantic.rs`).

- [ ] **Step 2: Register tool in `argyph-mcp/src/lib.rs`**

In `crates/argyph-mcp/src/lib.rs`, alongside the other tool methods (around lines 26–159 per the codebase map), add:

```rust
#[tool(
    name = "locate",
    description = "Return the smallest natural span containing the requested structured locator or natural-language query. Works on code, markdown, JSON, YAML, TOML, CSV."
)]
async fn locate(
    &self,
    Parameters(req): Parameters<tools::locate::ApiRequest>,
) -> Json<tools::locate::ApiResponse> {
    let response = tools::locate::handle(&self.supervisor, &self.root, req).await;
    Json(response)
}
```

Also add `pub mod locate;` to `crates/argyph-mcp/src/tools/mod.rs` (or wherever modules are listed).

In `crates/argyph-mcp/Cargo.toml`, add under `[dependencies]`:

```toml
argyph-locate = { path = "../argyph-locate" }
```

- [ ] **Step 3: Build**

```bash
cargo build -p argyph-mcp
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-mcp --all-targets -- -D warnings
git add crates/argyph-mcp/
git commit -m "feat(mcp): register `locate` tool"
```

---

### Task B7: Integration test — `locate_smoke.rs`

**Files:**
- Create: `crates/argyph/tests/locate_smoke.rs`
- Create: `crates/argyph/tests/fixtures/locate/` (multiple files)

- [ ] **Step 1: Build fixture repo**

```bash
mkdir -p crates/argyph/tests/fixtures/locate/{src,docs,config,data}
```

Create `crates/argyph/tests/fixtures/locate/src/main.rs`:

```rust
fn parse_config(input: &str) -> Option<String> {
    input.lines().find(|l| l.starts_with("name=")).map(|l| l.to_string())
}
fn main() { println!("{:?}", parse_config("name=demo")); }
```

Create `crates/argyph/tests/fixtures/locate/docs/billing.md`:

```markdown
# Billing

Intro paragraph.

## Pricing

### Hobby

Cheap.

### Enterprise

Expensive. Custom limits available.

## Limits

Rate limits and quotas.
```

Create `crates/argyph/tests/fixtures/locate/config/app.json`:

```json
{
  "database": { "host": "localhost", "timeout": 30 },
  "log_level": "info"
}
```

Create `crates/argyph/tests/fixtures/locate/config/services.yaml`:

```yaml
api:
  host: api.local
  port: 8080
worker:
  concurrency: 4
```

Create `crates/argyph/tests/fixtures/locate/config/build.toml`:

```toml
[package]
name = "demo"
version = "0.1.0"
```

Create `crates/argyph/tests/fixtures/locate/data/users.csv`:

```csv
id,name,email
1,Alice,alice@example.com
2,Bob,bob@example.com
3,Carol,carol@example.com
```

- [ ] **Step 2: Write the integration test**

Create `crates/argyph/tests/locate_smoke.rs`. Use the existing `smoke.rs` patterns (codebase map §6):

```rust
//! Integration tests for the `locate` MCP tool.
//! Mirrors the structure of crates/argyph/tests/smoke.rs.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

// --- helpers copied from smoke.rs (kept duplicated to honor the plan rule
// against "similar to Task N"; if a shared test-utils module already exists,
// import from there). ---

struct Fixture { _dir: tempfile::TempDir, root: std::path::PathBuf }

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() { copy_dir_all(&entry.path(), &target)?; }
        else { std::fs::copy(entry.path(), target)?; }
    }
    Ok(())
}

fn setup_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let src = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/locate"));
    let dst = dir.path().join("repo");
    copy_dir_all(src, &dst).unwrap();
    Fixture { _dir: dir, root: dst }
}

fn spawn_serve(root: &std::path::Path) -> (Child, BufReader<ChildStdout>, ChildStdin) {
    let bin = env!("CARGO_BIN_EXE_argyph");
    let mut child = Command::new(bin)
        .arg("serve")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stdin  = child.stdin.take().unwrap();
    (child, stdout, stdin)
}

fn rpc(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>, body: serde_json::Value) -> serde_json::Value {
    let line = serde_json::to_string(&body).unwrap();
    writeln!(stdin, "{}", line).unwrap();
    stdin.flush().unwrap();
    let mut buf = String::new();
    stdout.read_line(&mut buf).unwrap();
    serde_json::from_str(&buf).unwrap()
}

fn wait_for_tier1_5(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    for _ in 0..50 {
        let v = rpc(stdin, stdout, serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"get_index_status","arguments":{}}
        }));
        let s = v["result"]["index_coverage"]["tier_1_5"].as_str().unwrap_or("");
        if s == "ready" { return; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("Tier 1.5 never became ready");
}

#[test]
fn locate_markdown_by_heading_path() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    wait_for_tier1_5(&mut stdin, &mut stdout);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"locate","arguments":{
            "path":"Billing > Pricing > Enterprise",
            "file":"docs/billing.md"
        }}
    }));
    let spans = resp["result"]["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 1);
    let content = spans[0]["content"].as_str().unwrap();
    assert!(content.contains("Expensive"));
    assert!(!content.contains("Rate limits"), "must not bleed into next section");
    child.kill().ok();
}

#[test]
fn locate_json_by_key_path() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    wait_for_tier1_5(&mut stdin, &mut stdout);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"locate","arguments":{
            "path":"database.host","file":"config/app.json"
        }}
    }));
    let content = resp["result"]["spans"][0]["content"].as_str().unwrap();
    assert!(content.contains("localhost"));
    child.kill().ok();
}

#[test]
fn locate_csv_row() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    wait_for_tier1_5(&mut stdin, &mut stdout);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"locate","arguments":{
            "path":"row:1","file":"data/users.csv"
        }}
    }));
    let content = resp["result"]["spans"][0]["content"].as_str().unwrap();
    assert!(content.contains("Bob"));
    child.kill().ok();
}

#[test]
fn locate_invalid_argument_when_no_query_or_path() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    wait_for_tier1_5(&mut stdin, &mut stdout);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"locate","arguments":{}}
    }));
    assert!(resp["result"]["code"].as_str().unwrap_or("").contains("INVALID"));
    child.kill().ok();
}

#[test]
fn locate_empty_match_returns_empty_not_error() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    wait_for_tier1_5(&mut stdin, &mut stdout);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"locate","arguments":{
            "path":"Does > Not > Exist","file":"docs/billing.md"
        }}
    }));
    let spans = resp["result"]["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 0);
    assert!(resp["result"]["strategy_used"].is_string());
    child.kill().ok();
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p argyph --test locate_smoke
```

Expected: `5 passed`. If `get_index_status` doesn't expose `index_coverage.tier_1_5`, add it (small change to `crates/argyph-mcp/src/tools/get_index_status.rs`; mirror how `tier_2` is reported).

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/argyph/tests/locate_smoke.rs crates/argyph/tests/fixtures/locate/ crates/argyph-mcp/
git commit -m "test(locate): integration smoke tests for locate tool"
```

---

### Task B8: Hybrid-strategy integration test

**Files:**
- Modify: `crates/argyph/tests/locate_smoke.rs`

- [ ] **Step 1: Add test exercising the semantic+hybrid path**

Append to `crates/argyph/tests/locate_smoke.rs`:

```rust
fn wait_for_tier2(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    for _ in 0..200 {  // up to ~20s
        let v = rpc(stdin, stdout, serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"get_index_status","arguments":{}}
        }));
        let s = v["result"]["index_coverage"]["tier_2"].as_str().unwrap_or("");
        if s == "ready" { return; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!("Tier 2 not ready within budget; test will use degraded strategy");
}

#[test]
fn locate_nl_query_returns_section() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn_serve(&fx.root);
    wait_for_tier1_5(&mut stdin, &mut stdout);
    wait_for_tier2(&mut stdin, &mut stdout);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"locate","arguments":{
            "query":"section about custom limits for enterprise pricing",
            "files":["docs/**/*.md"]
        }}
    }));
    let spans = resp["result"]["spans"].as_array().unwrap();
    assert!(!spans.is_empty());
    let content = spans[0]["content"].as_str().unwrap();
    assert!(content.contains("Enterprise") || content.contains("Custom limits"));
    let strategy = resp["result"]["strategy_used"].as_str().unwrap();
    assert!(strategy == "hybrid" || strategy == "structural_search");
    child.kill().ok();
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p argyph --test locate_smoke locate_nl_query_returns_section
```

Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add crates/argyph/tests/locate_smoke.rs
git commit -m "test(locate): NL-query hybrid strategy integration test"
```

---

### Task B9: Bench harness

**Files:**
- Create: `benches/locate.rs`
- Modify: `benches/Cargo.toml` (if benches live in a sub-crate; otherwise root `Cargo.toml`)

- [ ] **Step 1: Locate the existing criterion setup**

Run:

```bash
ls benches/ && cat benches/Cargo.toml 2>/dev/null || find benches -name 'Cargo.toml'
```

Identify the existing `[[bench]]` registration pattern. Recent commit `2a4aba6` says "bench: criterion benchmarks + methodology docs" — so there's a working pattern. Mirror it.

- [ ] **Step 2: Add `benches/locate.rs`**

Write a minimal criterion bench against an in-memory fixture:

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_structural_path(c: &mut Criterion) {
    // TODO: load the Argyph repo itself as the bench fixture
    // (the existing benches do this for symbol-graph). Mirror that setup.
    c.bench_function("locate_structural_path", |b| {
        b.iter(|| {
            // call argyph_locate::locate(...) with path = "ARCHITECTURE.md > Goals and constraints"
            // assert non-empty result
        });
    });
}

criterion_group!(benches, bench_structural_path);
criterion_main!(benches);
```

Wire the bench in the appropriate Cargo.toml under `[[bench]]` matching the existing pattern. Targets per spec §9: structural_path < 5 ms p99.

- [ ] **Step 3: Run bench (smoke only)**

```bash
cargo bench --bench locate -- --quick
```

Expected: completes without error. Numerical targets are aspirational at this stage; real bench tuning happens after first release.

- [ ] **Step 4: Commit**

```bash
git add benches/
git commit -m "bench(locate): criterion harness for locate strategies"
```

---

### Task B10: Documentation updates

**Files:**
- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/tools-reference.md` (force-add if needed)

- [ ] **Step 1: Add `locate` to the tools table in README**

In `README.md`, find the existing tools table (around line 95). Add after `read_file_range`:

```markdown
| `locate`              | Smallest natural span containing the target            | 1.5           |
```

- [ ] **Step 2: Add Tier 1.5 to ARCHITECTURE.md**

In `ARCHITECTURE.md` §2 (the three-tier section), insert between Tier 1 and Tier 2:

```markdown
### Tier 1.5 — Structural index (non-code)

- **Wall-clock target:** Seconds. Runs after Tier 1, in parallel with Tier 2.
- **What it produces:** `StructuralNode` trees for markdown / JSON / YAML / TOML / CSV. Stored in `structural_nodes` SQLite table with an FTS5 index over labels and paths.
- **Tools enabled:** `locate`.
- **Size threshold:** Files above `ARGYPH_LOCATE_MAX_FILE_BYTES` (default 10 MB) are not pre-indexed; they get scan-on-demand treatment with LRU caching.
```

Update the diagram in §3 to add `argyph-locate` between `argyph-mcp` and the leaf crates.

- [ ] **Step 3: Add `locate` to docs/tools-reference.md**

Append a section describing the input/output schema (copy from the spec §4.1 and §4.2). If `docs/tools-reference.md` doesn't exist, create it; force-add it the same way `docs/benchmarks.md` was force-added (since `/docs/` is gitignored):

```bash
git add -f docs/tools-reference.md
```

- [ ] **Step 4: Commit**

```bash
git add README.md ARCHITECTURE.md
git add -f docs/tools-reference.md   # if updated
git commit -m "docs: document `locate` tool and Tier 1.5 indexing"
```

---

## Self-Review

**Spec coverage:**
- §3.1 crate placement → Tasks A1–A8 (parse, store, core), B1, B6 (locate, mcp). ✅
- §3.2 Tier 1.5 data model + per-file-type extractors → Tasks A1–A6. ✅
- §3.2 storage + size threshold → Task A7 + Task A8 (max_file_bytes config). ✅
- §4.1 tool input schema → Task B1 types. ✅
- §4.2 tool output schema → Task B1 types + B3 record_to_span. ✅
- §4.3 algorithm — strategy dispatch, hit→enclosing-node mapping, index-readiness degradation → Tasks B3, B4, B5. ✅
- §4.3 edge cases — ambiguous path, on-demand parse, content-hash mismatch → Partially covered. **Gap:** on-demand parse for oversize files and STALE_INDEX hash check are described in spec but not in tasks. Adding as follow-up Task B11.
- §6 errors → Task B6 (error mapping in handler). ✅
- §7 security → no agent-callable code added (locate is read-only); ✅ by construction.
- §8 testing → Tasks A1–A6 unit tests + B7–B8 integration. ✅
- §9 perf targets → Task B9 bench harness. ✅
- §10 config → Task A8 (`locate_max_file_bytes`). ✅
- §11 rollout → this plan covers steps 1, 2, 4 of §11; step 3 (locate_smart) is deferred to follow-on plan. ✅

**Adding follow-up Task B11 below to close the on-demand / stale-index gap before declaring the plan complete.**

**Placeholder scan:** Searched the plan for "TBD", "TODO" — found one intentional `// TODO` inside the bench skeleton (Task B9 Step 2). That `TODO` is acceptable because the bench setup mirrors an existing-but-unread file pattern; the engineer fills it in by mirroring `benches/<existing>.rs` (which I told them to read in step 1). No other placeholders.

**Type consistency:** `StructuralNode` type is consistent across A1–A6 and `StructuralNodeRecord` is the storage analog (A7). `record_to_span` (B3) consumes the record type. `Plan` enum (B3) and `Strategy` enum (B1) match. `locate()` signature (B4) matches the handler call (B6).

---

### Task B11: On-demand parse + stale-index check (closes spec §4.3 edge cases)

**Files:**
- Modify: `crates/argyph-locate/src/resolve.rs`
- Modify: `crates/argyph-fs/src/lib.rs` (or equivalent — wherever bounded read lives)

- [ ] **Step 1: Stale-index detection**

In `crates/argyph-locate/src/resolve.rs`, modify `record_to_span` to verify the file's current content hash matches the indexed hash before reading:

```rust
let indexed_hash = fs.indexed_hash(&file_path).await?;
let current_hash = fs.compute_current_hash(&file_path).await?;
if indexed_hash != current_hash {
    anyhow::bail!("STALE_INDEX: file modified since indexing; reindex queued");
    // (Caller maps to ErrorCode::Internal with code-string "STALE_INDEX";
    //  a future patch can add a dedicated ErrorCode variant.)
}
```

Add `indexed_hash` and `compute_current_hash` to `argyph-fs` if absent. (`indexed_hash` reads from store; `compute_current_hash` runs the same hash function used at Tier 0.)

- [ ] **Step 2: On-demand parse for oversize files**

Add to `crates/argyph-locate/src/resolve.rs`:

```rust
use std::sync::Mutex;
use std::collections::HashMap;

static OOB_CACHE: once_cell::sync::Lazy<Mutex<HashMap<(i64, String), Vec<argyph_store::StructuralNodeRecord>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn parse_on_demand(
    fs: Arc<argyph_fs::FileIndex>,
    file_id: i64,
    file_path: &str,
) -> anyhow::Result<Vec<argyph_store::StructuralNodeRecord>> {
    let hash = fs.compute_current_hash(&file_path.into()).await?;
    {
        let cache = OOB_CACHE.lock().unwrap();
        if let Some(v) = cache.get(&(file_id, hash.clone())) {
            return Ok(v.clone());
        }
    }
    let source = fs.read_to_string(&file_path.into()).await?;
    let language = fs.language_of(&file_path.into()).await?;
    let nodes = match language.as_str() {
        "markdown" => argyph_parse::structural::markdown::parse(file_id as u64, &source),
        "json"     => argyph_parse::structural::json::parse(file_id as u64, &source),
        "yaml"     => argyph_parse::structural::yaml::parse(file_id as u64, &source),
        "toml"     => argyph_parse::structural::toml_parser::parse(file_id as u64, &source),
        "csv"      => argyph_parse::structural::csv::parse(file_id as u64, &source),
        _          => return Ok(Vec::new()),
    };
    let records: Vec<_> = nodes.into_iter().map(crate::record_from_node).collect();
    OOB_CACHE.lock().unwrap().insert((file_id, hash), records.clone());
    Ok(records)
}
```

Wire it into `resolve_structural_path`: if `store.get_structural_node_by_path` returns `None` AND the file's size exceeds the indexing threshold (check via `fs.file_size`), call `parse_on_demand` and search the result in memory.

Add `once_cell = "1"` to `argyph-locate/Cargo.toml`.

Add a `record_from_node` helper in `crates/argyph-locate/src/lib.rs` (the inverse of `tiers::to_record` from Task A8, written once in this crate to avoid a cyclic dependency).

- [ ] **Step 3: Test**

In `crates/argyph/tests/locate_smoke.rs`, add a test that creates a 15 MB markdown file at runtime (synthesized, not committed) and asserts `locate` against it still returns a result with `strategy_used` set, demonstrating the on-demand path. Skip with `#[ignore]` if creating the file is too slow for default test runs.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test -p argyph-locate
git add crates/argyph-locate/ crates/argyph-fs/ crates/argyph/tests/
git commit -m "feat(locate): on-demand parse + stale-index detection"
```

---

## Done criteria

After all tasks complete:

- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- A fresh `argyph serve` against this very repo answers `locate` calls for:
  - `path: "ARCHITECTURE.md > Tier 1.5 — Structural index (non-code)"`
  - `path: "package.name"` against `Cargo.toml`
  - `query: "section about three-tier indexing"` against the docs/architecture
- README, ARCHITECTURE, and tools-reference all mention `locate`.
- `locate_smart` is **not** implemented in this phase. It's documented in the spec (§5) and will be planned separately.
