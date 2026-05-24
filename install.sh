#!/usr/bin/env bash
set -euo pipefail

REPO="pgrsst/pgrs"
INSTALL_DIR="$HOME/.pgrs/bin"
BINARY_NAME="pgrs"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BOLD='\033[1m'
RESET='\033[0m'

error() {
  echo -e "${RED}error:${RESET} $1" >&2
  exit 1
}

# Validate environment
if ! command -v curl &>/dev/null; then
  error "curl is required but not installed."
fi

# Detect OS
OS_RAW="$(uname -s)"
case "$OS_RAW" in
  Linux)  OS_NAME="linux" ;;
  Darwin) OS_NAME="darwin" ;;
  *)      error "Unsupported OS: $OS_RAW. Supported: linux, darwin." ;;
esac

# Detect architecture
ARCH_RAW="$(uname -m)"
case "$ARCH_RAW" in
  x86_64)           ARCH="amd64" ;;
  aarch64 | arm64)  ARCH="arm64" ;;
  *)                error "Unsupported architecture: $ARCH_RAW. Supported: x86_64, aarch64/arm64." ;;
esac

# Detect latest version
echo "Fetching latest version..."
API_RESPONSE=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest") \
  || error "Failed to fetch latest release from GitHub API."

VERSION=$(echo "$API_RESPONSE" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/') || true
if [[ -z "$VERSION" ]]; then
  error "Could not parse version from GitHub API response."
fi

echo -e "Installing ${BOLD}pgrs ${VERSION}${RESET} (${OS_NAME}/${ARCH})..."

# Build download URL matching the release artifact naming convention
ASSET_NAME="${BINARY_NAME}-${VERSION}-${OS_NAME}-${ARCH}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"

TMP_FILE=$(mktemp)
trap 'rm -f "$TMP_FILE"' EXIT

echo "  Downloading ${ASSET_NAME}..."
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"; then
  error "Failed to download binary from: $DOWNLOAD_URL"
fi

# Install binary
echo "  Installing to ${INSTALL_DIR}/${BINARY_NAME}"
mkdir -p "$INSTALL_DIR"
mv "$TMP_FILE" "${INSTALL_DIR}/${BINARY_NAME}"
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

# Setup PATH
EXPORT_LINE="export PATH=\"${INSTALL_DIR}:\$PATH\""

add_to_path() {
  local rc_file="$1"
  if [[ -f "$rc_file" ]]; then
    if ! grep -qF '.pgrs/bin' "$rc_file"; then
      echo "" >> "$rc_file"
      echo "# pgrs" >> "$rc_file"
      echo "$EXPORT_LINE" >> "$rc_file"
      echo "  Adding ~/.pgrs/bin to PATH in $(basename "$rc_file")"
      return 0
    fi
  fi
  return 1
}

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  UPDATED_RC=0
  if [[ -f "$HOME/.bashrc" ]]; then
    add_to_path "$HOME/.bashrc" && UPDATED_RC=1
  fi
  if [[ -f "$HOME/.zshrc" ]]; then
    add_to_path "$HOME/.zshrc" && UPDATED_RC=1
  fi
  if [[ "$UPDATED_RC" == 0 ]]; then
    echo "  Manually add to your shell config:"
    echo "    $EXPORT_LINE"
  fi
fi

echo ""
echo -e "${GREEN}${BOLD}pgrs ${VERSION} installed successfully!${RESET}"
echo "Run 'source ~/.bashrc' (or your shell's rc file) or start a new terminal."
