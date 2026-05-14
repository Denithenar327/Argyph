# Precise Locate — Design

**Status:** Draft
**Date:** 2026-05-13
**Author:** Ezzy Rappeport (with Claude)

---

## 1. Motivation

Argyph's existing tools cover three retrieval shapes:

- `search_text` — regex/literal grep across the repo.
- `search_semantic` — hybrid BM25 + vector over AST-aware *code* chunks.
- `read_file_range` — bounded read by symbol range.

None of them answer the question an agent actually asks most often: **"give me the smallest slice of any file that contains this specific thing, and nothing else."** The gap is sharpest in two cases:

1. **Code** — the agent knows it wants one function but the existing tools either return a fixed chunk window or require the agent to first find the symbol and then read its range. Two round-trips for one intent.
2. **Non-code data** — a large markdown reference, a sprawling JSON config, a CSV of records. Today the agent either ripgreps and stitches lines together, or `read_file`s the whole thing and burns context. Tier 2's chunker is biased toward code.

This spec introduces a single tool — `locate` — that takes either a structured locator (heading path, JSON/YAML key path, symbol name, CSV row) or a natural-language query, and returns the smallest meaningful span(s) plus a cheap `expand_to` handle so the agent can widen the lens without re-searching.

It also introduces an opt-in `locate_smart` sibling tool: an in-process retrieval subagent that runs a bounded multi-step loop and returns a curated set of spans. `locate_smart` is off by default and requires an LLM provider to enable, preserving Argyph's "no API key required for full functionality" property.

---

## 2. Goals and non-goals

**Goals**

- Single MCP tool surface (`locate`) that returns the smallest natural unit containing the requested information, with byte and line ranges.
- First-class support for code, markdown, JSON, YAML, TOML, CSV.
- Deterministic, fast (<100 ms p99 for hybrid, <5 ms p99 for structural-path) and runs entirely locally.
- Optional `locate_smart` subagent for multi-step retrieval, opt-in and disabled by default.
- Reuse Argyph's existing storage, watcher, and incremental indexing — no parallel infrastructure.

**Non-goals**

- Replacing `search_text`, `search_semantic`, or `read_file_range`. They keep their current contracts; `locate` is the "smallest right thing" contract layered on top.
- Indexing binary files, images, or non-UTF-8 content. Same gate as Tier 1.
- Pre-indexing pathologically large single files (default threshold 10 MB). Those use a scan-on-demand path with LRU caching.
- General agent orchestration. `locate_smart` is a single-purpose retrieval loop, not a planner.

---

## 3. Architecture

### 3.1 Crate placement

| Component | Crate | Rationale |
|---|---|---|
| Structural parsers (markdown, JSON, YAML, TOML, CSV) | `argyph-parse` (new `structural` module) | Same conceptual job as code parsing: bytes → tree with byte ranges. |
| `structural_nodes` storage | `argyph-graph` | Reuses incremental invalidation and SQLite store already used for symbols. |
| Locate logic (strategy selection, ranking, expand hints, `locate_smart` loop) | `argyph-locate` (new crate) | Non-trivial composition over fs/graph/embed/store; doesn't belong in thin MCP handlers. |
| MCP tool handlers `locate`, `locate_smart` | `argyph-mcp` | Thin handlers calling `argyph-locate`, matches existing pattern. |

### 3.2 Tier 1.5 — structural index

A new indexing tier sitting between Tier 1 (symbols) and Tier 2 (embeddings). Runs in the same rayon pass as Tier 1 — most non-code files are small relative to the codebase, so the added wall-clock cost is negligible.

**Unified node shape (all file types):**

```rust
pub struct StructuralNode {
    pub id: NodeId,            // stable hash of (file_id, path_in_tree)
    pub file_id: FileId,
    pub kind: NodeKind,        // MdSection, MdParagraph, MdCodeBlock,
                               // JsonKey, YamlKey, TomlKey, CsvRow, ...
    pub label: String,         // heading text, key name, row index
    pub path: Vec<String>,     // ancestor labels: ["Pricing", "Enterprise"]
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub parent: Option<NodeId>,
    pub depth: u16,
}
```

**Per-file-type extraction:**

- **Markdown** — `pulldown-cmark`. Heading sections nest by level. Each section's `byte_range` runs from its heading start to the next sibling or ancestor heading. Fenced code blocks and tables are also nodes.
- **JSON / YAML / TOML** — full value-tree walk via `serde_json`, `serde_yaml`, `toml`. Each key becomes a node whose `byte_range` covers its entire value subtree. `path` is the dotted key path; array elements get numeric path segments.
- **CSV** — header row + one `CsvRow` per data row. `label` is the row index; `byte_range` is the line span.
- **Code** — *no new nodes*. The existing `Chunk` / `Symbol` tables are queried through a uniform adapter trait so the locate logic treats them like any other structural node.

