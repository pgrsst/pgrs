# Design: SQLite Analytics & Schema Cache

**Date:** 2026-05-24
**Status:** Approved

## Overview

Tambahkan SQLite sebagai embedded storage untuk tiga kebutuhan:

1. Query history per saved db-name
2. Frekuensi tabel yang paling sering di-query
3. Frekuensi kolom yang paling sering di-query (berelasi ke tabel)
4. Cache metadata schema (tables, columns) ke disk — tidak on-the-fly saat runtime

Semua data hanya digunakan internal oleh pgrs. File database: `~/.pgrs/pgrs.db`.

---

## Arsitektur

SQLite duduk sebagai adapter baru di sisi `driven`, sejajar dengan `file_connection_repository.rs`. Dua port baru ditambahkan di `core/ports/`:

### `AnalyticsPort` trait

```rust
fn record_query(db_name: &str, query: &str, tables: &[String], columns: &[(String /*table*/, String /*column*/)]);
fn get_history(db_name: &str) -> Vec<HistoryEntry>;
fn get_frequent_tables(db_name: &str) -> Vec<(String, u64)>;
fn get_frequent_columns(db_name: &str, table: &str) -> Vec<(String, u64)>;
```

### `SchemaCachePort` trait

```rust
fn save_schema(db_name: &str, schema: &Schema);
fn load_schema(db_name: &str) -> Option<Schema>;
fn invalidate(db_name: &str);
```

### Adapter baru

`src/adapters/driven/sqlite_repository.rs` — mengimplementasikan kedua port di atas menggunakan crate `rusqlite`.

### Perubahan pada SchemaService

Saat startup REPL, `SchemaService` cek `SchemaCachePort::load_schema` terlebih dahulu. Jika cache ada → pakai langsung (tidak query Postgres). Jika tidak ada → fetch dari Postgres → simpan ke cache. Saat DDL terdeteksi atau `\refresh` → invalidate cache → fetch ulang.

---

## Database Schema

```sql
-- Riwayat query yang dieksekusi
CREATE TABLE query_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    query       TEXT    NOT NULL,
    executed_at INTEGER NOT NULL  -- unix timestamp
);
CREATE INDEX idx_history_db ON query_history(db_name, executed_at DESC);

-- Frekuensi akses per tabel
CREATE TABLE table_access (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    query_id    INTEGER REFERENCES query_history(id),
    accessed_at INTEGER NOT NULL
);
CREATE INDEX idx_table_access_db ON table_access(db_name, table_name);

-- Frekuensi akses per kolom
CREATE TABLE column_access (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    column_name TEXT    NOT NULL,
    query_id    INTEGER REFERENCES query_history(id),
    accessed_at INTEGER NOT NULL
);
CREATE INDEX idx_column_access_db ON column_access(db_name, table_name);

-- Cache metadata schema: daftar tabel
CREATE TABLE schema_tables (
    db_name    TEXT    NOT NULL,
    table_name TEXT    NOT NULL,
    cached_at  INTEGER NOT NULL,
    PRIMARY KEY (db_name, table_name)
);

-- Cache metadata schema: daftar kolom per tabel
CREATE TABLE schema_columns (
    db_name     TEXT NOT NULL,
    table_name  TEXT NOT NULL,
    column_name TEXT NOT NULL,
    data_type   TEXT,
    cached_at   INTEGER NOT NULL,
    PRIMARY KEY (db_name, table_name, column_name)
);
```

Schema version ditrack via SQLite `PRAGMA user_version` untuk mendukung migrasi di versi pgrs mendatang.

---

## Data Flow

### Query dieksekusi di REPL

```
User ketik query
→ executor.rs kirim ke Postgres
→ hasil ditampilkan ke user
→ record ke SQLite (fire-and-forget, error diabaikan):
    1. INSERT INTO query_history
    2. Extract tables dari AliasMap yang sudah ada di alias.rs
    3. INSERT INTO table_access (per tabel)
    4. Extract columns dari token SELECT/WHERE via tokenizer.rs
    5. INSERT INTO column_access (per kolom)
```

### REPL startup

```
app.rs buka koneksi ke db_name
→ load_schema(db_name) dari SQLite
→ Ada cache  → SchemaService pakai cache langsung
→ Tidak ada  → fetch dari Postgres → save_schema() → pakai
```

### DDL terdeteksi atau \refresh

```
invalidate(db_name) → fetch dari Postgres → save_schema() → reload SchemaService
```

### Akses statistik (backslash commands baru)

| Command    | Query SQLite                                                              |
|------------|---------------------------------------------------------------------------|
| `\history` | `SELECT query, executed_at FROM query_history WHERE db_name=? ORDER BY executed_at DESC LIMIT 50` |
| `\stats`   | `SELECT table_name, COUNT(*) FROM table_access WHERE db_name=? GROUP BY table_name ORDER BY COUNT(*) DESC` |
| `\stats <table>` | `SELECT column_name, COUNT(*) FROM column_access WHERE db_name=? AND table_name=? GROUP BY column_name ORDER BY COUNT(*) DESC` |

---

## Error Handling

SQLite adalah fitur pendukung, bukan jalur kritis. Prinsip: **gagal diam-diam, pgrs tetap jalan**.

| Skenario | Perilaku |
|----------|----------|
| SQLite tidak bisa dibuka (permission, disk penuh, corrupt) | Log warning ke stderr; schema di-load on-the-fly dari Postgres seperti sebelumnya |
| INSERT history/stats gagal | Silent ignore; query tetap dieksekusi dan hasil tetap ditampilkan |
| Schema cache corrupt atau stale | Fallback fetch dari Postgres, overwrite cache |
| Schema SQLite berubah antar versi pgrs | Cek `PRAGMA user_version` saat startup, jalankan migration jika berbeda |

---

## Dependency Baru

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

Feature `bundled` menyertakan SQLite sebagai static library — tidak bergantung pada `libsqlite3` sistem operasi, mempermudah distribusi binary.

---

## File yang Akan Dibuat/Diubah

| File | Perubahan |
|------|-----------|
| `Cargo.toml` | Tambah dependency `rusqlite` |
| `src/core/ports/analytics_port.rs` | Port baru |
| `src/core/ports/schema_cache_port.rs` | Port baru |
| `src/core/ports/mod.rs` | Export port baru |
| `src/adapters/driven/sqlite_repository.rs` | Adapter baru |
| `src/adapters/driven/mod.rs` | Export adapter baru |
| `src/core/services/schema/service.rs` | Integrasi schema cache |
| `src/adapters/driving/repl/executor.rs` | Record history & table/column access |
| `src/adapters/driving/repl/mod.rs` | Tambah `\history`, `\stats` commands |
| `src/app.rs` | Wire `SqliteRepository` ke service dan REPL |
