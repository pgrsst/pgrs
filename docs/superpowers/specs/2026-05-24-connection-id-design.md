# Connection ID Feature Design

**Date:** 2026-05-24

## Overview

Tambah field `id` ke `Connection` — short 8-character hex string yang di-generate saat koneksi dibuat. ID bisa dipakai sebagai pengganti `name` di semua perintah CLI.

## Data Layer

- `Connection` struct ditambah `id: Option<String>` dengan `#[serde(default)]`
- Koneksi lama di `connections.json` tanpa field `id` deserialisasi ke `None` — tidak ada migrasi
- ID di-generate di `ConnectionService::add_connection()`, bukan di domain struct
- Generator: baca 4 bytes dari `/dev/urandom`, format sebagai lowercase hex 8 karakter (contoh: `a3f9c2d1`)
- ID tidak diekspos di `EditConnectionInput` — tidak bisa diubah setelah dibuat

## Lookup & Commands

Semua perintah yang menerima `<name>` (`connect`, `shell`, `edit`, `delete`, `rename`) juga menerima `<id>`.

Urutan lookup di `ConnectionService`:
1. Exact match pada `id == Some(input)`
2. Fallback exact match pada `name == input`
3. Error jika tidak ditemukan

Method baru `find_connection(input: &str) -> Result<Connection, String>` di `ConnectionService` menggantikan `get_connection` di semua command handler. `get_connection` (lookup by name only) tetap ada untuk internal use.

## Display

Kolom `ID` ditambahkan paling kiri di output `list`:

```
ID        NAME    HOST       PORT    DATABASE  ENV  USER  TLS      PASSWORD
a3f9c2d1  prod    localhost  5432    mydb      -    admin disable  ****
-         old     db.host    5432    legacy    -    root  disable  ****
```

- Koneksi tanpa ID tampil `-`
- Lebar kolom ID fixed 8 karakter
- `--names-only` tidak berubah
