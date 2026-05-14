# `argyph-mcp` — MCP server

## Purpose

The MCP protocol surface. Hosts the JSON-RPC server (via the official `rmcp` crate), defines tool schemas, and dispatches calls to `argyph-core::Index`. Handlers are intentionally thin.

## Owns

- The MCP server lifecycle (stdio framing).
- Per-tool request and response types (with JSON Schema derivation via `schemars`).
- Schema validation at the boundary.
- Path normalization and traversal-protection guards on every path argument.
- Argument clamping (`k` to [1, 100], `token_budget` to [1000, 200000], etc.).
- Error code mapping from internal `argyph-core` errors to MCP error envelopes with stable codes (`INDEX_NOT_READY`, `INVALID_PATH`, etc.).
- Correlation ID generation per request, tied to a tracing span.

## Must never own

- Any business logic. Each tool handler is <100 lines and dispatches to a single `Index` method.
- Indexing, storage, parsing, embedding, packing, graph queries.
- Anything that would justify a new third-party dependency.

## Public surface

The crate exposes one function:

```rust
pub async fn serve(supervisor: Arc<Supervisor>) -> Result<()>;
```

Internally, each tool is a small module under `src/tools/<tool>.rs` exposing a handler function and request/response types.

The MCP tool catalog (v1.0):

| Tool                  | Source file                               |
|-----------------------|-------------------------------------------|
| `get_index_status`    | `src/tools/get_index_status.rs`           |
| `get_repo_overview`   | `src/tools/get_repo_overview.rs`          |
| `search_text`         | `src/tools/search_text.rs`                |
| `search_semantic`     | `src/tools/search_semantic.rs`            |
| `find_definition`     | `src/tools/find_definition.rs`            |
| `find_references`     | `src/tools/find_references.rs`            |
| `get_callers`         | `src/tools/get_callers.rs`                |
| `get_callees`         | `src/tools/get_callees.rs`                |
| `get_imports`         | `src/tools/get_imports.rs`                |
| `get_symbol_outline`  | `src/tools/get_symbol_outline.rs`         |
| `pack_repo`           | `src/tools/pack_repo.rs`                  |

## Internal structure

- `src/lib.rs` — `serve()` and `rmcp` integration.
- `src/tools/` — one file per tool. Each <100 lines.
- `src/types.rs` — shared request/response shapes (`Filter`, `SymbolSelector`, etc.).
- `src/validate.rs` — boundary validation (paths, ranges, globs).
- `src/error.rs` — MCP error envelope and code enum.

## Failure modes

- **AI agents putting logic in handlers.** Handlers must dispatch to `Index` methods. If a handler exceeds 100 lines, the logic belongs upstream. CI lints check handler size.
- **AI agents skipping path validation.** `validate_repo_path()` is a single hardened function; every path argument passes through it. Forgetting it is a CVE-shaped bug.
- **AI agents adding new third-party deps to handle JSON.** Use `serde_json` and `schemars` only.
- **AI agents emitting raw Rust errors across the MCP boundary.** Errors are typed `ArgyphError { code, message, retryable }`. Panics must be caught at the supervisor boundary.
- **AI agents writing logs to stdout.** stdio is the MCP channel. Logs go to stderr only. CI lints for `println!` in this crate.

## Honest limitations

- MCP Resources and Prompts are not implemented in v1.0 (deferred to v1.1, demand-driven).
- Schema introspection is provided per-tool; there is no global schema dump endpoint (the MCP spec does not require one).

## Stability

- MCP tool schemas are locked at v1.0. Adding a tool is a minor-version event; removing or breaking a required field is a major-version event.
- The error code enum is stable; new codes can be added (additive), existing codes never removed.
- Adding a new MCP tool is a common contribution. Recipe: `docs/recipes/add-tool.md`.
