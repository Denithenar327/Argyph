#!/usr/bin/env bash
set -euo pipefail

REPO="Ezzy1630/argyph"
VERSION="${ARGYPH_VERSION:-latest}"
INSTALL_DIR="${ARGYPH_INSTALL_DIR:-$HOME/.local/bin}"

detect_platform() {
    local os arch
    case "$(uname -s)" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *)      echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
    esac
    case "$(uname -m)" in
        arm64|aarch64) arch="aarch64" ;;
        x86_64)        arch="x86_64" ;;
        *)             echo "Unsupported arch: $(uname -m)" >&2; exit 1 ;;
    esac
    echo "${arch}-${os}"
}

PLATFORM=$(detect_platform)
BINARY_NAME="argyph"

if [[ "$PLATFORM" == *"pc-windows"* ]]; then
    BINARY_NAME="argyph.exe"
fi

echo "Installing argyph ${VERSION} for ${PLATFORM}..."

if [ "$VERSION" = "latest" ]; then
    VERSION=$(curl -s https://api.github.com/repos/${REPO}/releases/latest | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "Failed to determine latest version" >&2
        exit 1
    fi
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/argyph-${PLATFORM}.tar.gz"
BINARY_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading argyph ${VERSION}..."
cd "$TMP_DIR"

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o argyph.tar.gz 2>/dev/null || {
        echo "Downloading binary directly..."
        curl -fsSL "$BINARY_URL" -o "$BINARY_NAME"
    }
elif command -v wget >/dev/null 2>&1; then
    wget -q "$DOWNLOAD_URL" -O argyph.tar.gz 2>/dev/null || {
        echo "Downloading binary directly..."
        wget -q "$BINARY_URL" -O "$BINARY_NAME"
    }
fi

if [ -f argyph.tar.gz ]; then
    tar xzf argyph.tar.gz
fi

mkdir -p "$INSTALL_DIR"
if [ -f "$BINARY_NAME" ]; then
    cp "$BINARY_NAME" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
else
    echo "Downloaded archive did not contain argyph binary" >&2
    ls -la "$TMP_DIR" >&2
    exit 1
fi

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
