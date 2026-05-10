# `argyph-pack` — repo packing

## Purpose

Take a repo (or a subset) and produce a token-budgeted, agent-readable flattened representation. Equivalent in spirit to `repomix`, but driven by the symbol graph for smarter prioritization.

## Owns

- The `Packer` trait and default implementation.
- Token counting per provider tokenizer (default `tiktoken-rs` for OpenAI-compatible models; per-provider tokenizers when configured).
- Priority heuristic for inclusion order under a token budget:
  1. Explicitly-requested paths
  2. Entry points (`main.rs`, `lib.rs`, `index.ts`, `__main__.py`, `package.json` `main` field)
  3. README and top-level docs
  4. Recently modified files (within last 7 days, by mtime)
  5. Files with high in-edge count in the symbol graph
  6. Remaining files in lexicographic order
- Output formats: XML (primary, agent-friendly), markdown (human-friendly).
- Per-file truncation strategy when even one file exceeds the remaining budget (drop function bodies, keep signatures).

## Must never own

- Indexing of any kind.
- Storage.
- Embedding.
- Anything user-input-shaped beyond the `PackRequest` struct.
- Direct filesystem I/O — file content comes through `argyph-core::Index::read_range` or equivalent.

## Public surface

```rust
pub trait Packer {
    fn pack(&self, req: &PackRequest, ctx: &dyn PackContext) -> Result<PackResult>;
}

pub trait PackContext {
    fn list_files(&self, scope: &PackScope) -> Vec<Utf8PathBuf>;
    fn read(&self, file: &Utf8Path) -> Result<String>;
    fn modified(&self, file: &Utf8Path) -> Option<SystemTime>;
    fn in_edges(&self, file: &Utf8Path) -> Result<usize>;
}

pub struct PackRequest {
    pub scope: PackScope,                 // All | Paths(...) | Symbol(name)
    pub format: PackFormat,               // Xml | Markdown
    pub token_budget: usize,
    pub include: PackInclude,             // tests, docs flags
}

pub struct PackResult {
    pub format: PackFormat,
    pub content: String,
    pub token_count: usize,
    pub files_included: Vec<Utf8PathBuf>,
    pub files_truncated: Vec<Utf8PathBuf>,
    pub files_omitted: Vec<Utf8PathBuf>,
}
```

## Internal structure

- `src/lib.rs` — trait, `DefaultPacker`.
- `src/priority.rs` — the inclusion-order heuristic.
- `src/render/xml.rs` — XML format.
- `src/render/markdown.rs` — markdown format.
- `src/tokenize.rs` — token counting.
- `src/truncate.rs` — per-file truncation strategy.

## Failure modes

- **AI agents adding a JSON renderer "for completeness."** It is deliberately deferred (see `docs/SPEC.md` § 4.2).
- **AI agents counting tokens with the wrong tokenizer.** The default is `tiktoken-rs` (OpenAI-compatible). Per-provider tokenizers are loaded from `argyph-embed`'s tokenizer registry when configured.
- **AI agents reading files directly through `std::fs`.** They must go through `PackContext`, which the orchestrator (`argyph-core`) supplies.
- **AI agents producing malformed XML.** Output is validated against an XSD-equivalent test in CI.

## Honest limitations

- Token counts are estimates within ~5% accuracy across non-OpenAI providers; the budget is a soft guarantee.
- The priority heuristic is hand-tuned, not learned. Future work could add a small re-ranker fed by feedback signals.
- We do not currently dedupe near-duplicate files; in monorepos with vendored copies of the same lib, both copies may be packed.

## Stability

- The `PackResult` shape is part of the public MCP tool schema. Changes require a major version bump after v1.0.
- Output format strings are part of the user contract — a downstream agent's prompt may parse the XML structure. Format changes are breaking.
