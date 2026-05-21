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

if [[ "$(uname -s)" != "Linux" ]]; then
  error "Only Linux is supported."
fi

# Detect latest version
echo "Fetching latest version..."
API_RESPONSE=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest") \
  || error "Failed to fetch latest release from GitHub API."

VERSION=$(echo "$API_RESPONSE" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/') || true
if [[ -z "$VERSION" ]]; then
  error "Could not parse version from GitHub API response."
fi

echo -e "Installing ${BOLD}pgrs ${VERSION}${RESET}..."

# Download binary
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"
TMP_FILE=$(mktemp)
trap 'rm -f "$TMP_FILE"' EXIT

echo "  Downloading binary..."
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"; then
  rm -f "$TMP_FILE"
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
    fi
  fi
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
