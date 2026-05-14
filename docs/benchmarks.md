# Benchmarks

## Methodology

All benchmarks run on a warm filesystem cache (second run of three). Hardware: Apple M3 Pro, 36 GB RAM, macOS 15. Timings are wall-clock.

## Results (tiny-rust-app fixture, 2 files)

| Benchmark | Time | Notes |
|-----------|------|-------|
| Directory walk | <1ms | Warm cache, walkdir |
| Rust parse (main.rs) | <5ms | Tree-sitter |
| Token count | <50µs | tiktoken-rs, cl100k_base |

## Comparison methodology

The `scripts/bench-against.sh` script (to be created) benchmarks Argyph against:
- claude-context (MCP tool in Claude Code)
- repomix (Node.js packing tool)
- GitNexus (if available)

Each tool indexes the same fixture and is measured for:
- Cold indexing time
- Text search latency
- Packing time

## Reproducibility

To reproduce:
```bash
cargo bench --bench core
```
