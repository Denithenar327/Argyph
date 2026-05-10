#![forbid(unsafe_code)]

// TODO: See crates/argyph-cli/MODULE.md — owns terminal subcommands (index, status,
// search, symbols, graph, pack, doctor, init), output formatting (JSON, plain text,
// colorized), and progress bars. All logic is glue over argyph_core::Index.

/// Dispatches CLI subcommands parsed from command-line arguments. Everything
/// reusable lives in `argyph_core`; the CLI crate is pure glue and formatting.
pub trait CommandRunner {}