**Storage**

New SQLite table `structural_nodes` in `.argyph/index.db`, keyed by `(file_id, content_hash)`. FTS5 virtual table over `label` and joined `path` for cheap structural search. Invalidation piggybacks on the existing file-change watcher.

**Size threshold**

Files larger than `ARGYPH_LOCATE_MAX_FILE_BYTES` (default 10 MB) are skipped during pre-indexing. They get a *scan-on-demand* path: first call parses and caches the tree in an in-memory LRU (default 64 entries). This keeps pathological data dumps from bloating the on-disk index.

### 3.3 High-level diagram

```
                ┌────────────────────────────────┐
                │  AI Agent (Claude/Codex/...)   │
                └────────────────┬───────────────┘
                                 │ MCP (locate, locate_smart)
                ┌────────────────▼───────────────┐
                │       argyph-mcp               │
                └────────────────┬───────────────┘
                                 │
                ┌────────────────▼───────────────┐
                │       argyph-locate (new)      │
                │   strategy → resolve → rank    │
                │   locate_smart ReAct loop      │
                └──┬──────────┬──────────┬───────┘
                   │          │          │
            ┌──────▼──┐  ┌────▼────┐ ┌───▼──────┐
            │ argyph- │  │ argyph- │ │ argyph-  │
            │ graph   │  │ embed   │ │ fs       │
            │ struct  │  │ search  │ │ bounded  │
            │ nodes   │  │ semantic│ │ reads    │
            └─────────┘  └─────────┘ └──────────┘
```

---

## 4. The `locate` tool

### 4.1 Input

```json
{
  "query": "string (optional)",
  "path":  "string (optional)",
  "file":  "string (optional)",
  "files": "string[] (optional, glob)",
  "max_results": 3,
  "max_bytes_per_span": 4096
}
```

- At least one of `query` / `path` must be set.
- `file` and `files` are mutually exclusive.
- `max_results` clamped to `[1, 10]`.

`path` formats:

| File kind | Format | Example |
|---|---|---|
| Markdown | heading path, ` > ` separator | `"Pricing > Enterprise > Limits"` |
| JSON/YAML/TOML | dotted key path with `[N]` for arrays | `"database.pools[0].timeout"` |
| CSV | `row:N` or `header:<col>,row:N` | `"row:42"` |
| Code | `symbol:<name>` (matches existing symbol table) | `"symbol:parseConfig"` |

### 4.2 Output

```json
{
  "spans": [
    {
      "file": "docs/billing.md",
      "byte_range": [12480, 13104],
      "line_range": [312, 338],
      "kind": "MdSection",
      "path": ["Pricing", "Enterprise", "Limits"],
      "content": "### Limits\n\nEnterprise accounts...",
      "score": 0.87,
      "truncated": false,
      "expand_to": {
        "parent": { "node_id": "n_4f...", "label": "Enterprise", "bytes": 4210 },
        "file":   { "bytes": 81234 }
      }
    }
  ],
  "strategy_used": "structural_path",
  "index_coverage": { "tier_1_5": "ready", "tier_2": "partial:73%" }
}
```

- `expand_to` is the cheap follow-up handle. The agent calls `locate` again with `path: "<node_id>"` or uses `read_file_range` with the parent's byte range — no re-search.
- `truncated: true` when the natural span exceeds `max_bytes_per_span`; we return a head slice on a safe boundary (line end for prose, never mid-token for code).
- `strategy_used ∈ { "structural_path", "structural_search", "semantic", "hybrid" }`.
- Empty result is **not** an error: `spans: []` with `strategy_used` set.

### 4.3 Span resolution algorithm

**Step 1 — file set.** `file` ⟶ singleton. `files` ⟶ glob via `argyph-fs`. Neither ⟶ all indexed files.

**Step 2 — strategy dispatch** (first applicable wins):

1. **`path` only ⟶ `structural_path`.**
   Parse by prefix:
   - `symbol:NAME` ⟶ symbols table lookup.
   - `row:N` or `header:COL,row:N` ⟶ CSV row lookup.
   - Contains ` > ` ⟶ markdown heading path.
   - Contains `.` or `[N]` and target file extension is JSON/YAML/TOML ⟶ key path.
   - Otherwise: try heading path first, then key path; first match wins.

   O(log n) SQLite query. If no match found, fall through to strategy (2) treating the `path` string as a `query`.

