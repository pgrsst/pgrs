# Install Script Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Buat `install.sh` di root repo agar user bisa install `pgrs` via satu perintah `curl | bash`.

**Architecture:** Shell script (`#!/usr/bin/env bash`) di root repo diakses via GitHub raw URL. Script fetch versi terbaru dari GitHub API, download binary dari GitHub Releases, install ke `~/.pgrs/bin/`, dan auto-append PATH ke `.bashrc`/`.zshrc`.

**Tech Stack:** Bash, curl, GitHub Releases API

---

## File Structure

| File | Action | Tanggung jawab |
|------|--------|----------------|
| `install.sh` | Create | Script installer utama |
| `README.md` | Create | Dokumentasi instalasi dengan one-liner |

---

### Task 1: Buat `install.sh`

**Files:**
- Create: `install.sh`

- [ ] **Step 1: Buat file `install.sh` dengan konten berikut**

```bash
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

VERSION=$(echo "$API_RESPONSE" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
if [[ -z "$VERSION" ]]; then
  error "Could not parse version from GitHub API response."
fi

echo -e "Installing ${BOLD}pgrs ${VERSION}${RESET}..."

# Download binary
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"
TMP_FILE=$(mktemp)

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
EXPORT_LINE="export PATH=\"\$HOME/.pgrs/bin:\$PATH\""

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
  add_to_path "$HOME/.bashrc"
  add_to_path "$HOME/.zshrc"
fi

echo ""
echo -e "${GREEN}${BOLD}pgrs ${VERSION} installed successfully!${RESET}"
echo "Run 'source ~/.bashrc' to update your current shell."
```

- [ ] **Step 2: Beri permission executable pada script**

```bash
chmod +x install.sh
```

- [ ] **Step 3: Verifikasi script bisa diparsing tanpa error**

```bash
bash -n install.sh
```

Expected output: tidak ada output (berarti syntax valid).

- [ ] **Step 4: Commit**

```bash
git add install.sh
git commit -m "feat: add install.sh for one-liner installation"
```

---

### Task 2: Buat `README.md`

**Files:**
- Create: `README.md`

- [ ] **Step 1: Buat file `README.md`** dengan konten berikut (gunakan Write tool atau text editor):

  Heading dan section:
  - `# pgrs` — judul
  - `## Installation` — berisi one-liner curl dan instruksi source
  - `## Usage` — contoh perintah add, list, connect, delete
  - `## Connections disimpan di` — `~/.pgrs/connections.json`

  Bagian Installation:

      ## Installation

      ```bash
      curl -fsSL https://raw.githubusercontent.com/pgrsst/pgrs/main/install.sh | bash
      ```

      Setelah install, restart terminal atau jalankan:

      ```bash
      source ~/.bashrc
      ```

  Bagian Usage:

      ## Usage

      ```bash
      # Tambah koneksi
      pgrs add mydb --host=localhost --username=postgres --password=secret --database=mydb

      # List semua koneksi
      pgrs list

      # Connect ke database
      pgrs connect mydb

      # Hapus koneksi
      pgrs delete mydb
      ```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add README with installation instructions"
```

---

### Task 3: Smoke test manual install script

**Tujuan:** Verifikasi script berjalan tanpa error di environment lokal (tanpa benar-benar download dari GitHub).

- [ ] **Step 1: Test validasi curl tersedia**

```bash
bash -c 'source install.sh' 2>&1 || true
```

Karena script menggunakan `set -e`, kita cukup test bagian validasi dengan menjalankan potongan script:

```bash
bash -c '
  if ! command -v curl &>/dev/null; then echo "curl missing"; fi
  if [[ "$(uname -s)" != "Linux" ]]; then echo "not linux"; else echo "linux ok"; fi
'
```

Expected output: `linux ok`

- [ ] **Step 2: Test logika PATH setup**

```bash
bash -c '
  INSTALL_DIR="$HOME/.pgrs/bin"
  EXPORT_LINE="export PATH=\"\$HOME/.pgrs/bin:\$PATH\""
  TEST_RC=$(mktemp)
  if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    if ! grep -qF ".pgrs/bin" "$TEST_RC"; then
      echo "" >> "$TEST_RC"
      echo "# pgrs" >> "$TEST_RC"
      echo "$EXPORT_LINE" >> "$TEST_RC"
    fi
  fi
  echo "RC file contents:"
  cat "$TEST_RC"
  rm "$TEST_RC"
'
```

Expected output: menampilkan baris `export PATH="$HOME/.pgrs/bin:$PATH"`

- [ ] **Step 3: Test tidak ada duplikasi PATH jika sudah ada**

```bash
bash -c '
  INSTALL_DIR="$HOME/.pgrs/bin"
  TEST_RC=$(mktemp)
  echo "export PATH=\"\$HOME/.pgrs/bin:\$PATH\"" >> "$TEST_RC"
  
  # Simulate PATH already contains install dir
  export PATH="$INSTALL_DIR:$PATH"
  
  if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "export PATH added (should not happen)" >> "$TEST_RC"
  else
    echo "PATH already set, skip (correct)"
  fi
  rm "$TEST_RC"
'
```

Expected output: `PATH already set, skip (correct)`
