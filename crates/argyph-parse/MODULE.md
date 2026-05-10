# `argyph-parse` — parsing & chunking

## Purpose

Tree-sitter integration. Parses source files into ASTs, extracts symbols (functions, classes, methods, imports, exports), and produces AST-aware chunks for downstream embedding.

## Owns

- The `Parser` trait and language-pack registry.
- Per-language `.scm` queries for symbol extraction (in `queries/<language>.scm`).
- Tree-sitter language registration. Currently: Rust, TypeScript, Python (v1.0).
- Chunking strategy: AST-aware boundaries with character-based fallback for unsupported languages or oversized AST nodes.
- Symbol extraction: turning AST nodes into `Symbol` records with stable identifiers.
- Chunk identification: content-addressed chunk IDs (BLAKE3 of normalized chunk text).
- Import-statement extraction (raw — resolution is `argyph-graph`'s job).

## Must never own

- Symbol resolution across files. We extract local symbols and raw import statements; `argyph-graph` resolves them into edges.
- Persistence. We yield in-memory data structures; `argyph-store` writes them.
- Embedding. We define the chunk; `argyph-embed` produces the vector.
- Anything related to MCP or CLI.

## Public surface

```rust
pub trait Parser {
    fn parse(&self, file: &FileEntry, source: &str) -> Result<ParsedFile>;
}

pub struct DefaultParser { /* private */ }

pub struct ParsedFile {
    pub symbols: Vec<Symbol>,
    pub chunks: Vec<Chunk>,
    pub imports: Vec<Import>,
}

pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,        // Function | Method | Struct | Enum | Trait | Class | Module | ...
    pub file: Utf8PathBuf,
    pub range: ByteRange,
    pub signature: Option<String>,
    pub parent: Option<SymbolId>,
}

pub struct Chunk {
    pub id: ChunkId,             // BLAKE3 of normalized text
    pub file: Utf8PathBuf,
    pub range: ByteRange,
    pub text: String,
    pub kind: ChunkKind,         // FunctionBody | TypeDef | TopLevel | Fallback
    pub language: Language,
}

pub struct Import {
    pub raw: String,             // unresolved — graph crate resolves
    pub module_path: Vec<String>,
    pub items: Vec<String>,
    pub range: ByteRange,
}
```

## Internal structure

- `src/parser.rs` — `DefaultParser` and trait.
- `src/languages/` — one file per language: `rust.rs`, `typescript.rs`, `python.rs`. Each registers its tree-sitter language and its `.scm` queries.
- `src/queries/` — `.scm` files per language. Loaded at compile time via `include_str!`.
- `src/chunker.rs` — chunking strategy (AST-aware with fallback).
- `src/symbol.rs` — `Symbol`, `SymbolId`, `SymbolKind`.
- `src/chunk.rs` — `Chunk`, `ChunkId`, `ChunkKind`.

## Failure modes

- AI agents over-eager chunking that splits mid-function. Property test: reassembling chunks must yield original source.
- AI agents update tree-sitter language deps without regenerating queries. Pin versions; document the upgrade procedure in `docs/recipes/upgrade-tree-sitter.md`.
- AI agents try to do "symbol resolution" here. They cannot. Resolution requires the cross-file context that `argyph-graph` builds.
- Oversized or pathological files (generated, minified, autogen). Per-file size cap is enforced upstream by `argyph-fs`; we additionally clamp per-AST-node depth to avoid stack overflows.

## Honest limitations

- Chunk boundaries inside very large functions fall back to character-based splits. We mark these `ChunkKind::Fallback` so downstream can deprioritize them in ranking.
- We do not currently parse comments as separate searchable entities. Comments inside function bodies are part of the function's chunk; doc comments above declarations are merged into the symbol's signature.
- Languages outside the v1.0 set produce file-level chunks only — no symbols, no graph.

## Stability

- Adding a new language pack is the most common contribution to this crate. Recipe: `docs/recipes/add-language.md`.
- The `Symbol`/`Chunk`/`Import` shapes are part of the inter-crate contract with `argyph-graph` and `argyph-store`.
- `.scm` queries can be updated freely as long as snapshot tests on fixtures still pass.
