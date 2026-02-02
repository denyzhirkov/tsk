#!/bin/sh
set -e

REPO="denyzhirkov/tsk"
INSTALL_DIR="$HOME/.local/bin"
PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'

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

# Add to PATH if needed
add_to_path() {
    local file="$1"
    if [ -f "$file" ]; then
        if ! grep -q '.local/bin' "$file" 2>/dev/null; then
            echo "" >> "$file"
            echo "$PATH_LINE" >> "$file"
            echo "Added to PATH in $file"
            return 0
        fi
    fi
    return 1
}

# Check if already in PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        echo "PATH already configured"
        ;;
    *)
        # Try to add to shell config
        added=false

        # Detect current shell
        current_shell=$(basename "$SHELL")

        if [ "$current_shell" = "zsh" ]; then
            add_to_path "$HOME/.zshrc" && added=true
        elif [ "$current_shell" = "bash" ]; then
            if [ "$(uname)" = "Darwin" ]; then
                add_to_path "$HOME/.bash_profile" && added=true
            else
                add_to_path "$HOME/.bashrc" && added=true
            fi
        fi

        # Fallback to .profile
        if [ "$added" = false ]; then
            add_to_path "$HOME/.profile" && added=true
        fi

        if [ "$added" = true ]; then
            echo "Restart your terminal or run: source ~/.${current_shell}rc"
        else
            echo "Add manually: $PATH_LINE"
        fi
        ;;
esac

echo ""
echo "Run 'tsk --help' to get started"
