# `argyph-core` — orchestration

## Purpose

The orchestration layer. Owns the runtime lifecycle, the three-tier indexing flow, configuration, and the `Index` facade type that everything above this layer (CLI, MCP) consumes. This is the only crate that wires together `argyph-fs`, `argyph-parse`, `argyph-graph`, `argyph-embed`, and `argyph-store`.

## Owns

- `Supervisor` — the single owner of runtime state. Boots, runs, schedules, shuts down.
- The three-tier indexing orchestration logic: walk → parse → graph-build → embed.
- Background task spawning. **All long-lived tasks in the project go through `Supervisor::spawn`.**
- `CancellationToken` propagation for graceful shutdown.
- `Config` type and config layering (env > repo file > defaults). Uses `figment` internally.
- The `Index` facade — a single `Arc<Index>` that exposes domain-shaped queries to UI layers without leaking storage details.
- Tier readiness state (`TierState`) and its observer pattern.
- Filesystem watcher integration (debouncing, fallback to polling).

## Must never own

- Filesystem walking primitives (those live in `argyph-fs`).
- Tree-sitter or any parser specifics (live in `argyph-parse`).
- Symbol graph construction (lives in `argyph-graph`).
- Embedding model loading or HTTP calls (live in `argyph-embed`).
- SQLite or LanceDB schema/queries (live in `argyph-store`).
- MCP protocol handling (lives in `argyph-mcp`).
- Direct CLI output formatting (lives in `argyph-cli`).

## Public surface

```rust
pub struct Supervisor { /* private */ }

impl Supervisor {
    pub async fn boot(root: PathBuf, config: Config) -> Result<Self>;
    pub async fn run(self) -> Result<()>;
    pub fn index(&self) -> Arc<Index>;
    pub async fn shutdown(self) -> Result<()>;
    pub fn spawn<F, T>(&self, fut: F)
        where F: Future<Output = T> + Send + 'static, T: Send + 'static;
}

pub struct Index { /* private */ }

impl Index {
    pub async fn status(&self) -> IndexStatus;
    pub async fn overview(&self) -> RepoOverview;
    pub async fn search_text(&self, q: &TextQuery) -> Result<Vec<TextHit>>;
    pub async fn search_semantic(&self, q: &SemanticQuery) -> Result<SemanticResults>;
    pub async fn find_definition(&self, name: &str, hint: Option<&str>) -> Result<Vec<Definition>>;
    pub async fn find_references(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    pub async fn callers(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    pub async fn callees(&self, sel: SymbolSelector) -> Result<Vec<Reference>>;
    pub async fn imports(&self, file: &Utf8Path) -> Result<ImportInfo>;
    pub async fn outline(&self, file: &Utf8Path) -> Result<Vec<OutlineEntry>>;
    pub async fn pack(&self, req: &PackRequest) -> Result<PackResult>;
    pub async fn read_range(&self, file: &Utf8Path, range: Range<usize>) -> Result<String>;
    pub async fn reindex(&self, scope: ReindexScope) -> Result<()>;
}

pub struct Config { /* private */ }

impl Config {
    pub fn load(root: &Path) -> Result<Self>;
    // accessors for sub-configs
}
```

## Internal structure

- `src/supervisor.rs` — `Supervisor` struct and lifecycle. **Architecture-protected. Edits require an issue + design discussion.**
- `src/index.rs` — the `Index` facade.
- `src/tiers.rs` — three-tier orchestration logic.
- `src/config.rs` — `figment`-backed layered config.
- `src/watcher.rs` — `notify` integration, debouncing, polling fallback.
- `src/error.rs` — typed errors for the crate.

## Failure modes

- AI agents try to spawn long-lived tasks elsewhere (in MCP handlers, in the watcher, in domain crates). All `tokio::spawn` calls outside `Supervisor::spawn` are forbidden by lint and CI.
- AI agents add new methods to `Index` to "make it easier" without checking that the underlying capability exists. Index methods must always have a clear domain-crate counterpart.
- Cancellation handling is omitted. Every background task must respect `self.shutdown` (the cancellation token).
- Configuration leakage: env-var parsing and config-file parsing both live here. Other crates take strongly-typed sub-configs.

## Honest limitations

- The Supervisor is single-instance per process. Argyph indexes one repo per running server. Multi-repo workspaces are a v2 concern.
- Tier 2 progress reporting is coarse-grained (chunks remaining, percent). Per-file progress is intentionally not exposed because it would be stale and misleading.

## Stability

- Every public type and method here is part of the project's internal contract. Changes ripple through `argyph-cli` and `argyph-mcp`.
- `supervisor.rs` is architecture-protected — see `CONTRIBUTING.md` § 4.
- Config schema changes require a `CHANGELOG.md` entry and may require a migration note for existing users' `.argyph/config.toml`.
