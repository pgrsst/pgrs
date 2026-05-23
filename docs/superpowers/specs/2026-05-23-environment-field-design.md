# Design: Environment Field on Connection

**Date:** 2026-05-23  
**Status:** Approved

## Overview

Tambah field `environment` (opsional) ke `Connection` untuk memberi label konteks koneksi (contoh: `production`, `staging`, `dev`). Field ini ditampilkan di REPL prompt dan kolom `list`.

## Data Model

`Connection` di `src/core/domain/connection.rs` ditambah satu field:

```rust
#[serde(default)]
pub environment: Option<String>,
```

`#[serde(default)]` memastikan koneksi lama yang tidak punya key ini tetap valid — di-deserialisasi sebagai `None`.

## Service Layer

`AddConnectionInput` tambah:
```rust
pub environment: Option<String>,
```

`EditConnectionInput` tambah:
```rust
pub environment: Option<Option<String>>,
```
`None` = tidak diubah, `Some(None)` = hapus env, `Some(Some("prod"))` = set env.

## CLI

### `add`
Flag opsional: `--env=<value>`. Kalau tidak diberikan, `environment` di-set ke `None`.

### `edit`
Flag opsional: `--env=<value>`. `--env=` (kosong) menghapus env yang sudah ada (`Some(None)`). Tidak memberikan flag berarti tidak ada perubahan (`None`).

### `list`
Tambah kolom `ENV` setelah kolom `DATABASE`. Kalau kosong, tampilkan string kosong.

## REPL Prompt

`PgrsPrompt` di `src/adapters/driving/repl/mod.rs` ditambah field `env: Option<String>`.

Format `render_prompt_left`:
- Ada env: `pgrs(mydb:production)`
- Tidak ada: `pgrs(mydb)`

`repl::run` signature berubah dari `db_name: &str` menjadi `db_name: &str, environment: Option<&str>`.

`app.rs::run_shell` meneruskan `conn.environment.as_deref()`.

## Backward Compatibility

- Koneksi lama di `~/.pgrs/connections.json` tanpa key `environment` tetap valid (default `None`).
- Tidak ada migrasi data diperlukan.

## Testing

Setiap perubahan dilindungi test unit:
- `connection.rs`: deserialisasi tanpa field `environment` → `None`
- `service.rs`: `add_connection` menyimpan env, `edit_connection` set/clear env
- `cli.rs`: `add` dengan/tanpa `--env`, `edit` dengan `--env=` untuk clear
- `repl/mod.rs`: prompt format dengan dan tanpa env
