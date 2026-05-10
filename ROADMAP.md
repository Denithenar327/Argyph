# Roadmap

The roadmap is updated after every release. Items reflect intent, not commitment. Anything in "Later" may move forward, backward, or out of scope based on what users actually need.

Detailed milestone definitions are in [`docs/BUILD_PLAN.md`](docs/BUILD_PLAN.md).

---

## Now (v1.0 — current)

- [ ] **Phase 0** — Skeleton: workspace, CI, binary entry point.
- [ ] **Phase 1** — Tier 0: filesystem index, `get_index_status`, `get_repo_overview`, `search_text`.
- [ ] **Phase 2** — Tier 1: tree-sitter symbol graph; `find_definition`, `find_references`, callers/callees, imports, outline.
- [ ] **Phase 3** — Tier 2: hybrid search; bundled local model + OpenAI + Voyage providers.
- [ ] **Phase 4** — Repo packing: XML and markdown formats.
- [ ] **Phase 5** — Distribution: prebuilt binaries, npm, cargo, DXT.
- [ ] **Phase 6** — Polish: benchmarks, hero GIF, mdBook docs.

Target: v1.0.0 ships when all of the above are complete and the success criteria in [`docs/SPEC.md`](docs/SPEC.md) § 6 are met.

---

## Next (v1.1)

- Gemini and Ollama embedding providers (separate atomic PRs).
- Additional language packs: Go, Java, Kotlin.
- Homebrew tap and `install.sh` universal installer.
- MCP Resources: `argyph://overview`, `argyph://status`, `argyph://config`.
- MCP Prompts: `explore_codebase`, `trace_symbol`, `prepare_review`.
- JSON pack format (only if requested).
- Performance pass: end-to-end benchmarks against named competitors.

---

## Later (v1.x and v2)

### Memory layer

`memory_save`, `memory_search`, `memory_list`, `memory_forget` MCP tools. Persistent agent notes about a codebase. Same SQLite storage substrate.

### Library docs

Context7-style up-to-date library documentation. Start with vendored docs (`target/doc`, `node_modules/.../README.md`); registry fetches later.

### Diff-aware tools

`pack_diff(base="main", head="HEAD")` for code review workflows.

### Better cross-file resolution

LSP-bridge prototype. When a language server is running, opportunistically use it for symbol resolution; fall back to tree-sitter heuristics otherwise. Closes the cross-file accuracy gap honestly documented in `crates/argyph-graph/MODULE.md`.

### Multi-repo workspaces

Index a set of related repos; query across them. Probably the most-requested feature for monorepo-adjacent users.

### Plugin system (v2)

Sandboxed WASM or out-of-process tool plugins. Done well, this is a real differentiator. Done badly, it kills the project. We will not build this until the v1 surface is stable and the design is clear.

### Optional remote backend (v2)

For teams that want a shared index. Same product, with a managed sync backend. This is where the monetization story would start — but the local-first product remains the canonical version.

### Code-specific embeddings

Bundle (or fine-tune) a code-specific embedding model. The `Embedder` trait makes this a drop-in.

---

## Hard non-goals

These are out of scope by design and will not be built. They are listed so contributors know not to propose them.

- ❌ Code editing or writing tools.
- ❌ Agent orchestration or task running.
- ❌ Git mutations (commits, branches, pushes).
- ❌ Language server replacement.
- ❌ Web dashboard.
- ❌ Shell execution as an MCP tool.
- ❌ User-provided runtime language packs (security: tree-sitter queries can be slow but not malicious; we still want to keep the surface tight).

---

## How items move on the roadmap

- "Now" items are tracked as milestones in [`docs/BUILD_PLAN.md`](docs/BUILD_PLAN.md) and as issues with the `milestone:vX.X` label.
- "Next" items become "Now" after the prior major minor ships and the team (or maintainer) commits to scope.
- "Later" items move forward based on user demand (issues, PRs, discussions). Three independent requests for the same feature is the rough threshold for promotion to "Next."
- Hard non-goals do not move. If you believe one should, open a discussion, not a PR.
