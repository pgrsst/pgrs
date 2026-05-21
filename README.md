# pgrs

A Rust CLI tool for managing named PostgreSQL connection configurations.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/pgrsst/pgrs/main/install.sh | bash
```

Setelah install, restart terminal atau jalankan:

```bash
source ~/.bashrc
```

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

## Connections disimpan di

`~/.pgrs/connections.json`
