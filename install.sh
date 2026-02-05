#!/bin/sh
set -e

REPO="denyzhirkov/tsk"
INSTALL_DIR="$HOME/.local/bin"
GLOBAL_BIN="/usr/local/bin"
PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
COMPLETIONS_URL="https://raw.githubusercontent.com/${REPO}/master/completions"

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
download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        echo "Error: curl or wget required"
        exit 1
    fi
}

download "$LATEST_URL" "$INSTALL_DIR/tsk"

# Make executable
chmod +x "$INSTALL_DIR/tsk"

# Remove quarantine attribute on macOS (prevents "killed" error)
if [ "$OS" = "darwin" ]; then
    xattr -cr "$INSTALL_DIR/tsk" 2>/dev/null || true
fi

echo "Installed to $INSTALL_DIR/tsk"

# Create symlink in /usr/local/bin for MCP server compatibility
# (IDEs like VS Code may not have ~/.local/bin in PATH)
create_global_symlink() {
    if [ -d "$GLOBAL_BIN" ]; then
        if [ -w "$GLOBAL_BIN" ]; then
            ln -sf "$INSTALL_DIR/tsk" "$GLOBAL_BIN/tsk"
            echo "Created symlink: $GLOBAL_BIN/tsk"
        elif command -v sudo >/dev/null 2>&1; then
            echo "Creating symlink in $GLOBAL_BIN (requires sudo)..."
            if sudo ln -sf "$INSTALL_DIR/tsk" "$GLOBAL_BIN/tsk"; then
                echo "Created symlink: $GLOBAL_BIN/tsk"
            fi
        fi
    fi
}

create_global_symlink

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

# Install shell completions
install_completions() {
    local shell="$1"
    local rc_file="$2"
    local comp_dir="$HOME/.local/share/tsk/completions"

    mkdir -p "$comp_dir"

    if download "${COMPLETIONS_URL}/tsk.${shell}" "$comp_dir/tsk.${shell}" 2>/dev/null; then
        local source_line="source $comp_dir/tsk.${shell}"
        if [ -f "$rc_file" ] && ! grep -q "tsk.${shell}" "$rc_file" 2>/dev/null; then
            echo "" >> "$rc_file"
            echo "# tsk completions" >> "$rc_file"
            echo "$source_line" >> "$rc_file"
            echo "Installed $shell completions"
        fi
    fi
}

# Detect current shell
current_shell=$(basename "$SHELL")

# Check if already in PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        echo "PATH already configured"
        ;;
    *)
        # Try to add to shell config
        added=false

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

# Install completions for current shell
if [ "$current_shell" = "zsh" ]; then
    install_completions "zsh" "$HOME/.zshrc"
elif [ "$current_shell" = "bash" ]; then
    if [ "$(uname)" = "Darwin" ]; then
        install_completions "bash" "$HOME/.bash_profile"
    else
        install_completions "bash" "$HOME/.bashrc"
    fi
fi

echo ""
echo "Run 'tsk --help' to get started"
echo "Tab completion enabled (restart terminal to activate)"