2. **`query` only ⟶ `hybrid`.**
   - **2a.** FTS5 match `query` against node `label` and joined `path` in `structural_nodes` ⟶ top K candidates.
   - **2b.** Existing `search_semantic` pipeline, scoped to the same file set ⟶ top K candidates.
   - **2c.** Reciprocal-rank fusion (the function already used by `search_semantic` for BM25 + vector). Take top `max_results`.
   - For each winner, map the hit back to its **enclosing `StructuralNode`** (smallest node whose `byte_range` contains the hit position). The node's range is the returned span — this is what turns a chunk hit into a clean section.

3. **`path` + `query` ⟶ scoped semantic.** `structural_path` narrows scope to a single node; `query` runs semantic search restricted to that byte range. Enables "find the part about retries, inside the API reference section."

**Step 3 — post-processing.** Truncate if needed, attach `expand_to`, read content via `argyph-fs` bounded reader (same path `read_file_range` uses).

**Step 4 — index readiness.**

- Tier 1.5 not ready ⟶ structural strategies return `INDEX_NOT_READY { retry_after_ms }`.
- Tier 2 not ready ⟶ hybrid drops the semantic leg, runs structural-search only, sets `index_coverage.tier_2: "building"`. Same degradation as `search_semantic` today.

**Edge cases:**

| Case | Behavior |
|---|---|
| Ambiguous `path` (two `## Limits`) | Return both spans, ranked depth-then-file-order. |
| Match in a file above size threshold | On-demand parse for that file, LRU-cached. |
| File mutated between index and read (content-hash mismatch) | Return `STALE_INDEX`; trigger background reindex of that file. |
| Empty match | `spans: []`, not an error. |
| Pathologically long single line (minified JSON) | Byte range correct; truncation falls back to byte boundary. |

---

## 5. The `locate_smart` tool (opt-in)

### 5.1 Surface

```json
// MCP tool: locate_smart
{
  "query": "string (required)",
  "max_steps": 4,
  "max_output_tokens": 1024
}
```

Returns the same `spans[]` shape as `locate`, plus:

```json
{
  "spans": [ ... ],
  "reasoning_summary": "Chose the section under 'Retry policy' because the surrounding text references exponential backoff.",
  "steps_taken": 3,
  "strategy_used": "smart"
}
```

### 5.2 Configuration

```toml
# .argyph/config.toml — example (default is disabled; see §10)
[locate_smart]
enabled  = true
provider = "openai" | "anthropic" | "local"
model    = "gpt-5-mini" | "claude-haiku-4-5" | "Qwen2.5-3B-Instruct"
endpoint = "..."   # for local llama.cpp / Ollama
```

If the `[locate_smart]` section is absent or `enabled = false`, **the tool is not registered with the MCP server**. Agents don't see it. A fresh install never exposes a tool that needs a key.

### 5.3 Algorithm

A bounded ReAct loop inside `argyph-locate`, gated by a `smart` Cargo feature:

1. Model receives the user's `query` plus a system prompt describing four sub-tools it can call:
   - `locate(query, path, file, files, max_results, max_bytes_per_span)`
   - `read_file_range(file, byte_range)`
   - `get_symbol_outline(file)`
   - `get_repo_overview()`
   These are crate-level function calls (not MCP round-trips) so the loop is in-process and avoids JSON-RPC overhead.
2. Loop terminates on: model emits a final `spans[]` selection; `max_steps` hit; `max_output_tokens` budget exceeded.
3. Server **validates** the final selection: every byte range must come from a `locate` call made earlier in this request's loop. The model cannot hallucinate byte ranges.
4. Returns spans + `reasoning_summary` + `steps_taken`.

### 5.4 Provider abstraction

```rust
trait LocateModel: Send + Sync {
    async fn step(&self, msgs: &[Message]) -> Result<ModelStep>;
}
```

Implementations: `OpenAiModel`, `AnthropicModel`, `LocalOllamaModel`, `MockModel` (for tests). Provider credentials read from env (`OPENAI_API_KEY` etc.); never logged, never echoed in error strings.

### 5.5 Why a separate tool, not a flag on `locate`

- Different latency profile (seconds vs ms). The calling agent should choose deliberately.
- Different failure modes (rate limits, provider outages) shouldn't pollute the deterministic tool.
- Different cost profile — caller-facing tokens become non-trivial.

---

## 6. Errors

Matches existing `argyph-mcp` conventions.

| Code | When |
|---|---|
| `INVALID_ARGUMENT` | Neither `query` nor `path` given; `file` and `files` both set; `max_results > 10`. |
| `FILE_NOT_FOUND` | `file` doesn't exist or isn't indexed. |
| `INDEX_NOT_READY` | Tier 1.5 still building; includes `retry_after_ms`. |
| `STALE_INDEX` | File mutated between index and read; reindex queued. |
| `LOCATE_SMART_DISABLED` | `locate_smart` called when `[locate_smart].enabled = false`. |
| `LOCATE_SMART_BUDGET_EXCEEDED` | Step or token budget hit before model emitted final spans; returns best-effort partial. |
| `PROVIDER_ERROR` | LLM provider failed; includes upstream error string (no credentials). |

