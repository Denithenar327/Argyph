#!/usr/bin/env bash
# Update Formula/argyph.rb with the version + per-target SHA256 values
# from a tagged GitHub release. Run after the cargo-dist release workflow
# uploads the per-target tarballs.
set -euo pipefail

REPO="Ezzy1630/argyph"
FORMULA="$(cd "$(dirname "$0")/.." && pwd)/Formula/argyph.rb"

if [ ! -f "$FORMULA" ]; then
    echo "Formula not found at $FORMULA" >&2
    exit 1
fi

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "Failed to determine latest version" >&2
        exit 1
    fi
fi

STRIP_V="${VERSION#v}"
TARGETS=(
    "aarch64-apple-darwin:AARCH64_DARWIN"
    "aarch64-unknown-linux-gnu:AARCH64_LINUX"
    "x86_64-unknown-linux-gnu:X86_64_LINUX"
)
# Intel macOS (x86_64-apple-darwin) is intentionally absent —
# Homebrew falls back to `cargo install` on that target (see Formula).

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# Update version line.
sed -i.bak -E "s|^( *version )\".*\"|\1\"${STRIP_V}\"|" "$FORMULA"

for entry in "${TARGETS[@]}"; do
    target="${entry%%:*}"
    slot="${entry##*:}"

    archive="argyph-${target}.tar.xz"
    url="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"

    echo "Fetching ${url}..."
    curl -fsSL "$url" -o "$TMP_DIR/${archive}"
    SHA=$(shasum -a 256 "$TMP_DIR/${archive}" | cut -d' ' -f1)
    echo "  ${target}: ${SHA}"

    # Replace the placeholder, if the formula still carries one.
    sed -i.bak -E "s|REPLACE_WITH_${slot}_SHA256|${SHA}|" "$FORMULA"

    # Rewrite the sha256 line that immediately follows this target's url
    # line. `sed` cannot match across newlines, so use awk: when a line
    # contains this target's archive name, the next `sha256 "..."` line
    # is rewritten.
    awk -v archive="${archive}" -v sha="${SHA}" '
        index($0, archive) > 0 { pending = 1 }
        pending && /sha256 "/ {
            sub(/sha256 "[a-f0-9]*"/, "sha256 \"" sha "\"")
            pending = 0
        }
        { print }
    ' "$FORMULA" > "${FORMULA}.tmp" && mv "${FORMULA}.tmp" "$FORMULA"

    # Update URL to the current version.
    sed -i.bak -E "s|v[0-9][^/]*/${archive}|${VERSION}/${archive}|g" "$FORMULA"
done

rm -f "${FORMULA}.bak"
echo "Updated $FORMULA for ${VERSION}"
