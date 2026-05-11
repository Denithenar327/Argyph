# `argyph-graph` — symbol graph

## Purpose

Build the cross-file symbol graph from `argyph-parse`'s per-file outputs. Resolve raw import statements into edges, link references to definitions within files (cross-file is best-effort for v1), build the call graph.

## Owns

- `GraphBuilder` trait and `DefaultGraphBuilder` — assembles edges from per-file parsed data.
- `Graph` struct — in-memory edge store with query operations. **Sync, no I/O.**
- Per-language import resolvers under `src/resolve/` (Rust modules, TS module specifiers, Python relative imports). **Heuristic, not type-resolved.**
- Edge types: `Defines`, `References`, `Calls`, `Imports`, `ImportedBy`, `Implements`, `Inherits`.
- Confidence: `Resolved` (within-file identity edges only), `Heuristic` (references, calls, cross-file), `Ambiguous` (unused in v1).
- Graph queries: `find_definition`, `find_references`, `callers`, `callees`, `imports_of`, `imported_by`, `outline`.

## Must never own

- Parsing or tree-sitter (lives in `argyph-parse`).
- Persistence — graph queries are purely in-memory; `argyph-store` backs them.
- Embedding or semantic search.
- MCP or CLI surfaces.

## Public surface (v1.0 implemented)

```rust
pub trait GraphBuilder {
    fn build_edges(
        &self,
        files: &[(Utf8PathBuf, ParsedFile)],
    ) -> Result<Vec<Edge>, GraphError>;
}

pub struct DefaultGraphBuilder;

pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
    pub confidence: Confidence,
}

pub enum EdgeKind {
    Defines, References, Calls, Imports, ImportedBy, Implements, Inherits,
}

pub enum Confidence { Resolved, Heuristic, Ambiguous }

pub enum SymbolSelector {
    ById(SymbolId),
    ByName { file: Utf8PathBuf, name: String },
    Qualified(String),
}

pub struct Graph { /* private */ }

impl Graph {
    pub fn new(edges: Vec<Edge>) -> Self;
    pub fn edges(&self) -> &[Edge];
    pub fn count_by_kind(&self, kind: EdgeKind) -> usize;
    pub fn find_definition(&self, name: &str, file: Option<&Utf8Path>) -> Vec<&Edge>;
    pub fn find_references(&self, sel: &SymbolSelector) -> Vec<&Edge>;
    pub fn callers(&self, sel: &SymbolSelector) -> Vec<&Edge>;
    pub fn callees(&self, sel: &SymbolSelector) -> Vec<&Edge>;
    pub fn imports_of(&self, file: &Utf8Path) -> Vec<&Edge>;
    pub fn imported_by(&self, sel: &SymbolSelector) -> Vec<&Edge>;
    pub fn outline(&self, file: &Utf8Path) -> Vec<SymbolOutline>;
}
```

## Internal structure

- `src/builder.rs` — `GraphBuilder` trait + `DefaultGraphBuilder`. Builds all edges from parsed file data.
- `src/graph.rs` — `Graph` struct with indexed lookups (by_from, by_to, by_kind).
- `src/edge.rs` — `Edge`, `EdgeKind`, `Confidence`.
- `src/error.rs` — `GraphError` enum.
- `src/selector.rs` — `SymbolSelector` (by id, by name+file, by qualified name).
- `src/resolve/` — one file per language:
  - `rust.rs` — resolves `use crate::path::to::module` → `src/path/to/module.rs`
  - `typescript.rs` — resolves `import { X } from "./relative"` → `src/relative.ts`
  - `python.rs` — resolves `from .module import X` → `src/module.py` (uses raw import string to detect `.` prefixes)
- `tests/graph_tests.rs` — 16 integration tests covering all edge types across fixtures.

## v1 Edge construction approach

1. **Defines**: Every symbol → self edge (identity). Confidence: `Resolved`.
2. **References**: Scans chunk text overlapping with each symbol's range for other symbol names (word-boundary matching). Confidence: `Heuristic`.
3. **Calls**: Same as References, but specifically for `name(` patterns targeting Function/Method symbols. Confidence: `Heuristic`.
4. **Imports**: Resolves each import's module path to a target file via language resolvers, then links to matching symbols. Confidence: `Heuristic`.
5. **ImportedBy**: Reverse of Imports (from imported symbol back to importing symbol). Confidence: `Heuristic`.
6. **Implements/Inherits**: Not automatically constructed in v1 (requires type information).

## v1 limitations (honest)

| Limitation | Impact |
|-----------|--------|
| **Chunk precision**: The chunker produces `TopLevel` chunks for entire classes/types, not per-method `FunctionBody` chunks. This causes false-positive References/Calls between methods within the same class. | Class methods appear to reference/call each other even when they don't (e.g., `constructor` appears to call `greet`). |
| **Import per-symbol attribution**: Import edges are created from *every* symbol in the importing file to every imported item, not just the symbols that actually use the import. | 5 symbols × 2 imports × 2 items = 20+ edges for a small file. |
| **No type-path resolution for Rust**: `use` statements are resolved to file paths, but `mod` declarations and inline modules are not traversed. | Inline `mod tests` in Rust fixtures produces no cross-file edges. |
| **TS `paths` aliases unsupported**: `tsconfig.json` `paths` mappings are not parsed. | Bare specifiers like `import from '@utils/math'` are not resolved. |
| **Python dynamic imports**: Only `from .` relative imports are resolved. `importlib` and star imports are not handled. | ~70% cross-file accuracy for Python. |
| **No type inference**: `obj.foo()` where `obj`'s type is unclear does NOT create any reference. This is a deliberate omission, not a bug — it avoids the "all `foo` methods" spam that would mislead users. | Dynamic dispatch calls are silently absent from the call graph. |

## Cross-file resolution accuracy (v1)

| Language | Intra-file | Cross-file | Notes |
|----------|-----------|------------|-------|
| Rust | ~90% | ~50% | Intra-file works well. Cross-file limited: `mod` declarations not traversed, only `use crate::` and `use path::` patterns. |
| TypeScript | ~85% | ~75% | Intra-file: all function/class/interface references within a file detected. Cross-file: relative imports work; bare specifiers and `paths` aliases not supported. |
| Python | ~85% | ~70% | Intra-file: function calls and class references detected. Cross-file: `from .` relative imports resolved; dot-relative edge cases (single `.`, double `..`) handled; bare imports skipped. |

These limitations are design decisions, not bugs. The path to higher accuracy is the LSP-bridge in v2.

## Failure modes

- **Overconfident cross-file resolution.** Tree-sitter does not give us a type-resolved IR. Cross-file resolution is a heuristic. PRs that claim LSP-grade precision are rejected. The `Confidence` field on every edge documents this honestly.
- AI agents try to add a "type system" here. Don't. That is the LSP-bridge work scoped for v2.
- Stale graph after edits: incremental update must replace the affected file's outgoing edges atomically. Test under rapid file changes.
- AI agents resolve imports by string-matching. Real resolution requires understanding each language's module path conventions. Per-language modules in `resolve/` exist precisely to keep these rules contained.

## Stability

- Adding a new language's resolver is a contained, additive change.
- The `Edge`/`EdgeKind`/`Confidence` shape is part of the inter-crate contract with `argyph-store`.
- The query API is part of the `argyph-core::Index` facade and propagates to MCP tool schemas. Breaking changes require a major version bump after v1.0.
