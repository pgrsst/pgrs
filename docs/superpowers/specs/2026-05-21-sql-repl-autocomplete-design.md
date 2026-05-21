# Design: SQL REPL & Shell Auto-completion

**Date:** 2026-05-21  
**Status:** Approved

## Overview

Dua fitur baru ditambahkan ke `pgrs`:

1. **Shell completion** — `pgrs completions <bash|zsh|fish>` menghasilkan script yang di-eval oleh shell, sehingga `pgrs connect <TAB>` otomatis menampilkan connection names yang tersimpan.
2. **SQL REPL interaktif** — `pgrs shell <name>` membuka sesi interaktif berbasis `rustyline` dengan context-aware SQL auto-completion (keyword, table names, column names).

`pgrs connect <name>` yang sudah ada tidak berubah — tetap spawn psql seperti sekarang.

## Arsitektur

```
src/
  adapters/
    driving/
      cli.rs                    — tambah subcommand: shell, completions
      repl/
        mod.rs                  — entry point REPL, setup rustyline Editor
        completer.rs            — SqlCompleter: implement rustyline Completer
        executor.rs             — jalankan query ke DB, print hasil sebagai ASCII table
      completions.rs            — bash_completion_script(), zsh_completion_script(), fish_completion_script()
    driven/
      postgres_db.rs            — PostgresDb: implementasi DbConnection via postgres crate
  core/
    ports/
      db_connection.rs          — trait DbConnection (execute_query, list_tables, list_columns)
    services/
      schema/
        mod.rs
        service.rs              — SchemaService<D>: load & cache tables/columns in-memory
```

**Dependency direction:** `cli` → `SchemaService` → `DbConnection` trait ← `PostgresDb`. Core tidak mengimport dari adapters. `postgres` crate hanya ada di layer `PostgresDb`.

## Fitur 1: Shell Completion

### Subcommand

```
pgrs completions <bash|zsh|fish>
```

Print completion script ke stdout. Tidak ada side effect lain.

### Setup user

```bash
# bash (~/.bashrc)
eval "$(pgrs completions bash)"

# zsh (~/.zshrc)
eval "$(pgrs completions zsh)"

# fish (~/.config/fish/config.fish)
pgrs completions fish | source
```

### Apa yang di-complete

| Posisi | Completion |
|--------|-----------|
| `pgrs <TAB>` | `add`, `list`, `delete`, `connect`, `shell`, `completions` |
| `pgrs connect <TAB>` | connection names (dynamic) |
| `pgrs shell <TAB>` | connection names (dynamic) |
| `pgrs delete <TAB>` | connection names (dynamic) |
| `pgrs completions <TAB>` | `bash`, `zsh`, `fish` |

Dynamic connection names diperoleh dengan memanggil `pgrs list --names-only` (flag baru yang print satu nama per baris ke stdout). Script completion memanggil flag ini secara runtime.

### Implementasi

Tiga fungsi di `completions.rs` yang return `&'static str` — script di-hardcode sebagai string literal. Tidak ada dependency eksternal untuk bagian ini.

## Fitur 2: SQL REPL Interaktif

### Subcommand

```
pgrs shell <connection-name>
```

### UX

```
Connected to 'mydb' (localhost:5432/mydb). Type \q or Ctrl+D to exit.
pgrs> SELECT * FROM us▌
                      users  user_sessions  user_logs
pgrs> SELECT id, em▌    ← 'users' sudah ada di query
                  email  email_verified
pgrs> SELECT id, email FROM users WHERE id = 1;
 id | email
----+------------------
  1 | alice@example.com
(1 row)
pgrs> \dt
 Tables
-----------
 users
 user_logs
 user_sessions
pgrs> \q
```

### Completion logic (`SqlCompleter`)

Input di-parse token demi token. Context ditentukan dari token sebelum posisi cursor:

| Context | Trigger | Suggestions |
|---------|---------|-------------|
| Keyword | awal input, atau setelah `;` | SQL keywords |
| Table name | setelah `FROM`, `JOIN`, `INTO`, `UPDATE` | table names dari SchemaService |
| Column name | setelah `SELECT`, `WHERE`, `ON`, `SET` | kolom dari table yang sudah disebut di query |
| Default | token tidak dikenali | SQL keywords |

Column completion: SqlCompleter scan query saat ini untuk mencari table names yang sudah disebut (setelah FROM/JOIN), lalu lookup kolom dari tabel tersebut di cache SchemaService.

### Schema cache (`SchemaService`)

Diload sekali saat `pgrs shell` dimulai, sebelum REPL loop berjalan:

```sql
-- list tables
SELECT table_name FROM information_schema.tables
WHERE table_schema = 'public' AND table_type = 'BASE TABLE';

-- list columns
SELECT table_name, column_name FROM information_schema.columns
WHERE table_schema = 'public'
ORDER BY table_name, ordinal_position;
```

Disimpan sebagai `HashMap<String, Vec<String>>` (table_name → column_names). Tidak di-refresh selama sesi berjalan.

### Query executor (`executor.rs`)

- Query disubmit saat user tekan Enter dan input mengandung `;`
- Multi-line: jika belum ada `;`, prompt berubah ke `   ->` dan terus terima input
- Hasil di-print sebagai ASCII table sederhana (lebar kolom auto-fit)
- Error query di-print inline, REPL tidak exit

### Meta-commands

| Command | Aksi |
|---------|------|
| `\q` atau `exit` | keluar dari REPL |
| `\dt` | list semua tables |

### `DbConnection` trait

```rust
pub trait DbConnection {
    fn execute(&self, query: &str) -> Result<QueryResult, String>;
    fn list_tables(&self) -> Result<Vec<String>, String>;
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String>;
}
```

`QueryResult` adalah struct sederhana: `columns: Vec<String>`, `rows: Vec<Vec<String>>`.

## Dependencies Baru

| Crate | Versi | Alasan |
|-------|-------|--------|
| `rustyline` | latest stable | REPL input + completion |
| `postgres` | latest stable | koneksi PostgreSQL sync |

## Error Handling

- Gagal connect saat `pgrs shell <name>` → print pesan jelas, exit non-zero, tidak panic
- Query error → print error, REPL lanjut
- Connection name tidak ditemukan → reuse error dari `ConnectionService`
- Error dari `postgres` crate di-wrap ke `String` di `PostgresDb`, tidak bocor ke core

## Testing

- `SqlCompleter`: unit test input string → expected completions (tidak perlu DB)
- `SchemaService`: unit test dengan mock `DbConnection`
- Completion scripts: test output mengandung subcommand names dan `--names-only` call
- `pgrs completions bash` → exit 0, output non-empty
- `pgrs list --names-only` → satu nama per baris

## Non-goals

- Syntax highlighting di REPL (bisa ditambah di masa depan dengan `reedline`)
- Auto-refresh schema cache saat DDL dijalankan
- Support schema selain `public`
- Migrate dari manual arg parsing ke `clap`
