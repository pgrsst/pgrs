# Design: `\export` REPL Command

**Date:** 2026-05-25  
**Status:** Approved

## Overview

Tambahkan command `\export {queryId} {path/to/file}` ke REPL pgrs. Command ini mengambil query dari history (`\history`), re-execute ke koneksi DB aktif, dan menulis hasilnya sebagai CSV ke path yang ditentukan.

## Syntax

```
\export <id> <path>
```

- `id` — integer, diambil dari kolom `id` di output `\history`
- `path` — path file tujuan (absolut atau relatif ke working directory)

## Alur Eksekusi

1. Parse dua token dari input setelah `\export `
2. Validasi `id` adalah angka valid (i64)
3. Cek file di `path` belum ada — error jika sudah ada
4. Lookup query dari `analytics.get_history(connection_name)`, filter by `entry.id == id`
5. Error jika id tidak ditemukan
6. Cek query bukan DML (INSERT/UPDATE/DELETE/TRUNCATE) — error jika DML
7. Re-execute query via `conn.execute(&entry.query)`
8. Tulis hasil sebagai CSV ke file (RFC 4180)
9. Print konfirmasi: `Exported N rows to <path>`

## Format CSV

- Baris pertama: header (nama kolom)
- Baris berikutnya: data rows
- Nilai di-quote dengan double-quote jika mengandung koma, newline, atau double-quote
- Double-quote di dalam nilai di-escape sebagai `""`
- Separator: koma (`,`)
- Line ending: `\n`

## Error Cases

| Kondisi | Pesan |
|---|---|
| Format command salah | `Usage: \export <id> <path>` |
| `id` bukan angka | `error: invalid id '<x>'` |
| File sudah ada | `error: file already exists: <path>` |
| Id tidak ada di history | `error: no history entry with id <id>` |
| Query adalah DML | `error: cannot export DML query` |
| DB error saat execute | `error: <db error message>` |
| Gagal tulis file | `error: could not write file: <io error>` |
| Analytics tidak tersedia | `Analytics not available.` |

## Implementasi

### Lokasi perubahan

Semua perubahan di satu file: `src/adapters/driving/repl/mod.rs`

### Fungsi baru: `handle_export`

```rust
fn handle_export(
    id: i64,
    path: &str,
    connection_name: &str,
    conn: &dyn ReplPort,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
)
```

### Helper: `write_csv`

Fungsi private kecil di `mod.rs`:

```rust
fn write_csv(result: &QueryResult, file: &mut impl Write) -> io::Result<()>
```

Tulis header + rows ke file, quote nilai yang perlu.

### REPL_COMMANDS

Tambah entry:
```rust
("\\export <id> <path>", "export query result from history to CSV file"),
```

### Parsing di loop REPL

Di blok wildcard `_ =>`, tambahkan:
```rust
} else if let Some(rest) = trimmed.strip_prefix("\\export ") {
    // parse rest → id + path, lalu panggil handle_export
}
```

## Testing

Unit tests baru di `mod.rs #[cfg(test)]`:

- `handle_export_writes_csv_for_valid_id` — happy path, verifikasi isi file CSV
- `handle_export_errors_on_existing_file` — file sudah ada, tidak di-overwrite
- `handle_export_errors_on_unknown_id` — id tidak ada di history
- `handle_export_errors_on_dml_query` — query INSERT/UPDATE/DELETE ditolak

Semua test pakai `StubDb` dan `RecordingAnalytics` yang sudah ada.

## Catatan

- `\export` re-execute query, bukan snapshot hasil lama. Data bisa berbeda kalau DB sudah berubah sejak query dijalankan.
- DML (INSERT/UPDATE/DELETE/TRUNCATE) diblokir untuk mencegah side-effect tidak disengaja.
- Tidak ada `--force` / overwrite flag — user harus hapus file secara manual.
