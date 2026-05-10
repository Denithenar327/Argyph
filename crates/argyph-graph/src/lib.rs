#![forbid(unsafe_code)]

// TODO: See crates/argyph-graph/MODULE.md — owns symbol graph construction,
// cross-file import resolution (per-language heuristics), edge building
// (defs, refs, calls, imports), and graph query operations.

/// Builds cross-file symbol edges from per-file parse results. Resolves imports
/// into edges (best-effort, heuristic — not LSP-precise) and links references to
/// definitions where possible.
pub trait GraphBuilder {}
