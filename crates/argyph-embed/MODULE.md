# `argyph-embed` — embeddings

## Purpose

Convert text chunks into vector embeddings. Abstracts over a bundled local ONNX model and remote HTTP providers (OpenAI, Voyage in v1.0; Gemini, Ollama in v1.1).

## Owns

- The `Embedder` trait.
- ONNX runtime integration via the `ort` crate.
- The bundled local model (`bge-small-en-v1.5`, FP32) — lazy download, SHA-256 verification, `~/.cache/argyph/models/` caching.
- HTTP clients for remote providers, with batching, retries (exponential backoff), and provider-specific concurrency caps.
- Tokenizer integration via the `tokenizers` crate (per-model tokenizer; bundled with model files).
- Token counting for chunk-budgeted batching.
- The single allowlisted `unsafe` block in the project, isolated to ONNX FFI initialization.

## Must never own

- Storage or retrieval (lives in `argyph-store`).
- The decision of *what* to embed (lives in `argyph-core` — Tier 2 orchestration).
- Parsing or chunking (lives in `argyph-parse`).
- MCP, CLI, or any user-facing surface.

## Public surface

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    fn dimension(&self) -> usize;
    fn model_id(&self) -> &str;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        Ok(self.embed(&[query.to_string()]).await?.remove(0))
    }
}

pub struct LocalEmbedder { /* private */ }
pub struct OpenAiEmbedder { /* private */ }
pub struct VoyageEmbedder { /* private */ }

pub enum Provider {
    Local,
    OpenAi,
    Voyage,
    // v1.1: Gemini, Ollama
}

pub fn build(provider: Provider, config: EmbedConfig) -> Result<Arc<dyn Embedder>>;

pub struct ApiKey(/* redacted Display */);
```

## Internal structure

- `src/lib.rs` — trait definition and `build()` factory.
- `src/local.rs` — ONNX-backed local embedder. Contains the project's only `unsafe` block (ONNX initialization) — annotated with `// SAFETY:`.
- `src/openai.rs` — OpenAI provider.
- `src/voyage.rs` — Voyage provider.
- `src/http.rs` — shared HTTP client config (timeouts, retries, redaction).
- `src/model_files.rs` — lazy download with SHA-256 verification.
- `src/tokenize.rs` — tokenizer wrapper and token counting.
- `src/error.rs` — typed errors.

## Failure modes

- **Loading the ONNX model per task instead of pooling.** A 80MB model loaded per task explodes memory. Pool the model behind an `Arc`.
- **Cross-platform ONNX builds.** Particularly Windows. Test in CI on all platforms; gate the local provider behind a feature flag if it's not yet stable on a given platform (with a clear runtime error message).
- **Logging API keys.** `ApiKey` is a redacting newtype; `Display` prints `***`. Never use `Debug` for `ApiKey` outside trace-level diagnostics.
- **Naive batching.** Each provider has its own batch-size and concurrency limits; respect them. Voyage has lower per-request limits than OpenAI.
- **AI agents adding a 4th provider in the same PR as a 3rd.** Every provider is a separate atomic PR.

## Honest limitations

- The bundled local model is `bge-small-en-v1.5` — adequate for hybrid search, not state-of-the-art on code-specific queries. Users who care about top-tier semantic quality on prose-like queries should configure OpenAI or Voyage.
- We considered `nomic-embed-code-v1` (code-specific, larger). Future work: a "code-quality" preset that bundles `nomic-embed-code-v1` instead.
- No fine-tuning. The embeddings are off-the-shelf.

## Stability

- The `Embedder` trait is the inter-crate contract with `argyph-core` (which decides *when* to embed) and `argyph-store` (which writes the vectors). It is small, stable, and unlikely to change.
- Adding a new provider is the most common contribution to this crate. Recipe: `docs/recipes/add-embed-provider.md`.
- The bundled model can be swapped, but doing so changes the embedding dimension and forces a reindex. This is a major version event after v1.0.
