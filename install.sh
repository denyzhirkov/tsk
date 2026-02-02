#!/bin/sh
set -e

REPO="denyzhirkov/tsk"
INSTALL_DIR="$HOME/.local/bin"

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$OS" in
    darwin) OS="darwin" ;;
    linux) OS="linux" ;;
    *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) ARCH="x86_64" ;;
    amd64) ARCH="x86_64" ;;
    arm64) ARCH="arm64" ;;
    aarch64) ARCH="arm64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BINARY="tsk-${OS}-${ARCH}"
LATEST_URL="https://github.com/${REPO}/releases/latest/download/${BINARY}"

echo "Installing tsk (${OS}/${ARCH})..."

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download binary
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$LATEST_URL" -o "$INSTALL_DIR/tsk"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$LATEST_URL" -O "$INSTALL_DIR/tsk"
else
    echo "Error: curl or wget required"
    exit 1
fi

# Make executable
chmod +x "$INSTALL_DIR/tsk"

echo "Installed to $INSTALL_DIR/tsk"

# Check PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo ""
        echo "Add to your shell config:"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        ;;
esac

echo ""
echo "Run 'tsk --help' to get started"
