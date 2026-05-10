<!--
Thanks for contributing to Argyph. The checklist below is required.
PRs are squash-merged; the final commit message will be curated at merge time.
-->

## What this PR does



## Linked issue

Closes #

## Testing



## Checklist

- [ ] Module ownership respected — no leakage into adjacent crates (see `MODULE.md` per crate)
- [ ] Public surface documented (rustdoc on new pub items, with at least one example)
- [ ] Tests cover new behavior (unit + integration if MCP-touching)
- [ ] No new top-level workspace dependencies without prior approval
- [ ] No `unwrap()` outside tests, no `unsafe` outside the ONNX FFI module
- [ ] Errors typed at crate boundary with `thiserror`
- [ ] No regression in `criterion` benchmarks > 20% (link CI if run)
- [ ] Module files stay under 600 lines
- [ ] Commit subject follows [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, etc.)
- [ ] Attribution trailers comply with [`docs/COMMIT_CONVENTIONS.md`](../docs/COMMIT_CONVENTIONS.md) — most commits should NOT carry a `Co-authored-by: Claude` trailer
