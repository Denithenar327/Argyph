#!/usr/bin/env bash
# Convenience wrapper around `cargo insta` for reviewing/accepting snapshot
# changes introduced by argyph-pack (and any future crate using insta).
#
# Usage:
#   scripts/snapshot-review.sh             # interactive review
#   scripts/snapshot-review.sh accept      # accept all pending snapshots
#   scripts/snapshot-review.sh reject      # reject all pending snapshots
#   scripts/snapshot-review.sh test        # run snapshot tests
set -euo pipefail

if ! command -v cargo-insta >/dev/null 2>&1; then
    echo "cargo-insta not installed. Installing..." >&2
    cargo install cargo-insta --locked
fi

CMD="${1:-review}"
case "$CMD" in
    review|accept|reject)
        cargo insta "$CMD" --workspace
        ;;
    test)
        cargo test --workspace -- snapshot
        ;;
    *)
        echo "Unknown command: $CMD" >&2
        echo "Use one of: review, accept, reject, test" >&2
        exit 1
        ;;
esac
