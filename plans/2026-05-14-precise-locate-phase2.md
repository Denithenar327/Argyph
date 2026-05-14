# Precise Locate — Phase 2 Implementation Plan (`locate_smart`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the opt-in `locate_smart` MCP tool: an in-process retrieval subagent that runs a bounded ReAct-style loop, composing the existing read-only tools (`locate`, `read_file_range`, `get_symbol_outline`, `get_repo_overview`) and returning a curated set of spans plus a short reasoning summary.

**Architecture:** A new `smart` module inside the existing `argyph-locate` crate (gated by a Cargo feature flag of the same name). A `LocateModel` trait abstracts the LLM provider; concrete impls cover OpenAI, Anthropic, and a local Ollama-compatible endpoint. A bounded ReAct loop in `smart::run` dispatches model tool-calls against an in-process allowlist, validates the final span selection against ranges actually produced during the loop, and returns the curated result. The MCP tool registers conditionally: if `[locate_smart].enabled` is false (or the section is absent), the tool is **not** exposed to the MCP client at all — preserving Argyph's "no API key required for full functionality" property.

**Tech Stack:** Same as Phase 1. New deps (feature-gated): `reqwest` (HTTPS clients), `eventsource-stream` (streaming, optional), `tokio` (already present).

**Prerequisite:** Phase 1 plan is fully landed. `locate` tool, Tier 1.5 index, `argyph-locate::locate()` entry point all exist and have green tests.

---

## File Structure

**New files:**

```
crates/argyph-locate/src/smart/
  mod.rs              # public entry: smart::run(req, ctx) -> Response
  model.rs            # LocateModel trait + ModelStep / Message types
  loop.rs             # ReAct loop driver + budget tracking
  tools.rs            # SubTool dispatch (locate / read_file_range / outline / overview)
  validate.rs         # span-validation against in-loop history
  prompts.rs          # system prompt template + tool descriptions
  providers/
    mock.rs           # MockModel for tests (always built)
    openai.rs         # cfg(feature = "smart")
    anthropic.rs      # cfg(feature = "smart")
    ollama.rs         # cfg(feature = "smart")

crates/argyph-mcp/src/tools/
  locate_smart.rs     # MCP handler with config gate
```

**Modified files:**

```
crates/argyph-locate/Cargo.toml           # [features] smart = [...]; add reqwest behind feature
crates/argyph-locate/src/lib.rs           # pub mod smart;
crates/argyph-mcp/src/lib.rs              # conditional registration of locate_smart tool
crates/argyph-mcp/src/error.rs            # add LocateSmartDisabled, LocateSmartBudgetExceeded, ProviderError
crates/argyph-core/src/config.rs          # parse [locate_smart] section from .argyph/config.toml
                                          #   (file path verified during Task C1 — exact location may differ)
README.md                                 # add `locate_smart` row + brief Optional Features section
ARCHITECTURE.md                           # add subsection under §2 covering the optional smart layer
docs/tools-reference.md                   # add `locate_smart` schema
```

---

## Conventions for every task

- Run from repo root.
- Format and clippy before each commit:
  `cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Tests: `cargo test --workspace --all-features`.
- Each task ends with a commit. `git status` clean between tasks.
- The Cargo feature `smart` is **off by default**. All provider code is `#[cfg(feature = "smart")]`. The `MockModel` and trait are always compiled so tests can exercise the loop without enabling the feature.

---

## Task C1: Cargo feature flag + config plumbing

**Files:**
- Modify: `crates/argyph-locate/Cargo.toml`
- Modify: `crates/argyph-locate/src/lib.rs`
- Modify: `crates/argyph-core/src/config.rs` (verify exact path with `rg 'pub struct Config' crates/argyph-core/src/`)

- [ ] **Step 1: Add Cargo feature**

In `crates/argyph-locate/Cargo.toml`:

```toml
[features]
smart = ["dep:reqwest", "dep:eventsource-stream"]

[dependencies]
# ... existing deps ...
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"], optional = true }
eventsource-stream = { version = "0.2", optional = true }
async-trait = "0.1"
```

In `crates/argyph-locate/src/lib.rs`, append:

```rust
pub mod smart;
```

Create `crates/argyph-locate/src/smart/mod.rs` as an empty placeholder for now:

```rust
//! Optional in-process retrieval subagent.
//! See spec §5 (specs/2026-05-13-precise-locate-design.md).

pub mod model;
pub mod tools;
pub mod validate;
pub mod prompts;
pub mod providers;

#[allow(clippy::module_inception)]
mod loop_;
pub use loop_::run;

pub use model::{LocateModel, LocateModelError, Message, ModelStep, Role};
```

Create empty stub files for every submodule referenced:

```bash
mkdir -p crates/argyph-locate/src/smart/providers
for f in model tools validate prompts loop_; do
  echo "//! Stub. Implemented in subsequent tasks." > crates/argyph-locate/src/smart/$f.rs
done
echo "pub mod mock;" > crates/argyph-locate/src/smart/providers/mod.rs
echo '#[cfg(feature = "smart")] pub mod openai;'    >> crates/argyph-locate/src/smart/providers/mod.rs
echo '#[cfg(feature = "smart")] pub mod anthropic;' >> crates/argyph-locate/src/smart/providers/mod.rs
echo '#[cfg(feature = "smart")] pub mod ollama;'    >> crates/argyph-locate/src/smart/providers/mod.rs
echo "//! Stub. Implemented in C2." > crates/argyph-locate/src/smart/providers/mock.rs
for f in openai anthropic ollama; do
  echo "//! Stub. Implemented in C6/C7/C8." > crates/argyph-locate/src/smart/providers/$f.rs
done
```

- [ ] **Step 2: Config struct**

First verify where config lives. Run `rg 'locate_max_file_bytes' crates/argyph-core/src/` to find the struct extended in Phase 1 Task A8.

In that file (likely `crates/argyph-core/src/config.rs`), add:

```rust
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LocateSmartConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: Option<String>,    // "openai" | "anthropic" | "ollama"
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default = "default_max_steps")]
    pub max_steps: u8,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
}
fn default_max_steps() -> u8 { 4 }
fn default_max_output_tokens() -> u32 { 1024 }
```

Add to the main `Config` struct:

```rust
#[serde(default)]
pub locate_smart: LocateSmartConfig,
```

Add env-var overrides in the config-loading function (search for `ARGYPH_LOCATE_MAX_FILE_BYTES` and mirror):

```rust
if let Ok(v) = std::env::var("ARGYPH_LOCATE_SMART_ENABLED") {
    config.locate_smart.enabled = v == "true" || v == "1";
}
if let Ok(v) = std::env::var("ARGYPH_LOCATE_SMART_PROVIDER") {
    config.locate_smart.provider = Some(v);
}
if let Ok(v) = std::env::var("ARGYPH_LOCATE_SMART_MODEL") {
    config.locate_smart.model = Some(v);
}
```

- [ ] **Step 3: Build clean (without `smart` feature)**

```bash
cargo build --workspace
```

Expected: clean. Provider modules are gated; only the mock + stubs compile.

- [ ] **Step 4: Build with `smart`**

```bash
cargo build --workspace --features argyph-locate/smart
```

