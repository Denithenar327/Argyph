#![forbid(unsafe_code)]

// TODO: See crates/argyph-core/MODULE.md — owns Supervisor lifecycle, three-tier
// indexing orchestration, configuration, background task spawning, and the Index facade.

/// Orchestrates runtime lifecycle: boots the index, runs the three-tier pipeline,
/// spawns background work, and owns graceful shutdown.
pub trait Supervisor {}
