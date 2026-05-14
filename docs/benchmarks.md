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

Measured on `m3-pro`. Run with the `argyph-benches` crate (criterion).

| Bench                         | Time     | Notes                                |
|-------------------------------|----------|--------------------------------------|
| Tree-sitter parse, `main.rs`  | < 5 ms   | tiny-rust-app fixture                |
| Directory walk                | < 1 ms   | warm cache, walkdir                  |
| Token count (cl100k_base)     | < 50 µs  | tiktoken-rs                          |
| `locate` parse_path           | < 1 µs   | criterion                            |
| `locate` strategy dispatch    | < 1 µs   | criterion                            |

These are sanity checks, not end-to-end claims.

---

## 5. End-to-end results (PENDING)

These rows are intentionally **blank** until the competitor harness lands
and runs against a fixed 1M-LOC fixture. Filling them is gating the
`v1.0.0` final release per `docs/SPEC.md` § 7.

| Repo                         | Hardware | Tier 0 cold | Tier 1 full | Sym query p99 | Semantic p50 | `locate` p99 |
|------------------------------|----------|-------------|-------------|---------------|---------------|---------------|
| `react`                       | TBD      | —           | —           | —             | —             | —             |
| `tensorflow/tensorflow`       | TBD      | —           | —           | —             | —             | —             |
| `rust-lang/rust` (subset)    | TBD      | —           | —           | —             | —             | —             |

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