Empty result is **not** an error.

---

## 7. Security and read-only invariants

- `locate_smart`'s model **cannot** invoke arbitrary tools. Sub-tool surface is a hardcoded allowlist: `locate`, `read_file_range`, `get_symbol_outline`, `get_repo_overview`. All read-only, all in-process.
- Server validates the model's final `spans[]` against ranges actually returned by `locate` calls in the same loop. No hallucinated byte ranges escape.
- File reads go through `argyph-fs` bounded reader — same `.gitignore` and sandbox rules as everything else.
- Provider credentials read from env only. Never logged, never serialized into error payloads.
- Symlink policy inherits from `argyph-fs`.

---

## 8. Testing

### 8.1 Unit (per crate)

- `argyph-parse::structural` — golden fixture tests, one per file type. Fixture in, expected `StructuralNode` tree out. ~15 fixtures total.
- `argyph-locate::strategy` — table-driven dispatch tests: (`path` shape, `query` presence) ⟶ expected `strategy_used`. Pure logic, no I/O.
- `argyph-locate::resolve` — fake `StructuralNode` set; tests path matching, ambiguity ranking, truncation.

### 8.2 Integration (`crates/argyph/tests/`)

Tiny repo fixture under `tests/fixtures/locate/`: code, markdown with nested sections, JSON + YAML + TOML configs, CSV. For each test: index the fixture, call `locate` with known input, assert on `spans[].file`, `path`, and `byte_range` content. One test per strategy. One test per error code.

### 8.3 `locate_smart` testing

Provider behind `LocateModel` trait. Inject `MockModel` emitting a scripted tool-call sequence and a final selection. Assert on (a) loop termination conditions, (b) span validation rejecting fabricated ranges, (c) `LOCATE_SMART_BUDGET_EXCEEDED` returning best-effort. One real-provider smoke test gated behind `ARGYPH_SMART_E2E=1` + env key — runs as an optional CI job, not by default.

### 8.4 Bench (`benches/locate.rs`, criterion)

- `locate` p50 / p99 on the 1M-LOC fixture for each strategy.
- Targets: `structural_path` < 5 ms p99; `hybrid` < 100 ms p99 (excluding the semantic search latency already benched).
- `locate_smart` benches only the in-process overhead (validation, ranking) — provider latency is out of scope.

### 8.5 Eval (`benches/eval/locate/`)

30 handcrafted Q&A pairs across file types. Score: does the returned span contain the expected text? Run manually; informs `locate_smart` prompt iteration. Not gating CI.

---

## 9. Performance targets

| Operation | p50 | p99 |
|---|---|---|
| `locate` — `structural_path` | < 1 ms | < 5 ms |
| `locate` — `structural_search` | < 10 ms | < 30 ms |
| `locate` — `hybrid` (excl. existing semantic cost) | < 30 ms | < 100 ms |
| Tier 1.5 cold-index cost on 1M-LOC repo | < 10% added on top of Tier 1 |
| `locate_smart` in-process overhead per step | < 5 ms |

---

## 10. Configuration summary

```toml
# .argyph/config.toml — all optional

[locate]
max_file_bytes      = 10_485_760   # 10 MB
on_demand_lru_size  = 64

[locate_smart]
enabled  = false                   # off by default
provider = "openai"
model    = "gpt-5-mini"
# endpoint = "http://localhost:11434"   # for local providers
```

Environment overrides: `ARGYPH_LOCATE_MAX_FILE_BYTES`, `ARGYPH_LOCATE_SMART_ENABLED`, `ARGYPH_LOCATE_SMART_PROVIDER`, `ARGYPH_LOCATE_SMART_MODEL`.

---

## 11. Rollout

1. **Tier 1.5 + structural parsers in `argyph-parse` and `argyph-graph`** — landable independently; no public tool surface yet.
2. **`argyph-locate` crate + `locate` MCP tool** — first user-visible release.
3. **`locate_smart` behind a Cargo feature flag** — ships disabled-by-default; documented in README under "Optional features."
4. **Docs** — `docs/tools-reference.md` and `ARCHITECTURE.md` updated to describe Tier 1.5 and the two new tools.

---

## 12. Open questions

None blocking. Followups to revisit after first release:

- Whether to add HTML / PDF text extraction as additional Tier 1.5 parsers.
- Whether `locate_smart` should be able to call `search_text` (regex) in addition to the current four-tool allowlist.
- Whether to expose `expand_to.parent.node_id` as a first-class `path` form (e.g., `path: "node:n_4f..."`) for round-trip cheapness.
