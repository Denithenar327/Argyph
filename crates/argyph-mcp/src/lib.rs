#![forbid(unsafe_code)]

// TODO: See crates/argyph-mcp/MODULE.md — owns the MCP server lifecycle (stdio
// JSON-RPC via rmcp), per-tool request/response types with JSON Schema derivation,
// boundary validation, error-code mapping, and correlation ID generation.
// Tool handlers are thin (<100 LOC) and dispatch to argyph_core::Index.

/// Starts the MCP stdio server. Each incoming tool request is validated at the
/// boundary and dispatched to the corresponding method on [`argyph_core::Index`].
pub trait McpServer {}