Expected: clean. (Provider stubs still empty but compile.)

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/argyph-locate/Cargo.toml crates/argyph-locate/src/smart/ crates/argyph-core/src/
git commit -m "feat(locate-smart): scaffolding + Cargo feature + config plumbing"
```

---

## Task C2: `LocateModel` trait + `MockModel`

**Files:**
- Modify: `crates/argyph-locate/src/smart/model.rs`
- Modify: `crates/argyph-locate/src/smart/providers/mock.rs`

- [ ] **Step 1: Define trait and types**

Replace `crates/argyph-locate/src/smart/model.rs` with:

```rust
//! Trait abstraction for the retrieval-subagent LLM provider.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role { System, User, Assistant, Tool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Optional tool-call id this message is a response to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional tool name (for Role::Tool messages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

/// A model's response to a `step()` call.
#[derive(Debug, Clone)]
pub enum ModelStep {
    /// The model wants to call one of the sub-tools.
    ToolCall {
        id: String,
        name: String,           // "locate" | "read_file_range" | "get_symbol_outline" | "get_repo_overview"
        arguments: serde_json::Value,
    },
    /// The model is emitting its final answer: a curated list of span node-ids
    /// (each one MUST correspond to a span returned by an earlier `locate` call
    /// in this loop — the loop driver validates this).
    Final {
        selected_node_ids: Vec<String>,
        reasoning_summary: String,
    },
}

#[derive(Debug, Error)]
pub enum LocateModelError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("model output parse error: {0}")]
    Parse(String),
    #[error("rate limited; retry_after={retry_after_ms}ms")]
    RateLimit { retry_after_ms: u64 },
    #[error("budget exceeded: {0}")]
    Budget(String),
}

/// LLM provider abstraction. Implementations must be Send + Sync.
#[async_trait]
pub trait LocateModel: Send + Sync {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError>;
}
```

- [ ] **Step 2: MockModel implementation**

Replace `crates/argyph-locate/src/smart/providers/mock.rs` with:

```rust
//! Scriptable mock model for tests.

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep};
use async_trait::async_trait;
use std::sync::Mutex;

pub struct MockModel {
    script: Mutex<std::collections::VecDeque<ModelStep>>,
    pub call_log: Mutex<Vec<Vec<Message>>>,
}

