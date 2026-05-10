# `argyph-graph` — symbol graph

## Purpose

Build the cross-file symbol graph from `argyph-parse`'s per-file outputs. Resolve raw import statements into edges, link references to definitions where possible, build the call graph.

## Owns

- The `GraphBuilder` trait and default implementation.
- Cross-file import resolution per language (Rust modules, TS/JS module specifiers, Python imports). **This is heuristic, not type-resolved.**
- Reference-to-definition linking within and across files.
- Call edge construction (caller → callee within resolvable scope).
- Edge types: `Defines`, `References`, `Calls`, `Imports`, `ImportedBy`, `Implements`, `Inherits` (per language).
- Graph query operations: `find_definition`, `find_references`, `callers`, `callees`, `imports_of`, `imported_by`, `outline`.

## Must never own

- Parsing or tree-sitter (lives in `argyph-parse`).
- Persistence — graph queries are answered against `argyph-store`-backed data.
- Embedding or semantic search.
- MCP or CLI surfaces.

## Public surface

```rust
pub trait GraphBuilder {
    fn build_edges(
        &self,
        symbols: &[Symbol],
        imports: &[Import],
        prior: &dyn GraphIndex,
    ) -> Result<Vec<Edge>>;
}

pub trait GraphIndex {
    fn symbols_in_file(&self, file: &Utf8Path) -> Result<Vec<Symbol>>;
    fn symbol_by_qualified_name(&self, qn: &str) -> Result<Option<Symbol>>;
}

pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
    pub site: SiteRange,
    pub confidence: Confidence, // Resolved | Heuristic | Ambiguous
}

pub enum EdgeKind {
    Defines, References, Calls, Imports, ImportedBy, Implements, Inherits,
}

pub struct GraphQuery<'a> { /* private */ }

impl<'a> GraphQuery<'a> {
    pub async fn find_definition(&self, name: &str, hint: Option<&str>) -> Result<Vec<Definition>>;
    pub async fn find_references(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    pub async fn callers(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    pub async fn callees(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    pub async fn imports_of(&self, file: &Utf8Path) -> Result<ImportInfo>;
    pub async fn outline(&self, file: &Utf8Path) -> Result<Vec<OutlineEntry>>;
}
```

## Internal structure

- `src/builder.rs` — `GraphBuilder` trait and default implementation.
- `src/resolve/` — one file per language: `rust.rs`, `typescript.rs`, `python.rs`. Each implements that language's import-resolution heuristic.
- `src/edges.rs` — `Edge`, `EdgeKind`, `Confidence`.
- `src/query.rs` — `GraphQuery` and the read-side query operations.
- `src/selector.rs` — `SymbolSelector` (by name+file, by qualified name, by symbol id).

## Failure modes

- **Overconfident cross-file resolution.** Tree-sitter does not give us a type-resolved IR. Cross-file resolution is a heuristic. PRs that claim LSP-grade precision are rejected. The `Confidence` field on every edge documents this honestly.
- AI agents try to add a "type system" here. Don't. That is the LSP-bridge work scoped for v2.
- Stale graph after edits: incremental update must replace the affected file's outgoing edges atomically. Test under rapid file changes.
- AI agents resolve imports by string-matching. Real resolution requires understanding each language's module path conventions. Per-language modules in `resolve/` exist precisely to keep these rules contained.

## Honest limitations

- Cross-file resolution accuracy for v1.0 (intra-file is much higher):
  - Rust: ~85% (module/path resolution is mostly tractable)
  - TypeScript: ~75% (relative imports are clean; `paths` aliases require `tsconfig.json` parsing — partially supported)
  - Python: ~70% (dynamic imports and namespace packages are intractable without a runtime)
- Method resolution on dynamic types (TypeScript `any`, Python duck typing) is typically `Confidence::Ambiguous`.
- We do not perform type inference. A call like `obj.foo()` where `obj`'s type is unclear results in references to *all* `foo` methods in the codebase, marked `Ambiguous`.

These limitations are documented in the README and are not bugs. The path to higher accuracy is the LSP-bridge in v2.

## Stability

- Adding a new language's resolver is a contained, additive change.
- The `Edge`/`EdgeKind`/`Confidence` shape is part of the inter-crate contract with `argyph-store`.
- The query API is part of the `argyph-core::Index` facade and propagates to MCP tool schemas. Breaking changes require a major version bump after v1.0.
