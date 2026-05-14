#!/usr/bin/env bash
# Universal installer for Argyph. Downloads the cargo-dist archive
# matching the current host, verifies the SHA256 sidecar, and installs
# the binary into $ARGYPH_INSTALL_DIR (default: $HOME/.local/bin).
set -euo pipefail

REPO="Ezzy1630/argyph"
VERSION="${ARGYPH_VERSION:-latest}"
INSTALL_DIR="${ARGYPH_INSTALL_DIR:-$HOME/.local/bin}"

detect_target() {
    local os arch
    case "$(uname -s)" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *)      echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
    esac
    case "$(uname -m)" in
        arm64|aarch64) arch="aarch64" ;;
        x86_64|amd64)  arch="x86_64" ;;
        *)             echo "Unsupported arch: $(uname -m)" >&2; exit 1 ;;
    esac
    echo "${arch}-${os}"
}

TARGET=$(detect_target)
BINARY_NAME="argyph"

if [ "$VERSION" = "latest" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
              | grep '"tag_name"' \
              | sed -E 's/.*"([^"]+)".*/\1/' || true)
    if [ -z "$VERSION" ]; then
        echo "Failed to determine latest version" >&2
        exit 1
    fi
fi

# cargo-dist (current) produces archives named:
#   argyph-<target>.tar.xz   (unix)
#   argyph-<target>.zip      (windows; not reached by this script)
# It also publishes a sibling .sha256 file with the SHA-256 of the archive.
ARCHIVE="argyph-${TARGET}.tar.xz"
BASE="https://github.com/${REPO}/releases/download/${VERSION}"
ARCHIVE_URL="${BASE}/${ARCHIVE}"
SHA_URL="${ARCHIVE_URL}.sha256"

echo "Installing argyph ${VERSION} for ${TARGET}..."

if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required" >&2
    exit 1
fi
if ! command -v tar >/dev/null 2>&1; then
    echo "tar is required" >&2
    exit 1
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT
cd "$TMP_DIR"

echo "Downloading ${ARCHIVE_URL}..."
if ! curl -fsSL "$ARCHIVE_URL" -o "$ARCHIVE"; then
    echo "Failed to download $ARCHIVE_URL" >&2
    echo "Falling back to: cargo install argyph --locked" >&2
    exit 1
fi

echo "Verifying SHA256..."
EXPECTED_SHA=$(curl -fsSL "$SHA_URL" 2>/dev/null | cut -d' ' -f1 || true)
if [ -n "$EXPECTED_SHA" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL_SHA=$(sha256sum "$ARCHIVE" | cut -d' ' -f1)
    else
        ACTUAL_SHA=$(shasum -a 256 "$ARCHIVE" | cut -d' ' -f1)
    fi
    if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
        echo "SHA256 mismatch: expected $EXPECTED_SHA got $ACTUAL_SHA" >&2
        exit 1
    fi
    echo "  ok"
else
    echo "Warning: no .sha256 sidecar found at $SHA_URL — skipping verification" >&2
fi

tar -xf "$ARCHIVE"

# Locate the binary inside the extracted tree.
EXTRACTED_BIN=$(find . -type f -name "$BINARY_NAME" -perm -u+x | head -n 1 || true)
if [ -z "$EXTRACTED_BIN" ]; then
    EXTRACTED_BIN=$(find . -type f -name "$BINARY_NAME" | head -n 1 || true)
fi
if [ -z "$EXTRACTED_BIN" ]; then
    echo "Archive did not contain a '$BINARY_NAME' binary" >&2
    ls -la
    exit 1
fi

mkdir -p "$INSTALL_DIR"
cp "$EXTRACTED_BIN" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo ""
echo "argyph ${VERSION} installed to ${INSTALL_DIR}/${BINARY_NAME}"
echo ""

if ! echo "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
    case "${SHELL:-}" in
        */zsh)  RC_FILE="$HOME/.zshrc" ;;
        */bash) RC_FILE="$HOME/.bashrc" ;;
        */fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
        *)      RC_FILE="$HOME/.profile" ;;
    esac
    echo "Add ${INSTALL_DIR} to your PATH:"
    echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ${RC_FILE}"
    echo "  source ${RC_FILE}"
fi

echo "Run 'argyph serve' to start the MCP server."