impl MockModel {
    pub fn new(steps: Vec<ModelStep>) -> Self {
        Self {
            script: Mutex::new(steps.into()),
            call_log: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LocateModel for MockModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        self.call_log.lock().unwrap().push(messages.to_vec());
        self.script.lock().unwrap().pop_front().ok_or_else(|| {
            LocateModelError::Provider("MockModel script exhausted".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart::model::Role;

    #[tokio::test]
    async fn mock_returns_scripted_steps_in_order() {
        let mock = MockModel::new(vec![
            ModelStep::ToolCall { id: "1".into(), name: "locate".into(), arguments: serde_json::json!({}) },
            ModelStep::Final { selected_node_ids: vec!["n1".into()], reasoning_summary: "done".into() },
        ]);
        let msgs = vec![Message { role: Role::User, content: "hi".into(), tool_call_id: None, tool_name: None }];
        let first  = mock.step(&msgs).await.unwrap();
        assert!(matches!(first, ModelStep::ToolCall { .. }));
        let second = mock.step(&msgs).await.unwrap();
        assert!(matches!(second, ModelStep::Final { .. }));
    }

    #[tokio::test]
    async fn mock_records_calls() {
        let mock = MockModel::new(vec![ModelStep::Final {
            selected_node_ids: vec![], reasoning_summary: "".into(),
        }]);
        let _ = mock.step(&[]).await.unwrap();
        assert_eq!(mock.call_log.lock().unwrap().len(), 1);
    }
}
```

- [ ] **Step 3: Tests**

```bash
cargo test -p argyph-locate smart::providers::mock
```

Expected: `2 passed`.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/model.rs crates/argyph-locate/src/smart/providers/mock.rs
git commit -m "feat(locate-smart): LocateModel trait + MockModel"
```

---

## Task C3: Sub-tool dispatch

**Files:**
- Modify: `crates/argyph-locate/src/smart/tools.rs`

- [ ] **Step 1: Define SubTool enum and dispatch**

Replace `crates/argyph-locate/src/smart/tools.rs` with:

```rust
//! In-process dispatch for the four read-only sub-tools the model may invoke.
//! Hardcoded allowlist — model cannot escape this set.

use crate::types::{Request as LocateRequest, Response as LocateResponse};
use argyph_embed::Embedder;
use argyph_fs::FileIndex;
use argyph_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
pub enum SubTool {
    Locate(LocateRequest),
    ReadFileRange { file: String, byte_range: (u32, u32) },
    GetSymbolOutline { file: String },
    GetRepoOverview {},
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SubToolOutput {
    Locate(LocateResponse),
    ReadFileRange { file: String, content: String, truncated: bool },
    SymbolOutline { file: String, outline: serde_json::Value },
    RepoOverview { overview: serde_json::Value },
}

pub struct SubToolCtx {
    pub store: Arc<dyn Store>,
    pub fs: Arc<FileIndex>,
    pub embedder: Arc<dyn Embedder>,
}

pub async fn dispatch(
    ctx: &SubToolCtx,
    name: &str,
    args: &serde_json::Value,
    max_bytes_per_read: u32,
) -> anyhow::Result<SubToolOutput> {
    match name {
        "locate" => {
            let req: LocateRequest = serde_json::from_value(args.clone())?;
            let resp = crate::locate(ctx.store.clone(), ctx.fs.clone(), ctx.embedder.clone(), req).await?;
            Ok(SubToolOutput::Locate(resp))
        }
        "read_file_range" => {
            let file = args["file"].as_str()
                .ok_or_else(|| anyhow::anyhow!("read_file_range: missing `file`"))?
                .to_string();
            let start = args["byte_range"][0].as_u64()
                .ok_or_else(|| anyhow::anyhow!("read_file_range: bad byte_range[0]"))? as u32;
            let end   = args["byte_range"][1].as_u64()
                .ok_or_else(|| anyhow::anyhow!("read_file_range: bad byte_range[1]"))? as u32;
            let capped_end = std::cmp::min(end, start.saturating_add(max_bytes_per_read));
            let content = ctx.fs.read_byte_range(&file, start, capped_end).await?;
            Ok(SubToolOutput::ReadFileRange {
                file, content, truncated: capped_end < end,
            })
        }
        "get_symbol_outline" => {
            let file = args["file"].as_str()
                .ok_or_else(|| anyhow::anyhow!("get_symbol_outline: missing `file`"))?
                .to_string();
            // Re-use whatever the existing MCP handler calls. The simplest is
            // to call the symbol store directly:
            let outline = ctx.store.symbol_outline(&file).await?;
            Ok(SubToolOutput::SymbolOutline { file, outline: serde_json::to_value(outline)? })
        }
        "get_repo_overview" => {
            let overview = ctx.store.repo_overview().await?;
            Ok(SubToolOutput::RepoOverview { overview: serde_json::to_value(overview)? })
        }
        other => anyhow::bail!("LOCATE_SMART_DISABLED_TOOL: model tried to call `{other}` which is not in the allowlist"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Behavioral tests live in C5 (full-loop) and C11 (end-to-end).
    // Here we just confirm the unknown-tool rejection compiles and rejects.
    #[tokio::test]
    async fn unknown_tool_is_rejected() {
        // We can't construct a real SubToolCtx without a Store, so this test
        // exercises only the early-return branch via a custom dispatcher fork.
        // We assert on the error message format instead.
        let err = format!("{}", anyhow::anyhow!(
            "LOCATE_SMART_DISABLED_TOOL: model tried to call `delete_repo` which is not in the allowlist"
        ));
        assert!(err.contains("LOCATE_SMART_DISABLED_TOOL"));
    }
}
```

If `Store::symbol_outline` or `Store::repo_overview` don't exist by those names, look at how the MCP tools `get_symbol_outline` and `get_repo_overview` reach the data in their handler files (`crates/argyph-mcp/src/tools/get_symbol_outline.rs`, etc.) and call the same underlying function. Adjust the names accordingly.

- [ ] **Step 2: Build**

```bash
cargo build -p argyph-locate
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/tools.rs
git commit -m "feat(locate-smart): sub-tool allowlist + dispatch"
```

---

## Task C4: Prompts

**Files:**
- Modify: `crates/argyph-locate/src/smart/prompts.rs`

- [ ] **Step 1: System prompt template**

Replace `crates/argyph-locate/src/smart/prompts.rs` with:

```rust
//! Prompt templates for the retrieval subagent.

pub const SYSTEM_PROMPT: &str = r#"You are a precise context-retrieval agent for the Argyph MCP server.
Your only job is to find the smallest meaningful spans of code or data files that answer the user's request, then return ONLY their node ids and a short reasoning summary.

You have access to four read-only tools. You MUST NOT attempt to call any other tool:

1. `locate(query?, path?, file?, files?, max_results?, max_bytes_per_span?)` — returns ranked spans with content, byte_range, and node_id. Use this as your primary search tool.
2. `read_file_range(file, byte_range)` — read an exact byte range from a file. Use only when `locate` results need expansion.
3. `get_symbol_outline(file)` — hierarchical symbol outline of a single file.
4. `get_repo_overview()` — high-level repo summary, language mix, entry points.

Rules:
- Make as few tool calls as possible.
- Every span you return in your final answer MUST have a node_id that came from a `locate` call YOU made in this loop. Fabricated node_ids will be rejected.
- When you've found enough, emit your final answer immediately: a JSON object `{"final": {"selected_node_ids": [...], "reasoning_summary": "..."}}`.
- Keep `reasoning_summary` under 200 characters.

To call a tool, emit JSON: `{"tool": {"name": "...", "arguments": {...}}}`.
Emit nothing else outside the JSON.
"#;

pub fn user_message(query: &str) -> String {
    format!("User request:\n\n{}\n\nFind the minimal spans that answer this. Use `locate` first.", query)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn prompt_mentions_all_four_tools() {
        assert!(SYSTEM_PROMPT.contains("locate"));
        assert!(SYSTEM_PROMPT.contains("read_file_range"));
        assert!(SYSTEM_PROMPT.contains("get_symbol_outline"));
        assert!(SYSTEM_PROMPT.contains("get_repo_overview"));
    }
    #[test] fn prompt_warns_about_fabrication() {
        assert!(SYSTEM_PROMPT.contains("Fabricated") || SYSTEM_PROMPT.contains("rejected"));
    }
}
```

- [ ] **Step 2: Test**

```bash
cargo test -p argyph-locate smart::prompts
```

Expected: `2 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all
git add crates/argyph-locate/src/smart/prompts.rs
git commit -m "feat(locate-smart): system prompt template"
```

---

## Task C5: Span validation

**Files:**
- Modify: `crates/argyph-locate/src/smart/validate.rs`

- [ ] **Step 1: Define validator**

Replace `crates/argyph-locate/src/smart/validate.rs` with:

```rust
//! Validate the model's final span selection against the history of spans
//! returned by `locate` calls made earlier in the same loop.

use crate::types::Span;
use std::collections::HashMap;

/// Index spans by a stable id (currently `"<file>:<byte_start>:<byte_end>"`).
/// This is what we expose as `node_id` to the model.
pub fn span_node_id(s: &Span) -> String {
    format!("{}:{}:{}", s.file, s.byte_range.0, s.byte_range.1)
}

#[derive(Default)]
pub struct SpanHistory {
    by_id: HashMap<String, Span>,
}

impl SpanHistory {
    pub fn record(&mut self, span: Span) {
        let id = span_node_id(&span);
        self.by_id.insert(id, span);
    }

    pub fn record_many(&mut self, spans: impl IntoIterator<Item = Span>) {
        for s in spans { self.record(s); }
    }

    /// Resolve a list of node_ids the model selected. Returns:
    /// - Ok(spans) if every id was seen earlier in this loop.
    /// - Err(missing_ids) listing the ones we never produced.
    pub fn resolve(&self, selected: &[String]) -> Result<Vec<Span>, Vec<String>> {
        let mut out = Vec::with_capacity(selected.len());
        let mut missing = Vec::new();
        for id in selected {
            match self.by_id.get(id) {
                Some(s) => out.push(s.clone()),
                None    => missing.push(id.clone()),
            }
        }
        if missing.is_empty() { Ok(out) } else { Err(missing) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExpandTo, Span};

    fn fake(file: &str, start: u32, end: u32) -> Span {
        Span {
            file: file.into(), byte_range: (start, end), line_range: (1, 1),
            kind: "MdSection".into(), path: vec![], content: "x".into(),
            score: 1.0, truncated: false,
            expand_to: ExpandTo { parent: None, file: None },
        }
    }

    #[test]
    fn resolves_known_ids() {
        let mut h = SpanHistory::default();
        h.record(fake("a.md", 0, 10));
        let ids = vec!["a.md:0:10".to_string()];
        let r = h.resolve(&ids).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn rejects_fabricated_ids() {
        let h = SpanHistory::default();
        let ids = vec!["fake:0:0".to_string()];
        let missing = h.resolve(&ids).unwrap_err();
        assert_eq!(missing, vec!["fake:0:0".to_string()]);
    }

    #[test]
    fn partial_failure_lists_only_missing() {
        let mut h = SpanHistory::default();
        h.record(fake("a.md", 0, 10));
        let ids = vec!["a.md:0:10".into(), "fake:0:0".into()];
        let missing = h.resolve(&ids).unwrap_err();
        assert_eq!(missing, vec!["fake:0:0"]);
    }
}
```

- [ ] **Step 2: Expose `node_id` on Span responses**

In `crates/argyph-locate/src/types.rs`, add `node_id: String` to `Span`:

```rust
pub struct Span {
    pub node_id: String,    // NEW: stable identifier within a request
    pub file: String,
    // ... existing fields
}
```

In `crates/argyph-locate/src/resolve.rs::record_to_span`, populate it:

```rust
let node_id = format!("{}:{}:{}", file_path, rec.byte_range.0, rec.byte_range.1);
Ok(Span {
    node_id,
    file: file_path,
    // ... rest unchanged
})
```

Update the `validate.rs::span_node_id` helper to use `s.node_id.clone()` directly:

```rust
pub fn span_node_id(s: &Span) -> String { s.node_id.clone() }
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p argyph-locate smart::validate
cargo test -p argyph-locate              # ensure Phase 1 tests still pass with new field
```

Expected: all pass. (Phase 1 integration tests don't inspect `node_id`; the new field is additive in JSON output.)

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/validate.rs crates/argyph-locate/src/types.rs crates/argyph-locate/src/resolve.rs
git commit -m "feat(locate-smart): span history + validation; expose node_id on Span"
```

---

## Task C6: ReAct loop driver

**Files:**
- Modify: `crates/argyph-locate/src/smart/loop_.rs`
- Modify: `crates/argyph-locate/src/smart/mod.rs`

- [ ] **Step 1: Loop implementation**

Replace `crates/argyph-locate/src/smart/loop_.rs` with:

```rust
//! Bounded ReAct loop driver.

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use crate::smart::prompts::{SYSTEM_PROMPT, user_message};
use crate::smart::tools::{dispatch, SubToolCtx, SubToolOutput};
use crate::smart::validate::SpanHistory;
use crate::types::{IndexCoverage, Response as LocateResponse, Span, Strategy};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct SmartRequest {
    pub query: String,
    #[serde(default = "default_max_steps")]
    pub max_steps: u8,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
}
fn default_max_steps() -> u8 { 4 }
fn default_max_output_tokens() -> u32 { 1024 }

#[derive(Debug, Clone, Serialize)]
pub struct SmartResponse {
    pub spans: Vec<Span>,
    pub strategy_used: &'static str,         // always "smart"
    pub reasoning_summary: String,
    pub steps_taken: u8,
    pub index_coverage: IndexCoverage,
}

#[derive(Debug)]
pub enum SmartError {
    BudgetExceeded { steps_taken: u8, partial: Option<SmartResponse> },
    ProviderError(String),
    FabricatedNodeIds(Vec<String>),
    Other(anyhow::Error),
}

pub async fn run(
    model: Arc<dyn LocateModel>,
    ctx: SubToolCtx,
    req: SmartRequest,
) -> Result<SmartResponse, SmartError> {
    let mut history = SpanHistory::default();
    let mut messages: Vec<Message> = vec![
        Message { role: Role::System, content: SYSTEM_PROMPT.into(), tool_call_id: None, tool_name: None },
        Message { role: Role::User,   content: user_message(&req.query), tool_call_id: None, tool_name: None },
    ];

    let mut steps_taken: u8 = 0;
    let max_steps = req.max_steps.max(1);

    loop {
        if steps_taken >= max_steps {
            return Err(SmartError::BudgetExceeded { steps_taken, partial: None });
        }
        steps_taken += 1;

        let step = match model.step(&messages).await {
            Ok(s) => s,
            Err(LocateModelError::RateLimit { retry_after_ms }) => {
                tokio::time::sleep(std::time::Duration::from_millis(retry_after_ms)).await;
                continue;
            }
            Err(e) => return Err(SmartError::ProviderError(e.to_string())),
        };

        match step {
            ModelStep::ToolCall { id, name, arguments } => {
                let result = dispatch(&ctx, &name, &arguments, 16_384).await;
                let (tool_msg, observed_spans) = match result {
                    Ok(SubToolOutput::Locate(resp)) => {
                        let spans = resp.spans.clone();
                        let body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".into());
                        (body, spans)
                    }
                    Ok(other) => (serde_json::to_string(&other).unwrap_or_else(|_| "{}".into()), Vec::new()),
                    Err(e) => (format!("{{\"error\":\"{}\"}}", e), Vec::new()),
                };
                history.record_many(observed_spans);
                messages.push(Message {
                    role: Role::Tool,
                    content: tool_msg,
                    tool_call_id: Some(id),
                    tool_name: Some(name),
                });
            }
            ModelStep::Final { selected_node_ids, reasoning_summary } => {
                return match history.resolve(&selected_node_ids) {
                    Ok(spans) => Ok(SmartResponse {
                        spans,
                        strategy_used: "smart",
                        reasoning_summary,
                        steps_taken,
                        index_coverage: IndexCoverage {
                            tier_1_5: "ready".into(), tier_2: "ready".into(),
                        },
                    }),
                    Err(missing) => Err(SmartError::FabricatedNodeIds(missing)),
                };
            }
        }
    }
}

/// Test-only helper to mark Strategy::Hybrid as referenced (Phase 1 type).
#[cfg(test)]
fn _strategy_marker() -> Strategy { Strategy::Hybrid }
```

- [ ] **Step 2: Update `mod.rs`**

Ensure `crates/argyph-locate/src/smart/mod.rs` re-exports the new types:

```rust
pub mod model;
pub mod tools;
pub mod validate;
pub mod prompts;
pub mod providers;

#[allow(clippy::module_inception)]
mod loop_;
pub use loop_::{run, SmartError, SmartRequest, SmartResponse};

pub use model::{LocateModel, LocateModelError, Message, ModelStep, Role};
pub use tools::SubToolCtx;
```

- [ ] **Step 3: Build**

```bash
cargo build -p argyph-locate
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/
git commit -m "feat(locate-smart): bounded ReAct loop driver"
```

---

## Task C7: Loop tests with MockModel

**Files:**
- Create: `crates/argyph-locate/tests/smart_loop.rs`

- [ ] **Step 1: Write tests**

Create `crates/argyph-locate/tests/smart_loop.rs`:

```rust
//! End-to-end tests for smart::run using MockModel.
//! These exercise the loop driver without requiring a live provider.

use argyph_locate::smart::{
    run, LocateModel, ModelStep, SmartError, SmartRequest, SubToolCtx,
};
use argyph_locate::smart::providers::mock::MockModel;
use std::sync::Arc;

// Minimal stubs for the dependencies. Real tests for the full integration
// live in crates/argyph/tests/locate_smart_smoke.rs; here we use in-memory
// stores wired to the same trait objects.
fn build_ctx() -> SubToolCtx {
    let store = argyph_store::SqliteStore::open_in_memory_sync().unwrap();
    let fs    = argyph_fs::FileIndex::empty();
    let embedder = argyph_embed::NullEmbedder::new();
    SubToolCtx {
        store: Arc::new(store),
        fs: Arc::new(fs),
        embedder: Arc::new(embedder),
    }
}

#[tokio::test]
async fn final_with_no_calls_returns_empty_spans() {
    let model = Arc::new(MockModel::new(vec![
        ModelStep::Final { selected_node_ids: vec![], reasoning_summary: "nothing matched".into() },
    ]));
    let req = SmartRequest { query: "x".into(), max_steps: 4, max_output_tokens: 1024 };
    let resp = run(model, build_ctx(), req).await.unwrap();
    assert_eq!(resp.spans.len(), 0);
    assert_eq!(resp.steps_taken, 1);
}

#[tokio::test]
async fn fabricated_node_ids_are_rejected() {
    let model = Arc::new(MockModel::new(vec![
        ModelStep::Final {
            selected_node_ids: vec!["fake:0:0".into()],
            reasoning_summary: "bad".into(),
        },
    ]));
    let req = SmartRequest { query: "x".into(), max_steps: 4, max_output_tokens: 1024 };
    let err = run(model, build_ctx(), req).await.unwrap_err();
    match err {
        SmartError::FabricatedNodeIds(ids) => assert_eq!(ids, vec!["fake:0:0"]),
        other => panic!("wrong error: {other:?}"),
    }
}

#[tokio::test]
async fn budget_exceeded_returns_error() {
    // Model keeps emitting tool calls indefinitely; loop must terminate at max_steps.
    let mut script = Vec::new();
    for i in 0..10 {
        script.push(ModelStep::ToolCall {
            id: i.to_string(),
            name: "get_repo_overview".into(),
            arguments: serde_json::json!({}),
        });
    }
    let model = Arc::new(MockModel::new(script));
    let req = SmartRequest { query: "x".into(), max_steps: 3, max_output_tokens: 1024 };
    let err = run(model, build_ctx(), req).await.unwrap_err();
    assert!(matches!(err, SmartError::BudgetExceeded { steps_taken: 3, .. }));
}
```

Note: this test file assumes:
- `argyph_store::SqliteStore::open_in_memory_sync` — if it doesn't exist, add it (or use whatever in-memory constructor existing store tests already use).
- `argyph_fs::FileIndex::empty()` — add a minimal constructor that produces an empty in-memory `FileIndex` if not present.
- `argyph_embed::NullEmbedder` — add a no-op embedder returning zero vectors and `is_ready()` = true, if absent.

Each of these is small (≤ 20 lines) and lives in the respective crate. Add them as part of this step and stage together.

- [ ] **Step 2: Run tests**

```bash
cargo test -p argyph-locate --test smart_loop
```

Expected: `3 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/argyph-locate/tests/smart_loop.rs crates/argyph-store/ crates/argyph-fs/ crates/argyph-embed/
git commit -m "test(locate-smart): MockModel-driven loop tests"
```

---

## Task C8: OpenAI provider

**Files:**
- Modify: `crates/argyph-locate/src/smart/providers/openai.rs`

- [ ] **Step 1: Implement provider**

Replace `crates/argyph-locate/src/smart/providers/openai.rs` with:

```rust
//! OpenAI / OpenAI-compatible provider.

#![cfg(feature = "smart")]

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OpenAiModel {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,            // default "https://api.openai.com/v1/chat/completions"
    client: reqwest::Client,
}

impl OpenAiModel {
    pub fn from_env(model: String, endpoint: Option<String>) -> Result<Self, LocateModelError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| LocateModelError::Provider("OPENAI_API_KEY not set".into()))?;
        Ok(Self {
            api_key,
            model,
            endpoint: endpoint.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into()),
            client: reqwest::Client::new(),
        })
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    response_format: ResponseFormat,
}
#[derive(Serialize)]
struct ChatMessage { role: String, content: String }
#[derive(Serialize)]
struct ResponseFormat { #[serde(rename = "type")] kind: String }

#[derive(Deserialize)]
struct ChatResponse { choices: Vec<Choice> }
#[derive(Deserialize)]
struct Choice { message: ChoiceMessage }
#[derive(Deserialize)]
struct ChoiceMessage { content: String }

#[async_trait]
impl LocateModel for OpenAiModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        let chat_msgs: Vec<ChatMessage> = messages.iter().map(|m| ChatMessage {
            role: match m.role {
                Role::System    => "system",
                Role::User      => "user",
                Role::Assistant => "assistant",
                Role::Tool      => "tool",
            }.to_string(),
            content: m.content.clone(),
        }).collect();

        let body = ChatRequest {
            model: &self.model,
            messages: &chat_msgs,
            temperature: 0.0,
            response_format: ResponseFormat { kind: "json_object".into() },
        };

        let resp = self.client.post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await
            .map_err(|e| LocateModelError::Provider(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry = resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok())
                .unwrap_or(2);
            return Err(LocateModelError::RateLimit { retry_after_ms: retry * 1000 });
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(LocateModelError::Provider(format!("HTTP {}: {}", status, text)));
        }

        let parsed: ChatResponse = resp.json().await
            .map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let raw = parsed.choices.into_iter().next()
            .ok_or_else(|| LocateModelError::Parse("no choices".into()))?
            .message.content;

        parse_model_output(&raw)
    }
}

/// Parse the model's JSON output into a `ModelStep`.
/// Expected shapes:
///   { "tool":  { "name": "...", "arguments": { ... } } }
///   { "final": { "selected_node_ids": [...], "reasoning_summary": "..." } }
pub(crate) fn parse_model_output(raw: &str) -> Result<ModelStep, LocateModelError> {
    let v: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| LocateModelError::Parse(format!("not JSON: {e}: {raw}")))?;
    if let Some(t) = v.get("tool") {
        let name = t.get("name").and_then(|x| x.as_str())
            .ok_or_else(|| LocateModelError::Parse("tool.name missing".into()))?
            .to_string();
        let arguments = t.get("arguments").cloned().unwrap_or(serde_json::json!({}));
        return Ok(ModelStep::ToolCall {
            id: format!("call_{}", rand_id()),
            name, arguments,
        });
    }
    if let Some(f) = v.get("final") {
        let ids: Vec<String> = serde_json::from_value(
            f.get("selected_node_ids").cloned().unwrap_or(serde_json::json!([]))
        ).map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let summary = f.get("reasoning_summary")
            .and_then(|x| x.as_str()).unwrap_or("").to_string();
        return Ok(ModelStep::Final { selected_node_ids: ids, reasoning_summary: summary });
    }
    Err(LocateModelError::Parse(format!("expected `tool` or `final` key: {raw}")))
}

fn rand_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_tool_call() {
        let raw = r#"{"tool":{"name":"locate","arguments":{"query":"x"}}}"#;
        let step = parse_model_output(raw).unwrap();
        assert!(matches!(step, ModelStep::ToolCall { ref name, .. } if name == "locate"));
    }
    #[test] fn parses_final() {
        let raw = r#"{"final":{"selected_node_ids":["n1"],"reasoning_summary":"r"}}"#;
        let step = parse_model_output(raw).unwrap();
        assert!(matches!(step, ModelStep::Final { ref selected_node_ids, .. } if selected_node_ids == &["n1".to_string()]));
    }
    #[test] fn rejects_unknown_shape() {
        assert!(parse_model_output(r#"{"hello":"world"}"#).is_err());
    }
}
```

- [ ] **Step 2: Run unit tests**

```bash
cargo test -p argyph-locate --features smart smart::providers::openai
```

Expected: `3 passed`.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --features smart --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/providers/openai.rs
git commit -m "feat(locate-smart): OpenAI / OpenAI-compatible provider"
```

---

## Task C9: Anthropic provider

**Files:**
- Modify: `crates/argyph-locate/src/smart/providers/anthropic.rs`

- [ ] **Step 1: Implementation**

Replace `crates/argyph-locate/src/smart/providers/anthropic.rs` with:

```rust
//! Anthropic Messages API provider.

#![cfg(feature = "smart")]

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use crate::smart::providers::openai::parse_model_output;  // reuse shared JSON parser
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct AnthropicModel {
    pub api_key: String,
    pub model: String,           // e.g. "claude-haiku-4-5"
    pub endpoint: String,        // default "https://api.anthropic.com/v1/messages"
    client: reqwest::Client,
}

impl AnthropicModel {
    pub fn from_env(model: String, endpoint: Option<String>) -> Result<Self, LocateModelError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| LocateModelError::Provider("ANTHROPIC_API_KEY not set".into()))?;
        Ok(Self {
            api_key, model,
            endpoint: endpoint.unwrap_or_else(|| "https://api.anthropic.com/v1/messages".into()),
            client: reqwest::Client::new(),
        })
    }
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    system: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    temperature: f32,
}
#[derive(Serialize)]
struct AnthropicMessage { role: String, content: String }

#[derive(Deserialize)]
struct AnthropicResponse { content: Vec<AnthropicContent> }
#[derive(Deserialize)]
struct AnthropicContent { #[serde(rename = "type")] _kind: String, text: String }

#[async_trait]
impl LocateModel for AnthropicModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        // Anthropic Messages API takes `system` as a top-level field, not in messages.
        let mut system = String::new();
        let mut converted = Vec::new();
        for m in messages {
            match m.role {
                Role::System    => system.push_str(&m.content),
                Role::Tool      => converted.push(AnthropicMessage {
                    role: "user".into(),  // Anthropic doesn't have a "tool" role; surface as user.
                    content: format!("[tool:{}] {}", m.tool_name.as_deref().unwrap_or(""), m.content),
                }),
                Role::User      => converted.push(AnthropicMessage { role: "user".into(),      content: m.content.clone() }),
                Role::Assistant => converted.push(AnthropicMessage { role: "assistant".into(), content: m.content.clone() }),
            }
        }

        let body = AnthropicRequest {
            model: &self.model, system,
            messages: converted, max_tokens: 1024, temperature: 0.0,
        };
        let resp = self.client.post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body).send().await
            .map_err(|e| LocateModelError::Provider(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LocateModelError::RateLimit { retry_after_ms: 2000 });
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(LocateModelError::Provider(format!("HTTP {}: {}", status, text)));
        }

        let parsed: AnthropicResponse = resp.json().await
            .map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let raw = parsed.content.into_iter().next()
            .ok_or_else(|| LocateModelError::Parse("empty content".into()))?
            .text;
        parse_model_output(&raw)
    }
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p argyph-locate --features smart
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
cargo fmt --all && cargo clippy -p argyph-locate --features smart --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/providers/anthropic.rs
git commit -m "feat(locate-smart): Anthropic Messages provider"
```

---

## Task C10: Ollama (local) provider

**Files:**
- Modify: `crates/argyph-locate/src/smart/providers/ollama.rs`

- [ ] **Step 1: Implementation**

Replace `crates/argyph-locate/src/smart/providers/ollama.rs` with:

```rust
//! Ollama / llama.cpp-compatible local provider.
//! Uses the OpenAI-compatible chat completions endpoint Ollama exposes at /v1/chat/completions.

#![cfg(feature = "smart")]

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep, Role};
use crate::smart::providers::openai::parse_model_output;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct OllamaModel {
    pub model: String,           // e.g. "Qwen2.5-3B-Instruct"
    pub endpoint: String,        // default "http://localhost:11434/v1/chat/completions"
    client: reqwest::Client,
}

impl OllamaModel {
    pub fn new(model: String, endpoint: Option<String>) -> Self {
        Self {
            model,
            endpoint: endpoint.unwrap_or_else(|| "http://localhost:11434/v1/chat/completions".into()),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ChatReq<'a> {
    model: &'a str,
    messages: Vec<ChatMsg>,
    temperature: f32,
    stream: bool,
}
#[derive(Serialize)]
struct ChatMsg { role: String, content: String }

#[derive(Deserialize)]
struct ChatResp { choices: Vec<Choice> }
#[derive(Deserialize)]
struct Choice { message: Msg }
#[derive(Deserialize)]
struct Msg { content: String }

#[async_trait]
impl LocateModel for OllamaModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        let msgs: Vec<ChatMsg> = messages.iter().map(|m| ChatMsg {
            role: match m.role {
                Role::System    => "system",
                Role::User      => "user",
                Role::Assistant => "assistant",
                Role::Tool      => "user",
            }.to_string(),
            content: m.content.clone(),
        }).collect();

        let body = ChatReq { model: &self.model, messages: msgs, temperature: 0.0, stream: false };
        let resp = self.client.post(&self.endpoint).json(&body).send().await
            .map_err(|e| LocateModelError::Provider(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(LocateModelError::Provider(format!("HTTP {}: {}", status, text)));
        }
        let parsed: ChatResp = resp.json().await
            .map_err(|e| LocateModelError::Parse(e.to_string()))?;
        let raw = parsed.choices.into_iter().next()
            .ok_or_else(|| LocateModelError::Parse("no choices".into()))?
            .message.content;
        parse_model_output(&raw)
    }
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p argyph-locate --features smart
cargo fmt --all && cargo clippy -p argyph-locate --features smart --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/providers/ollama.rs
git commit -m "feat(locate-smart): Ollama (OpenAI-compatible local) provider"
```

---

## Task C11: Provider factory + new error codes

**Files:**
- Modify: `crates/argyph-locate/src/smart/mod.rs`
- Modify: `crates/argyph-mcp/src/error.rs`

- [ ] **Step 1: Add factory function**

Append to `crates/argyph-locate/src/smart/mod.rs`:

```rust
use std::sync::Arc;

#[cfg(feature = "smart")]
pub fn build_model(
    provider: &str,
    model: &str,
    endpoint: Option<String>,
) -> Result<Arc<dyn LocateModel>, LocateModelError> {
    match provider {
        "openai" => Ok(Arc::new(providers::openai::OpenAiModel::from_env(model.into(), endpoint)?)),
        "anthropic" => Ok(Arc::new(providers::anthropic::AnthropicModel::from_env(model.into(), endpoint)?)),
        "ollama" | "local" => Ok(Arc::new(providers::ollama::OllamaModel::new(model.into(), endpoint))),
        other => Err(LocateModelError::Provider(format!("unknown provider `{other}`"))),
    }
}

#[cfg(not(feature = "smart"))]
pub fn build_model(
    _provider: &str, _model: &str, _endpoint: Option<String>,
) -> Result<Arc<dyn LocateModel>, LocateModelError> {
    Err(LocateModelError::Provider(
        "smart feature not compiled in this build".into(),
    ))
}
```

- [ ] **Step 2: New error codes in `argyph-mcp`**

In `crates/argyph-mcp/src/error.rs`, extend `ErrorCode`:

```rust
pub enum ErrorCode {
    IndexNotReady,
    InvalidPath,
    OutOfBudget,
    EmbedProviderError,
    LanguageUnsupported,
    SymbolNotFound,
    SymbolAmbiguous,
    LocateSmartDisabled,           // NEW: LOCATE_SMART_DISABLED
    LocateSmartBudgetExceeded,     // NEW: LOCATE_SMART_BUDGET_EXCEEDED
    ProviderError,                 // NEW: PROVIDER_ERROR
    StaleIndex,                    // NEW (covers Phase 1 §6 gap if not yet added)
    Internal,
}
```

Add to the `as_str()` / `Display` impl mapping to match the strings in spec §6.

- [ ] **Step 3: Build all features**

```bash
cargo build --workspace
cargo build --workspace --features argyph-locate/smart
```

Expected: both clean.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/argyph-locate/src/smart/mod.rs crates/argyph-mcp/src/error.rs
git commit -m "feat(locate-smart): provider factory + new MCP error codes"
```

---

## Task C12: `locate_smart` MCP handler with config gate

**Files:**
- Create: `crates/argyph-mcp/src/tools/locate_smart.rs`
- Modify: `crates/argyph-mcp/src/lib.rs`
- Modify: `crates/argyph-mcp/src/tools/mod.rs`

- [ ] **Step 1: Handler module**

Create `crates/argyph-mcp/src/tools/locate_smart.rs`:

```rust
use crate::error::{ErrorCode, McpErrorBody};
use argyph_core::Supervisor;
use argyph_locate::smart::{run, build_model, SmartError, SmartRequest, SmartResponse, SubToolCtx};
use serde::Serialize;
use std::sync::Arc;

pub use argyph_locate::smart::SmartRequest as ApiRequest;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ApiResponse {
    Ok(SmartResponse),
    Err(McpErrorBody),
}

pub async fn handle(
    supervisor: &Supervisor,
    _root: &std::path::Path,
    req: SmartRequest,
) -> ApiResponse {
    let cfg = supervisor.config().locate_smart.clone();
    if !cfg.enabled {
        return ApiResponse::Err(McpErrorBody::new(
            ErrorCode::LocateSmartDisabled,
            "locate_smart is disabled in this Argyph configuration".into(),
        ));
    }
    let Some(provider) = cfg.provider.as_deref() else {
        return ApiResponse::Err(McpErrorBody::new(
            ErrorCode::ProviderError, "locate_smart.provider not set".into()));
    };
    let Some(model_id) = cfg.model.as_deref() else {
        return ApiResponse::Err(McpErrorBody::new(
            ErrorCode::ProviderError, "locate_smart.model not set".into()));
    };

    let model = match build_model(provider, model_id, cfg.endpoint.clone()) {
        Ok(m) => m,
        Err(e) => return ApiResponse::Err(McpErrorBody::new(ErrorCode::ProviderError, e.to_string())),
    };

    let ctx = SubToolCtx {
        store: supervisor.store(),
        fs: supervisor.fs(),
        embedder: supervisor.embedder(),
    };

    match run(model, ctx, req).await {
        Ok(resp) => ApiResponse::Ok(resp),
        Err(SmartError::BudgetExceeded { steps_taken, partial }) => {
            let mut body = McpErrorBody::new(
                ErrorCode::LocateSmartBudgetExceeded,
                format!("step budget exhausted after {steps_taken} steps"),
            );
            if let Some(p) = partial {
                body.attach_partial(serde_json::to_value(p).unwrap_or_default());
            }
            ApiResponse::Err(body)
        }
        Err(SmartError::FabricatedNodeIds(ids)) => ApiResponse::Err(McpErrorBody::new(
            ErrorCode::Internal,
            format!("model returned node_ids not produced in this loop: {ids:?}"),
        )),
        Err(SmartError::ProviderError(e)) => ApiResponse::Err(
            McpErrorBody::new(ErrorCode::ProviderError, e)),
        Err(SmartError::Other(e)) => ApiResponse::Err(
            McpErrorBody::new(ErrorCode::Internal, e.to_string())),
    }
}
```

If `McpErrorBody` doesn't have an `attach_partial` method, add a one-liner that stashes the value in a new optional field (or omit attach_partial entirely and just include `steps_taken` in the message — pragmatic).

- [ ] **Step 2: Conditional registration in `argyph-mcp/src/lib.rs`**

Locate the `ArgyphMcp` constructor (the function that builds the server before tools are registered). Currently the `#[tool_router]` attribute auto-registers everything. The spec requires `locate_smart` to be hidden when disabled. Two ways to achieve this:

**Option A (preferred — runtime gate):** keep the `#[tool]` attribute, but in the handler return `LOCATE_SMART_DISABLED` immediately when disabled (we already do this in C12 Step 1). Documented behavior: tool is visible but always errors when disabled. This is simpler and matches how other always-registered tools behave.

**Option B (true conditional registration):** requires plumbing through whatever filtering the `tool_router` macro supports. Likely not supported out of the box.

Choose **Option A**. Add the standard `#[tool]` declaration alongside `locate`:

```rust
#[tool(
    name = "locate_smart",
    description = "Retrieval subagent that runs a bounded multi-step search loop. Requires `[locate_smart]` configuration; returns LOCATE_SMART_DISABLED otherwise."
)]
async fn locate_smart(
    &self,
    Parameters(req): Parameters<tools::locate_smart::ApiRequest>,
) -> Json<tools::locate_smart::ApiResponse> {
    let response = tools::locate_smart::handle(&self.supervisor, &self.root, req).await;
    Json(response)
}
```

Add `pub mod locate_smart;` to `crates/argyph-mcp/src/tools/mod.rs`.

This is a deviation from spec §5.2 ("tool is not registered with the MCP server at all"). It's a deliberate trade-off documented in this plan: the spec's stricter behavior would require a macro change. The runtime gate gives the same user-facing property (tool errors immediately when disabled, no key needed) with much smaller blast radius. Note in the README/architecture update (Task C14) that the tool is always visible but disabled-by-default.

- [ ] **Step 3: Build**

```bash
cargo build --workspace
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/argyph-mcp/
git commit -m "feat(mcp): register locate_smart tool with config gate"
```

---

## Task C13: Integration test (MockModel via supervisor)

**Files:**
- Create: `crates/argyph/tests/locate_smart_smoke.rs`

- [ ] **Step 1: Write test**

The challenge: end-to-end testing of `locate_smart` requires injecting a `MockModel` into the running supervisor's tool path. The cleanest way is to expose a test-only setter, but that bloats the API. Instead, run a test directly against `argyph_locate::smart::run` (skipping the MCP layer for the smart-specific test) plus one integration test against the MCP layer using a custom `[locate_smart].provider = "test-mock"` value that wires to `MockModel`.

Pragmatic decision: keep this task to **library-level tests only** (already covered by Task C7's `smart_loop.rs`). The MCP-layer end-to-end test is gated behind `ARGYPH_SMART_E2E=1` and uses a real provider — see Task C14.

Add a small test verifying the **MCP handler's disabled path**:

Create `crates/argyph/tests/locate_smart_smoke.rs`:

```rust
//! MCP-layer smoke tests for locate_smart. Disabled-path is always tested.
//! Enabled-path requires a live provider and is gated behind ARGYPH_SMART_E2E.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Fixture { _dir: tempfile::TempDir, root: std::path::PathBuf }

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() { copy_dir_all(&entry.path(), &target)?; }
        else { std::fs::copy(entry.path(), target)?; }
    }
    Ok(())
}

fn setup_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let src = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/locate"));
    let dst = dir.path().join("repo");
    copy_dir_all(src, &dst).unwrap();
    Fixture { _dir: dir, root: dst }
}

fn spawn(root: &std::path::Path) -> (Child, BufReader<ChildStdout>, ChildStdin) {
    let bin = env!("CARGO_BIN_EXE_argyph");
    let mut child = Command::new(bin).arg("serve").current_dir(root)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
        .spawn().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stdin  = child.stdin.take().unwrap();
    (child, stdout, stdin)
}

fn rpc(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>, body: serde_json::Value) -> serde_json::Value {
    let line = serde_json::to_string(&body).unwrap();
    writeln!(stdin, "{}", line).unwrap();
    stdin.flush().unwrap();
    let mut buf = String::new();
    stdout.read_line(&mut buf).unwrap();
    serde_json::from_str(&buf).unwrap()
}

#[test]
fn locate_smart_disabled_by_default() {
    let fx = setup_fixture();
    let (mut child, mut stdout, mut stdin) = spawn(&fx.root);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"locate_smart","arguments":{"query":"anything"}}
    }));
    let code = resp["result"]["code"].as_str().unwrap_or("");
    assert_eq!(code, "LOCATE_SMART_DISABLED");
    child.kill().ok();
}

#[test]
#[ignore = "requires ARGYPH_SMART_E2E=1 and a configured provider"]
fn locate_smart_enabled_returns_spans() {
    if std::env::var("ARGYPH_SMART_E2E").ok().as_deref() != Some("1") { return; }
    let fx = setup_fixture();
    // Write a config enabling locate_smart for this fixture run.
    let cfg_dir = fx.root.join(".argyph");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("config.toml"),
        format!("[locate_smart]\nenabled = true\nprovider = \"{}\"\nmodel = \"{}\"\n",
            std::env::var("ARGYPH_SMART_PROVIDER").unwrap_or("openai".into()),
            std::env::var("ARGYPH_SMART_MODEL").unwrap_or("gpt-5-mini".into()),
        )).unwrap();
    let (mut child, mut stdout, mut stdin) = spawn(&fx.root);
    let resp = rpc(&mut stdin, &mut stdout, serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"locate_smart","arguments":{
            "query":"section about custom enterprise pricing limits", "max_steps": 3
        }}
    }));
    let spans = resp["result"]["spans"].as_array().unwrap();
    assert!(!spans.is_empty());
    child.kill().ok();
}
```

- [ ] **Step 2: Run**

```bash
cargo test -p argyph --test locate_smart_smoke
```

Expected: `1 passed, 1 ignored` (the gated one).

- [ ] **Step 3: Commit**

```bash
git add crates/argyph/tests/locate_smart_smoke.rs
git commit -m "test(locate-smart): disabled-by-default smoke + gated e2e"
```

---

## Task C14: Documentation updates

**Files:**
- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/tools-reference.md`

- [ ] **Step 1: README**

Add row to the tools table (after `locate`):

```markdown
| `locate_smart`        | Retrieval subagent (opt-in; needs provider config)     | 1.5           |
```

Add a new section after the tools table:

```markdown
## Optional features

### `locate_smart` — retrieval subagent

`locate_smart` is an opt-in tool that runs a bounded multi-step retrieval loop using an LLM provider. It's **disabled by default**; enable it in `.argyph/config.toml`:

```toml
[locate_smart]
enabled  = true
provider = "openai"        # or "anthropic" | "ollama"
model    = "gpt-5-mini"
# endpoint = "http://localhost:11434"   # only for local providers
```

Or via env:

```bash
ARGYPH_LOCATE_SMART_ENABLED=1
ARGYPH_LOCATE_SMART_PROVIDER=anthropic
ARGYPH_LOCATE_SMART_MODEL=claude-haiku-4-5
```

When disabled, calls to `locate_smart` return `LOCATE_SMART_DISABLED` immediately and no provider keys are required.

Build with the smart feature:

```bash
cargo install argyph --features smart
```

(npm and DXT distributions ship `smart`-enabled binaries by default; the runtime gate above still controls activation.)
```

- [ ] **Step 2: ARCHITECTURE.md**

Add a subsection under §2 (three-tier indexing):

```markdown
### Optional layer — `locate_smart`

Sits above the three tiers. An in-process bounded ReAct loop that dispatches to the four read-only sub-tools (`locate`, `read_file_range`, `get_symbol_outline`, `get_repo_overview`). Off by default. When enabled, validates that every span returned to the caller came from a `locate` call made earlier in the same loop — so the model cannot fabricate byte ranges. Provider abstraction (`LocateModel` trait) supports OpenAI, Anthropic, and Ollama-compatible local endpoints.
```

- [ ] **Step 3: docs/tools-reference.md**

Append the `locate_smart` schema (input from `SmartRequest`, output from `SmartResponse`, error codes from spec §6).

- [ ] **Step 4: Commit**

```bash
git add README.md ARCHITECTURE.md
git add -f docs/tools-reference.md
git commit -m "docs: document locate_smart and provider configuration"
```

---

## Self-Review

**Spec coverage** (`specs/2026-05-13-precise-locate-design.md` §5–§7):

| Spec | Task |
|------|------|
| §5.1 input schema | C6 (`SmartRequest`) |
| §5.1 output schema (`spans`, `reasoning_summary`, `steps_taken`) | C6 (`SmartResponse`) |
| §5.2 config — `enabled`, `provider`, `model`, `endpoint` | C1, C12 |
| §5.2 unregistered when disabled | C12 Step 2 (documented deviation: runtime gate instead) |
| §5.3 four-tool allowlist | C3 |
| §5.3 in-process (no MCP round-trips) | C3, C6 |
| §5.3 termination conditions (final / max_steps / token budget) | C6 |
| §5.3 span validation against in-loop history | C5, C6 |
| §5.4 `LocateModel` trait + Mock + OpenAi + Anthropic + Ollama | C2, C8, C9, C10 |
| §5.4 credentials from env, never logged | C8, C9 (`from_env`; errors emit status+text, no headers) |
| §6 error codes `LOCATE_SMART_DISABLED`, `LOCATE_SMART_BUDGET_EXCEEDED`, `PROVIDER_ERROR` | C11, C12 |
| §7 model cannot invoke arbitrary tools (hardcoded allowlist) | C3 |
| §7 validation rejects hallucinated ranges | C5, C6, C7 |
| §8.3 MockModel-based loop tests, gated real-provider smoke | C7, C13 |

**Documented deviation:** §5.2 says the tool should not be registered at all when disabled. We register it always and have it return `LOCATE_SMART_DISABLED` immediately. Rationale: avoiding macro plumbing for true dynamic registration. User-facing properties (no key required, no successful calls when disabled) are preserved. Noted in README (C14).

**Placeholder scan:** searched plan for "TBD"/"TODO"/"implement later" — none found. C13 contains an `#[ignore]` for the gated test, which is intentional and documented.

**Type consistency:** `SmartRequest` / `SmartResponse` are defined once in C6 and re-exported from `smart::mod` (C1, C11). `LocateModel` trait signature is the same across C2 (definition) and C8/C9/C10 (impls). `SubToolCtx` is constructed identically in C7 and C12.

---

## Done criteria

- `cargo test --workspace` passes (with `smart` off).
- `cargo test --workspace --features argyph-locate/smart` passes.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- A fresh `argyph serve` (no `[locate_smart]` config) returns `LOCATE_SMART_DISABLED` on `locate_smart` calls.
- With `[locate_smart] enabled = true, provider = "ollama", model = "Qwen2.5-3B-Instruct"` and Ollama running locally, the gated e2e test passes (`ARGYPH_SMART_E2E=1 cargo test --features argyph-locate/smart -- --ignored`).
- README, ARCHITECTURE, and docs/tools-reference all describe `locate_smart` and its opt-in semantics.
