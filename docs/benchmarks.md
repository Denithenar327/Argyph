# Argyph Benchmarks

This file is the canonical record of Argyph's published latency and
throughput numbers. Every number here must be reproducible by running
the scripts referenced below on the **reference hardware** and pasting
the raw output.

If a number drifts by more than 15% across two runs, treat it as a
regression and open an issue before updating this file.

---

## 1. Reference hardware

All published numbers are taken on at least one of:

| Tag         | Machine                                       | OS              |
|-------------|-----------------------------------------------|-----------------|
| `m3-pro`    | Apple M3 Pro, 36 GB RAM                       | macOS 15        |
| `ryzen-7950`| AMD Ryzen 9 7950X, 64 GB RAM, NVMe SSD        | Ubuntu 24.04    |
| `m1-air`    | Apple M1 Air, 16 GB RAM                       | macOS 15        |

Every result row below carries the hardware tag it was measured on.

---

## 2. Methodology

- Warm filesystem cache (second of three consecutive runs).
- All providers in local-only mode (bundled ONNX embedder, no remote API
  calls) unless otherwise noted.
- Index built once before timing, then re-used across runs.
- `cargo build --release` and `argyph serve --release` for all timings.
- Wall-clock from inside Argyph's tracing spans (see `tracing-subscriber`
  output); cross-checked against external `hyperfine` runs.

Reproduce locally with:

```bash
cargo bench -p argyph-benches --bench core
cargo bench -p argyph-benches --bench locate
```

The full competitor harness lives at `scripts/bench-against.sh` (TBD —
not yet committed; tracked in `ROADMAP.md` under "Now").

---

## 3. Targets (from `docs/SPEC.md` § 6)

| Metric                                                          | Target  |
|------------------------------------------------------------------|---------|
| Cold start, 1M-LOC TypeScript monorepo, Tier 0 ready            | < 1 s   |
| Warm start (already indexed, ~100K files)                       | < 1 s   |
| Tier 1 (symbol graph) on 1M-LOC repo                             | < 60 s  |
| Symbol query (`find_definition`, etc.) p99                       | < 50 ms |
| Semantic search p50 latency                                      | < 100 ms|
| Total install size (binary + bundled model on first index)       | < 120 MB|
| `locate` structural-path p99                                     | < 5 ms  |
| `locate` hybrid p99 (excluding semantic-search cost)             | < 100 ms|

A claim becomes "official" only after it appears in the table in §5
below with a reproducible row.

---

## 4. Internal microbenches (current)

Measured on an Apple M-series laptop. Run with the `argyph-benches`
crate (criterion): `cargo bench --workspace`.

| Bench                         | Time      | Notes                               |
|-------------------------------|-----------|-------------------------------------|
| Directory walk (this repo)    | ~244 ms   | walkdir, cold-ish cache             |
| Token count (cl100k_base)     | ~4.4 µs   | tiktoken-rs, one source file        |
| `locate` parse_path (bare)    | ~15 ns    | criterion                           |
| `locate` parse_path (heading) | ~18 ns    | criterion                           |
| `locate` strategy dispatch    | ~19–32 ns | criterion, path-only vs scoped      |

These are sanity checks, not end-to-end claims.

---

## 5. End-to-end results

Measured on an Apple M-series laptop, macOS 15, with the
`system_bench` harness:

```bash
cargo run --release -p argyph-benches --bin system_bench -- /path/to/repo
# large repos: raise the poll cap
ARGYPH_BENCH_CAP_SECS=900 cargo run --release -p argyph-benches \
  --bin system_bench -- /path/to/repo
```

| Repo / fixture                          | Files  | LOC    | Tier 0 cold | Tier 1 full  | Tier 1.5 |
|-----------------------------------------|-------:|-------:|------------:|-------------:|---------:|
| `BurntSushi/ripgrep`                    |    215 |   ~52K |       71 ms |        6.8 s |   ~0.3 s |
| `microsoft/TypeScript` (`src/` only)    |    709 |  ~452K |       30 ms |        8.2 s |   ~0.3 s |
| `microsoft/TypeScript` (whole repo)     | 81,310 |    ~2M |       2.1 s | > 30 min ⚠️  |        — |

**Tier 0 scales linearly and stays fast** — 81K files indexed in 2.1 s.
It is the gate that matters for "useful immediately," and it is met
comfortably.

**Tier 1 on normal-to-large repos is fast.** The within-file reference
resolver in `argyph-graph::builder` was rewritten from an
`O(symbols² × text-length)` substring scan to a one-pass tokenization
into hash-set word indices, with O(1) per-pair lookups. On the
452K-LOC TypeScript compiler source this cut Tier 1 wall-clock from
**34.6 s to 8.2 s** (4.2×) with the edge count unchanged.

**Very large monorepos remain a known scaling limit.** Two
optimizations were applied and verified: the O(symbols²) edge-builder
rewrite above, and batching the per-file symbol/chunk SQL writes
(previously ~158K individual transaction commits on this repo, now a
few hundred). Both help — but on the full 81K-file TypeScript repo
(~2M LOC, dominated by ~60K tiny test fixtures) Tier 1 still does not
complete within a 30-minute window. The residual cost is raw
tree-sitter parse *volume* across 79K files plus the edge upsert,
which needs streaming/parallel indexing rather than a point fix. The
server stays fully usable throughout: Tier 0, `search_text`, and
`pack_repo` are available the whole time, and tools requiring Tier 1
return `INDEX_NOT_READY` with a `retry_after_ms` hint rather than
blocking. Monorepo-scale Tier 1 (streaming upserts + parallel parse)
is tracked in `ROADMAP.md` under "Now (v1.1 — performance)".

---

## 6. Competitive comparison (PENDING)

Argyph is benchmarked against at least three named competitors before
`v1.0.0`. The fixture, command set, and raw output are committed under
`benches/competitors/` (TBD).

| Competitor       | Surface                            | Notes                                  |
|------------------|------------------------------------|----------------------------------------|
| `claude-context` | MCP semantic search via Zilliz     | Cloud Milvus required, OpenAI key      |
| `repomix`        | One-shot repo packing              | No MCP, no incremental                 |
| `Serena`         | Local symbol search                | No semantic, no memory                 |

For each competitor we publish:
- Wall-clock for the same canonical query set.
- Memory footprint at steady state.
- Setup steps required to reach a usable first query.

---

## 7. How this file is updated

- After each `🎯 Gate` in `docs/BUILD_GUIDE.md`, re-run the harness and
  paste fresh numbers. Older numbers are kept in `git log` (don't edit
  history).
- Regressions of >15% on any row block the next release tag until either
  fixed or explicitly accepted in the CHANGELOG.
- The competitor matrix is regenerated each minor version; if a
  competitor goes inactive, it stays in the table with a date-stamped
  note rather than being removed.
