#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.pgrs/bin"
BINARY_NAME="pgrs"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BINARY_PATH="${SCRIPT_DIR}/target/release/${BINARY_NAME}"

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
if ! command -v cargo &>/dev/null; then
  error "cargo is required but not installed."
fi

if [[ "$(uname -s)" != "Linux" ]]; then
  error "Only Linux is supported."
fi

VERSION=$(cargo metadata --manifest-path "${SCRIPT_DIR}/Cargo.toml" --no-deps --format-version 1 \
  | sed -E 's/.*"version":"([^"]+)".*/\1/') || true
if [[ -z "$VERSION" ]]; then
  error "Could not parse version from Cargo metadata."
fi

echo -e "Installing ${BOLD}pgrs ${VERSION}${RESET} from local source..."

# Build binary
echo "  Building release binary..."
(
  cd "$SCRIPT_DIR"
  cargo build --release
)

if [[ ! -f "$BINARY_PATH" ]]; then
  error "Build succeeded but binary was not found at: $BINARY_PATH"
fi

# Install binary
echo "  Installing to ${INSTALL_DIR}/${BINARY_NAME}"
mkdir -p "$INSTALL_DIR"
cp "$BINARY_PATH" "${INSTALL_DIR}/${BINARY_NAME}"
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
echo -e "${GREEN}${BOLD}pgrs ${VERSION} installed successfully from local source!${RESET}"
echo "Run 'source ~/.bashrc' (or your shell's rc file) or start a new terminal."
