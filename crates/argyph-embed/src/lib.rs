// SAFETY: This is the project's only allowlisted crate for `unsafe_code`.
// `unsafe` is permitted only inside the ONNX FFI module (`src/local.rs`) and
// must carry a `// SAFETY:` comment justifying each block. All other modules
// in this crate remain safe. See CONTRIBUTING.md §3.

// TODO: See crates/argyph-embed/MODULE.md — owns the embedding provider
// abstraction, ONNX runtime integration for the bundled local model, HTTP
// providers (OpenAI, Voyage), tokenizer wrapping, and lazy model download.

/// Converts text chunks into vector embeddings. Abstracts over a bundled local
/// ONNX model and remote HTTP providers with batching and retry logic.
pub trait Embedder {}
