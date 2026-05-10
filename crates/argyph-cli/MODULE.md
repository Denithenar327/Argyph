# `argyph-cli` — terminal UI

## Purpose

The non-MCP user-facing surface. Implements the terminal subcommands (`index`, `status`, `search`, `symbols`, `graph`, `pack`, `doctor`, `init`) so users can debug what the agent sees and so we have a free integration testing surface for the same `Index` methods.

## Owns

- All `argyph` subcommands except `serve` (which is `argyph-mcp`).
- Output formatting (JSON, plain text, color when `tty`).
- Progress bars via `indicatif` for long-running commands.
- The `init` command's config-file scaffolding.
- The `doctor` command's diagnostic output.

## Must never own

- Anything reusable. If a piece of logic could plausibly be useful from MCP as well, it belongs in `argyph-core`.
- Indexing, storage, parsing, embedding, packing, graph — those are reached through `argyph-core::Index`.
- Any direct file reads beyond the ones the user explicitly asks for via subcommand args.

## Public surface

```rust
pub fn dispatch(matches: &clap::ArgMatches) -> ExitCode;
```

Subcommand modules each expose a `run()` function called by `dispatch`.

## Internal structure

- `src/lib.rs` — `dispatch` and clap integration.
- `src/cmds/index.rs`, `status.rs`, `search.rs`, `symbols.rs`, `graph.rs`, `pack.rs`, `doctor.rs`, `init.rs` — one file per subcommand.
- `src/output.rs` — output formatters (JSON, plain text, table, colorized).
- `src/progress.rs` — `indicatif` wrapper that respects `--no-progress` and non-tty.

## Failure modes

- AI agents reimplementing logic that exists in `argyph-core`. Reject. The CLI is glue.
- AI agents emitting color escapes when stdout is not a tty. The output module checks `is_terminal()`.
- AI agents adding a TUI library "for ergonomics." Out of scope for v1.0.

## Honest limitations

- The CLI is intended for debugging and direct use, not as a primary interface. The primary interface is MCP.
- We do not stream subcommand output as JSON Lines (yet); that's a v1.1 ergonomics improvement.

## Stability

- Subcommand names and flag names are part of the public CLI contract. Breaking changes require a major version after v1.0.
- Output formats for JSON mode are stable; plain-text formats may change between minor versions.
