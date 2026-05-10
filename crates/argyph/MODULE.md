# `argyph` — main binary

## Purpose

The single binary entry point. It does nothing except parse command-line arguments and dispatch into either `argyph-cli` (for terminal commands) or `argyph-mcp` (for `serve`).

## Owns

- `main.rs` argument parsing via `clap`.
- Subcommand dispatch.
- Top-level error handling: convert any unhandled error into a non-zero exit code with a one-line user-facing message and a suggestion to set `ARGYPH_LOG=debug` for detail.
- Initialization of `tracing-subscriber` based on `ARGYPH_LOG`.

## Must never own

- Any business logic. Anything that warrants a unit test belongs in `argyph-core`, `argyph-cli`, or `argyph-mcp`.
- Direct access to `argyph-fs`, `argyph-parse`, `argyph-graph`, `argyph-embed`, `argyph-store`. Those are reached through `argyph-core::Index` only.
- Configuration parsing (lives in `argyph-core::config`).
- Any `unsafe`, any `unwrap()`, any panicking code path.

## Public surface

None. This is a binary crate.

## Internal structure

Single file: `src/main.rs`. Should not exceed 200 lines.

## Failure modes

- AI agents try to put logic here because it is "the entry point." Reject any PR that adds non-trivial logic to `main.rs`.
- AI agents add error handling that swallows context. Errors here propagate to the user; do not collapse them into generic strings.

## Honest limitations

- No fancy interactive UI. If a user wants progress bars, those live in `argyph-cli` for the relevant subcommand.

## Stability

The CLI surface is part of the project's public API. Breaking changes (renaming subcommands, removing flags) require a major version bump after v1.0.
