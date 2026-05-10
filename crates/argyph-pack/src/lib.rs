#![forbid(unsafe_code)]

// TODO: See crates/argyph-pack/MODULE.md — owns repo packing, token-budgeted
// file prioritization, XML and markdown format rendering, and per-file
// truncation strategy when a file exceeds the remaining token budget.

/// Flattens a repository into a token-budgeted agent-readable representation.
/// Files are ordered by a priority heuristic (entry points, READMEs, recently
/// changed, high-in-edge count, then lexicographic).
pub trait Packer {}
