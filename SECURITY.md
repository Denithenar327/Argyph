# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in Argyph, **do not open a public GitHub issue**.

Instead, report it privately:

- Open a GitHub Security Advisory at <https://github.com/Ezzy1630/argyph/security/advisories/new>, or
- Email the maintainer directly. The address is in [`MAINTAINERS.md`](MAINTAINERS.md).

You should expect an acknowledgment within 72 hours. We aim to triage and respond with a remediation plan within 7 days for critical issues.

## Supported versions

| Version    | Supported          |
|------------|--------------------|
| 1.0.x      | Yes                |
| Pre-1.0    | No (alpha/beta)    |

Security fixes are backported to the most recent minor of the most recent major.

## Threat model

Argyph runs on a developer's machine with read access to the indexed repository. The threat model treats the agent as semi-trusted: agents may receive prompt-injection attacks via repo contents that try to abuse Argyph's tools.

### In scope

- Path traversal via tool arguments.
- Prompt injection through file contents (mitigated; see § "Known mitigations").
- Embedding-provider API key exposure (logging, error messages).
- Model file supply-chain (tamper detection).
- SQLite/LanceDB corruption from power loss.
- Resource exhaustion from pathological input.
- Filesystem watcher abuse.
- Dependency supply-chain.

### Out of scope

- Compromise of the underlying OS, embedding provider, or downstream agent.
- Denial of service through legitimate-but-large input (we document limits, not enforce them as security boundaries).
- Privacy considerations beyond what Argyph itself logs (we make no claims about what the agent does with returned content).

## Known mitigations

### Path traversal

Every path argument to every MCP tool passes through `validate_repo_path()`. Symlinks are resolved; paths that escape the indexed root are hard-rejected.

### Prompt injection through file contents

Argyph never executes anything from file contents. Returned chunks are wrapped in clear envelopes (`<file path="..." source="indexed-codebase">...</file>`) so well-behaved agents can recognize untrusted content. The agent-side mitigation is out of our scope, but we make the labeling unambiguous.

### API key exposure

API keys are read from environment variables only — never config files, never logged. The `ApiKey` newtype redacts to `***` in `Display` and `Debug` outputs. `argyph doctor` reports key presence, never value.

### Model file supply-chain

Bundled model files have SHA-256 checksums hardcoded in the binary. Lazy downloads verify the checksum before use. Source: HuggingFace, pinned to specific revision.

### Database corruption

SQLite WAL mode with transactional writes. LanceDB writes are transactional natively. Integrity check on startup; on failure, drop and rebuild with a clear user warning.

### Resource exhaustion

- Per-file size cap (default 5 MB; configurable).
- Per-chunk character cap.
- Parsing timeout per file.
- Concurrent embedding cap per provider.
- Total memory soft cap with backpressure.

### Watcher abuse

- Debouncer on watcher events (default 250 ms).
- Hard cap on reindex events per minute; above threshold, fall back to periodic polling.

### Dependency supply-chain

- `cargo-deny` configured with curated allowlist.
- `cargo-audit` runs in CI on every PR.
- Dependabot for security updates.
- New dependencies require explicit justification in PR description.

## What Argyph does NOT do (intentionally)

- ❌ No shell execution as an MCP tool.
- ❌ No code editing (read-only is an architectural rule).
- ❌ No network requests beyond configured embedding providers.
- ❌ No indexing of repos outside the configured root.
- ❌ No runtime-loaded language packs.

These omissions are not features that haven't been built; they are deliberate security choices. Pull requests proposing any of them should expect a "no" with a pointer to this document.

## Disclosure policy

We follow coordinated disclosure:

1. Reporter contacts the maintainer privately.
2. Maintainer acknowledges within 72 hours.
3. Triage and remediation plan within 7 days for critical issues.
4. Fix prepared in a private branch, with a CVE requested if applicable.
5. Fix released; advisory published.
6. Reporter is credited in the advisory unless they request anonymity.

We do not currently offer a bug bounty program.
