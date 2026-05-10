# Changelog

All notable changes to Argyph will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Argyph adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries marked **breaking** require a major version bump.

---

## [Unreleased]

### Added

- Initial project scaffolding: workspace, CI, ARCHITECTURE.md, per-crate MODULE.md.
- Repository documentation: README, CONTRIBUTING, COMMIT_CONVENTIONS, AGENT_WORKFLOW, SPEC, BUILD_PLAN, ROADMAP.

### Changed

(none yet)

### Removed

(none yet)

---

## [0.1.0-alpha] — TBD

### Added

- Tier 0 filesystem index with `.gitignore`-aware walking via the `ignore` crate.
- BLAKE3 hashing per file.
- SQLite metadata store with WAL mode and migration runner.
- Supervisor lifecycle in `argyph-core` with cancellation token and `JoinSet`.
- MCP server skeleton via `rmcp`, with three tools: `get_index_status`, `get_repo_overview`, `search_text`.
- CLI entry point with `serve`, `doctor`, `--version`, `init` subcommands.
- CI matrix for macOS, Linux, Windows on x64 and arm64 where supported.

---

## [0.2.0-alpha] — TBD

### Added

- Tier 1 symbol index with tree-sitter integration for Rust, TypeScript, Python.
- Symbol graph construction with calls, references, imports edges.
- AST-aware chunking with character-based fallback.
- Filesystem watcher with `notify` and debouncing; polling fallback via `ARGYPH_WATCHER=poll`.
- MCP tools: `find_definition`, `find_references`, `get_callers`, `get_callees`, `get_imports`, `get_symbol_outline`.
- Incremental updates: edited files trigger reparse and graph delta in <500 ms.

### Known limitations

- Cross-file symbol resolution is best-effort, per-language heuristic. Documented in `crates/argyph-graph/MODULE.md`.

---

## [0.3.0-beta] — TBD

### Added

- Tier 2 semantic index with LanceDB backing.
- Embedding provider abstraction with three implementations: bundled local ONNX (`bge-small-en-v1.5`), OpenAI, Voyage.
- Hybrid search via reciprocal rank fusion of BM25 (SQLite FTS5) and vector results.
- Background Tier 2 indexing with progress reporting via `get_index_status`.
- `search_semantic` MCP tool with language and path-glob filters.
- Lazy model download to `~/.cache/argyph/models/` with SHA-256 verification.

---

## [1.0.0-rc.1] — TBD

### Added

- `argyph-pack` crate: token-budgeted repo packing.
- Output formats: XML (primary, agent-readable) and markdown.
- Priority heuristic: explicit paths → entry points → READMEs → recently modified → high in-edge → rest.
- `pack_repo` MCP tool.

---

## [1.0.0] — TBD

### Added

- Distribution: prebuilt binaries via `cargo-dist` for darwin-arm64, darwin-x64, linux-x64, linux-arm64, win32-x64.
- npm wrapper package `@argyph/server` with postinstall binary download.
- Cargo install: `cargo install argyph`.
- DXT bundle: `argyph.dxt` for one-click Claude Desktop install.
- Benchmarks against named competitors with reproducible methodology.
- mdBook docs site.

### Stability commitment

- MCP tool schemas locked. Adding a tool is a minor-version event; removing or breaking a required field is a major-version event.
- CLI subcommand and flag names locked under SemVer.
- On-disk `.argyph/` layout part of the user-visible contract; future schema changes go through migration files only.

---

[Unreleased]: https://github.com/Ezzy1630/argyph/compare/v1.0.0...HEAD
[0.1.0-alpha]: https://github.com/Ezzy1630/argyph/releases/tag/v0.1.0-alpha
[0.2.0-alpha]: https://github.com/Ezzy1630/argyph/releases/tag/v0.2.0-alpha
[0.3.0-beta]: https://github.com/Ezzy1630/argyph/releases/tag/v0.3.0-beta
[1.0.0-rc.1]: https://github.com/Ezzy1630/argyph/releases/tag/v1.0.0-rc.1
[1.0.0]: https://github.com/Ezzy1630/argyph/releases/tag/v1.0.0
