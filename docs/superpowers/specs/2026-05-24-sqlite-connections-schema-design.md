# Migrasi Connections ke SQLite — Schema V1 Rewrite

**Tanggal:** 2026-05-24  
**Scope:** Skema SQLite saja (bagian 1 dari migrasi penuh)

## Konteks

`pgrs` saat ini menyimpan named connections di `~/.pgrs/connections.json` via `FileConnectionRepository`. SQLite (`pgrs.db`) sudah dipakai untuk analytics dan schema cache. Tujuan: konsolidasi semua data ke satu file `pgrs.db`, dengan connections sebagai tabel utama yang direferensikan oleh tabel lain via FK.

## Keputusan Desain

- **Satu file SQLite** untuk connections, analytics, dan schema cache.
- **Semua tabel analytics dan schema cache** memakai `connection_id INTEGER FK` — bukan `db_name TEXT` — sehingga dua koneksi ke server berbeda dengan nama database yang sama tidak saling menimpa.
- **`ON DELETE CASCADE`** pada semua FK ke `connections(id)`: hapus koneksi otomatis bersihkan history dan schema cache terkait.
- **Tidak ada migrasi dari `connections.json`** — user mulai fresh (file JSON dan `pgrs.db` lama sudah dihapus).
- **Schema versi tetap V1** — rewrite langsung, tidak bump ke V2.

## Skema SQLite V1

```sql
CREATE TABLE IF NOT EXISTS connections (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE,
    host        TEXT    NOT NULL,
    port        INTEGER NOT NULL DEFAULT 5432,
    username    TEXT    NOT NULL,
    password    TEXT    NOT NULL,
    database    TEXT    NOT NULL,
    tls         TEXT    NOT NULL DEFAULT 'disable',
    environment TEXT,
    uuid        TEXT
);

CREATE TABLE IF NOT EXISTS query_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    query         TEXT    NOT NULL,
    executed_at   INTEGER NOT NULL,
    UNIQUE(connection_id, query)
);
CREATE INDEX IF NOT EXISTS idx_history_conn ON query_history(connection_id, executed_at);

CREATE TABLE IF NOT EXISTS table_access (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    query_id      INTEGER REFERENCES query_history(id),
    accessed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_table_access_conn ON table_access(connection_id, table_name);

CREATE TABLE IF NOT EXISTS column_access (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    column_name   TEXT    NOT NULL,
    query_id      INTEGER REFERENCES query_history(id),
    accessed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_column_access_conn ON column_access(connection_id, table_name);

CREATE TABLE IF NOT EXISTS schema_tables (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    cached_at     INTEGER NOT NULL,
    PRIMARY KEY (connection_id, table_name)
);

CREATE TABLE IF NOT EXISTS schema_columns (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    column_name   TEXT    NOT NULL,
    data_type     TEXT,
    cached_at     INTEGER NOT NULL,
    PRIMARY KEY (connection_id, table_name, column_name)
);
```

## Scope Implementasi (Tahap Ini)

1. Rewrite `SCHEMA_V1` di `sqlite_repository.rs` sesuai skema di atas.
2. Implementasi `ConnectionRepository` trait pada `SqliteRepository`.
3. Update `app.rs`: ganti `FileConnectionRepository` dengan `SqliteRepository`.
4. Update `SchemaCachePort` impl: ganti `db_name: &str` dengan `connection_name: &str`, resolve ke `connection_id` internal.
5. Update `AnalyticsPort` impl: ganti `db_name: &str` dengan `connection_name: &str`, resolve ke `connection_id` internal.

## Di Luar Scope (Nanti)

- Perubahan signature `AnalyticsPort` trait dan `repl::run` untuk pass `connection_name`.
- Shell completions membaca dari SQLite.
- Enkripsi password di SQLite.
