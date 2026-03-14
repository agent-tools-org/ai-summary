#!/bin/sh
# ai-summary installer — https://ai-summary.agent-tools.org
# Usage: curl -fsSL https://ai-summary.agent-tools.org/install.sh | sh
set -e

REPO="sunoj/ai-summary"
INSTALL_DIR="${AI_SUMMARY_INSTALL_DIR:-$HOME/.local/bin}"

main() {
    platform=$(detect_platform)
    version=$(latest_version)

    if [ -z "$version" ]; then
        echo "Error: could not determine latest version"
        exit 1
    fi

    echo "Installing ai-summary ${version} for ${platform}..."

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    url="https://github.com/${REPO}/releases/download/${version}/ai-summary-${platform}.tar.gz"
    echo "Downloading ${url}"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$tmpdir/ai-summary.tar.gz"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$tmpdir/ai-summary.tar.gz" "$url"
    else
        echo "Error: curl or wget required"
        exit 1
    fi

    tar xzf "$tmpdir/ai-summary.tar.gz" -C "$tmpdir"

    mkdir -p "$INSTALL_DIR"
    mv "$tmpdir/ai-summary" "$INSTALL_DIR/ai-summary"
    chmod +x "$INSTALL_DIR/ai-summary"

    echo "Installed ai-summary to ${INSTALL_DIR}/ai-summary"

    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
        echo ""
        echo "Add to your PATH:"
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi

    echo ""
    "${INSTALL_DIR}/ai-summary" --version
    echo "Run 'ai-summary init' to set up Claude Code hooks."
}

detect_platform() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Linux)  os_part="unknown-linux-gnu" ;;
        Darwin) os_part="apple-darwin" ;;
        *)      echo "Error: unsupported OS: $os" >&2; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch_part="x86_64" ;;
        aarch64|arm64) arch_part="aarch64" ;;
        *)             echo "Error: unsupported arch: $arch" >&2; exit 1 ;;
    esac

    # No aarch64-linux build yet — use x86_64 under emulation
    if [ "$arch_part" = "aarch64" ] && [ "$os_part" = "unknown-linux-gnu" ]; then
        arch_part="x86_64"
        echo "Note: no aarch64-linux build available, using x86_64" >&2
    fi

    echo "${arch_part}-${os_part}"
}

latest_version() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null
    fi | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//'
}

main
