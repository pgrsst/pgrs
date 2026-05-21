# pgrs

A Rust CLI tool for managing named PostgreSQL connection configurations.

## Requirements

- Linux
- `psql` (PostgreSQL client) — diperlukan untuk perintah `pgrs connect`

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/pgrsst/pgrs/main/install.sh | bash
```

Setelah install, restart terminal atau jalankan:

```bash
source ~/.bashrc  # atau ~/.zshrc jika menggunakan zsh
```

## Usage

```bash
# Tambah koneksi (port default: 5432)
pgrs add mydb --host=localhost --username=postgres --password=secret --database=mydb

# Tambah koneksi dengan port custom
pgrs add mydb --host=localhost --username=postgres --password=secret --database=mydb --port=5433

# List semua koneksi
pgrs list

# Connect ke database
pgrs connect mydb

# Hapus koneksi
pgrs delete mydb
```

## Connections disimpan di

`~/.pgrs/connections.json`
