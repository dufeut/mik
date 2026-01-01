#!/bin/sh
# mik installer for macOS and Linux
# Usage: curl -LsSf https://raw.githubusercontent.com/dufeut/mik/main/install.sh | sh

set -e

REPO="dufeut/mik"
INSTALL_DIR="${MIK_INSTALL_DIR:-$HOME/.mik/bin}"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)  PLATFORM="x86_64-unknown-linux-gnu" ;;
                aarch64) PLATFORM="aarch64-unknown-linux-gnu" ;;
                *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64)  PLATFORM="x86_64-apple-darwin" ;;
                arm64)   PLATFORM="aarch64-apple-darwin" ;;
                *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $OS"
            exit 1
            ;;
    esac
}

# Get latest version from GitHub
get_latest_version() {
    VERSION=$(curl -sL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "Failed to get latest version"
        exit 1
    fi
}

# Download and install
install() {
    detect_platform
    get_latest_version

    echo "Installing mik v$VERSION for $PLATFORM..."

    DOWNLOAD_URL="https://github.com/$REPO/releases/download/v$VERSION/mik-$PLATFORM.tar.gz"
    TEMP_DIR=$(mktemp -d)
    TEMP_FILE="$TEMP_DIR/mik.tar.gz"

    echo "Downloading from $DOWNLOAD_URL..."
    curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_FILE"

    echo "Extracting..."
    tar -xzf "$TEMP_FILE" -C "$TEMP_DIR"

    echo "Installing to $INSTALL_DIR..."
    mkdir -p "$INSTALL_DIR"
    mv "$TEMP_DIR/mik" "$INSTALL_DIR/mik"
    chmod +x "$INSTALL_DIR/mik"

    rm -rf "$TEMP_DIR"

    echo ""
    echo "mik v$VERSION installed successfully!"
    echo ""
    echo "Add mik to your PATH by adding this to your shell config:"
    echo ""
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
    echo "Then restart your shell or run:"
    echo ""
    echo "  source ~/.bashrc  # or ~/.zshrc"
    echo ""
}

install
