# \export Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambahkan command `\export <id> <path>` ke REPL pgrs — lookup query dari history by id, re-execute ke DB, tulis hasilnya sebagai CSV ke file.

**Architecture:** Semua perubahan di satu file: `src/adapters/driving/repl/mod.rs`. Tiga fungsi baru ditambahkan: `csv_quote` (helper escaping), `write_csv` (serialisasi ke CSV), `handle_export` (orkestrasi lookup → validate → execute → write). Parsing `\export` ditambahkan di wildcard branch REPL loop. Fungsi `is_dml` ditambahkan untuk menolak INSERT/UPDATE/DELETE.

**Tech Stack:** Rust std (`std::fs::File`, `std::io::Write`, `std::path::Path`), existing `AnalyticsPort` / `ReplPort` traits, `QueryResult` struct.

---

## File yang Diubah

- Modify: `src/adapters/driving/repl/mod.rs`
  - Tambah `csv_quote(val: &str) -> String`
  - Tambah `write_csv(result: &QueryResult, file: &mut impl Write) -> io::Result<()>`
  - Tambah `is_dml(query: &str) -> bool`
  - Tambah `handle_export(id, path, connection_name, conn, analytics, writer)`
  - Tambah entry di `REPL_COMMANDS`
  - Tambah parsing `\export` di REPL loop wildcard branch
  - Tambah unit tests untuk semua fungsi baru

---

## Task 1: `csv_quote` dan `write_csv` — helper CSV serialisasi

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Tulis failing test untuk `csv_quote`**

Tambahkan di dalam `#[cfg(test)]` mod di akhir file:

```rust
#[test]
fn csv_quote_plain_value_unchanged() {
    assert_eq!(csv_quote("hello"), "hello");
}

#[test]
fn csv_quote_value_with_comma_is_quoted() {
    assert_eq!(csv_quote("a,b"), "\"a,b\"");
}

#[test]
fn csv_quote_value_with_double_quote_is_escaped() {
    assert_eq!(csv_quote("say \"hi\""), "\"say \"\"hi\"\"\"");
}

#[test]
fn csv_quote_value_with_newline_is_quoted() {
    assert_eq!(csv_quote("line1\nline2"), "\"line1\nline2\"");
}
```

- [ ] **Step 2: Jalankan test untuk verifikasi FAIL**

```bash
cargo test csv_quote 2>&1 | tail -20
```

Expected: error `cannot find function csv_quote`

- [ ] **Step 3: Implementasi `csv_quote`**

Tambahkan sebelum fungsi `handle_d` di `mod.rs`:

```rust
fn csv_quote(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}
```

- [ ] **Step 4: Tulis failing test untuk `write_csv`**

Tambahkan di `#[cfg(test)]`:

```rust
#[test]
fn write_csv_produces_header_and_rows() {
    use crate::core::ports::db_connection::QueryResult;
    let result = QueryResult {
        columns: vec!["id".to_string(), "name".to_string()],
        rows: vec![
            vec!["1".to_string(), "alice".to_string()],
            vec!["2".to_string(), "bob".to_string()],
        ],
        rows_affected: None,
    };
    let mut out = Vec::new();
    write_csv(&result, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text, "id,name\n1,alice\n2,bob\n");
}

#[test]
fn write_csv_quotes_values_with_comma() {
    use crate::core::ports::db_connection::QueryResult;
    let result = QueryResult {
        columns: vec!["note".to_string()],
        rows: vec![vec!["a,b".to_string()]],
        rows_affected: None,
    };
    let mut out = Vec::new();
    write_csv(&result, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text, "note\n\"a,b\"\n");
}

#[test]
fn write_csv_empty_result_writes_only_header() {
    use crate::core::ports::db_connection::QueryResult;
    let result = QueryResult {
        columns: vec!["id".to_string()],
        rows: vec![],
        rows_affected: None,
    };
    let mut out = Vec::new();
    write_csv(&result, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text, "id\n");
}
```

- [ ] **Step 5: Jalankan test untuk verifikasi FAIL**

```bash
cargo test write_csv 2>&1 | tail -20
```

Expected: error `cannot find function write_csv`

- [ ] **Step 6: Implementasi `write_csv`**

Tambahkan setelah `csv_quote` di `mod.rs`:

```rust
fn write_csv(result: &QueryResult, file: &mut impl Write) -> io::Result<()> {
    let header: Vec<String> = result.columns.iter().map(|c| csv_quote(c)).collect();
    writeln!(file, "{}", header.join(","))?;
    for row in &result.rows {
        let cells: Vec<String> = row.iter().map(|v| csv_quote(v)).collect();
        writeln!(file, "{}", cells.join(","))?;
    }
    Ok(())
}
```

- [ ] **Step 7: Jalankan semua test untuk verifikasi PASS**

```bash
cargo test csv_quote write_csv 2>&1 | tail -20
```

Expected: 7 tests PASS

- [ ] **Step 8: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): add csv_quote and write_csv helpers"
```

---

## Task 2: `is_dml` dan `handle_export`

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Tambahkan stub analytics fleksibel untuk tests**

Di dalam `#[cfg(test)]` mod, tambahkan struct baru (letakkan setelah `RecordingAnalytics`):

```rust
struct FixedHistoryAnalytics {
    entries: Vec<HistoryEntry>,
}
impl FixedHistoryAnalytics {
    fn new(entries: Vec<HistoryEntry>) -> Self { Self { entries } }
}
impl AnalyticsPort for FixedHistoryAnalytics {
    fn record_query(&self, _: &str, _: &str, _: &[String], _: &[(String, String)]) {}
    fn get_history(&self, _: &str) -> Vec<HistoryEntry> { self.entries.clone() }
    fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> { vec![] }
    fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> { vec![] }
}
```

- [ ] **Step 2: Tulis failing tests untuk `is_dml`**

Tambahkan di `#[cfg(test)]`:

```rust
#[test]
fn is_dml_detects_insert() {
    assert!(is_dml("INSERT INTO foo VALUES (1);"));
    assert!(is_dml("insert into foo values (1);"));
}

#[test]
fn is_dml_detects_update() {
    assert!(is_dml("UPDATE foo SET x = 1;"));
}

#[test]
fn is_dml_detects_delete() {
    assert!(is_dml("DELETE FROM foo;"));
}

#[test]
fn is_dml_returns_false_for_select() {
    assert!(!is_dml("SELECT * FROM foo;"));
    assert!(!is_dml("WITH cte AS (SELECT 1) SELECT * FROM cte;"));
}
```

- [ ] **Step 3: Jalankan test untuk verifikasi FAIL**

```bash
cargo test is_dml 2>&1 | tail -10
```

Expected: error `cannot find function is_dml`

- [ ] **Step 4: Implementasi `is_dml`**

Tambahkan setelah fungsi `is_ddl` yang sudah ada di `mod.rs`:

```rust
fn is_dml(query: &str) -> bool {
    matches!(
        query
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_uppercase()
            .as_str(),
        "INSERT" | "UPDATE" | "DELETE"
    )
}
```

- [ ] **Step 5: Jalankan test untuk verifikasi PASS**

```bash
cargo test is_dml 2>&1 | tail -10
```

Expected: 4 tests PASS

- [ ] **Step 6: Tulis failing tests untuk `handle_export`**

Tambahkan di `#[cfg(test)]`. Perhatikan: path menggunakan process id agar tests parallel tidak konflik.

```rust
fn export_tmp_path(tag: &str) -> String {
    format!("/tmp/pgrs_export_{}_{}.csv", std::process::id(), tag)
}

#[test]
fn handle_export_writes_csv_for_valid_id() {
    let path = export_tmp_path("happy");
    let _ = std::fs::remove_file(&path);

    let stub = StubDb::ok(
        vec![vec!["1".to_string(), "alice".to_string()]],
        vec!["id".to_string(), "name".to_string()],
    );
    let analytics = FixedHistoryAnalytics::new(vec![
        HistoryEntry { id: 3, query: "SELECT id, name FROM users;".to_string(), executed_at: 1000 },
    ]);
    let mut out = Vec::new();
    handle_export(3, &path, "mydb", &stub, &analytics, &mut out);

    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("Exported 1 rows to"), "expected confirmation, got: {msg}");

    let csv = std::fs::read_to_string(&path).unwrap();
    assert_eq!(csv, "id,name\n1,alice\n");

    std::fs::remove_file(&path).ok();
}

#[test]
fn handle_export_errors_on_existing_file() {
    let path = export_tmp_path("exists");
    std::fs::write(&path, "existing").unwrap();

    let stub = StubDb::ok(vec![], vec![]);
    let analytics = FixedHistoryAnalytics::new(vec![
        HistoryEntry { id: 1, query: "SELECT 1;".to_string(), executed_at: 1000 },
    ]);
    let mut out = Vec::new();
    handle_export(1, &path, "mydb", &stub, &analytics, &mut out);

    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("file already exists"), "expected file-exists error, got: {msg}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing", "file must not be overwritten");

    std::fs::remove_file(&path).ok();
}

#[test]
fn handle_export_errors_on_unknown_id() {
    let path = export_tmp_path("unknown");
    let _ = std::fs::remove_file(&path);

    let stub = StubDb::ok(vec![], vec![]);
    let analytics = FixedHistoryAnalytics::new(vec![
        HistoryEntry { id: 1, query: "SELECT 1;".to_string(), executed_at: 1000 },
    ]);
    let mut out = Vec::new();
    handle_export(999, &path, "mydb", &stub, &analytics, &mut out);

    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("no history entry with id 999"), "expected id-not-found error, got: {msg}");
    assert!(!std::path::Path::new(&path).exists(), "file must not be created");
}

#[test]
fn handle_export_errors_on_dml_query() {
    let path = export_tmp_path("dml");
    let _ = std::fs::remove_file(&path);

    let stub = StubDb::ok(vec![], vec![]);
    let analytics = FixedHistoryAnalytics::new(vec![
        HistoryEntry { id: 5, query: "INSERT INTO foo VALUES (1);".to_string(), executed_at: 1000 },
    ]);
    let mut out = Vec::new();
    handle_export(5, &path, "mydb", &stub, &analytics, &mut out);

    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("cannot export DML query"), "expected DML error, got: {msg}");
    assert!(!std::path::Path::new(&path).exists(), "file must not be created");
}
```

- [ ] **Step 7: Jalankan test untuk verifikasi FAIL**

```bash
cargo test handle_export 2>&1 | tail -10
```

Expected: error `cannot find function handle_export`

- [ ] **Step 8: Implementasi `handle_export`**

Tambahkan setelah `handle_stats` di `mod.rs`:

```rust
fn handle_export(
    id: i64,
    path: &str,
    connection_name: &str,
    conn: &dyn ReplPort,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
) {
    if std::path::Path::new(path).exists() {
        writeln!(writer, "error: file already exists: {}", path).ok();
        return;
    }
    let history = analytics.get_history(connection_name);
    let entry = match history.iter().find(|e| e.id == id) {
        Some(e) => e,
        None => {
            writeln!(writer, "error: no history entry with id {}", id).ok();
            return;
        }
    };
    if is_dml(&entry.query) {
        writeln!(writer, "error: cannot export DML query").ok();
        return;
    }
    let result = match conn.execute(&entry.query) {
        Ok(r) => r,
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
            return;
        }
    };
    let mut file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            writeln!(writer, "error: could not write file: {}", e).ok();
            return;
        }
    };
    if let Err(e) = write_csv(&result, &mut file) {
        writeln!(writer, "error: could not write file: {}", e).ok();
        return;
    }
    writeln!(writer, "Exported {} rows to {}", result.rows.len(), path).ok();
}
```

- [ ] **Step 9: Jalankan semua test untuk verifikasi PASS**

```bash
cargo test handle_export is_dml 2>&1 | tail -20
```

Expected: 8 tests PASS

- [ ] **Step 10: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): add is_dml and handle_export for \\export command"
```

---

## Task 3: Wire `\export` ke REPL loop

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Tambah entry di `REPL_COMMANDS`**

Cari array `REPL_COMMANDS` (sekitar baris 109). Tambahkan baris baru setelah entry `\\history`:

```rust
("\\export <id> <path>", "export query result from history to CSV file"),
```

Sehingga blok menjadi:

```rust
const REPL_COMMANDS: &[(&str, &str)] = &[
    ("\\d",                  "list all tables"),
    ("\\dt",                 "list all tables with extended information (column count)"),
    ("\\d <table>",          "describe table (columns, indexes, constraints)"),
    ("\\d+ <table>",         "describe table (extended: + storage, triggers, comments)"),
    ("\\l",                  "list databases"),
    ("\\x",                  "toggle expanded display"),
    ("\\timing",             "toggle query execution time"),
    ("\\refresh",            "reload schema (after CREATE/DROP/ALTER TABLE)"),
    ("\\history",            "show recent query history"),
    ("\\export <id> <path>", "export query result from history to CSV file"),
    ("\\stats",              "show most frequently queried tables"),
    ("\\stats <table>",      "show most frequently queried columns for table"),
    ("\\help, \\?",          "show this help"),
    ("\\q, exit",            "quit (or Ctrl+D)"),
];
```

- [ ] **Step 2: Tambah parsing `\export` di REPL loop**

Di dalam `run()`, cari blok wildcard `_ =>` (sekitar baris 443). Tambahkan branch `\export` setelah branch `\stats <table>` dan sebelum blok `else` terakhir yang memanggil `handle_sql`:

```rust
} else if let Some(rest) = trimmed.strip_prefix("\\export ") {
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() != 2 || parts[1].is_empty() {
        writeln!(stdout, "Usage: \\export <id> <path>").ok();
    } else {
        match parts[0].parse::<i64>() {
            Err(_) => { writeln!(stdout, "error: invalid id '{}'", parts[0]).ok(); }
            Ok(id) => match analytics.as_deref() {
                None => { writeln!(stdout, "Analytics not available.").ok(); }
                Some(a) => handle_export(id, parts[1], connection_name, conn.as_ref(), a, &mut stdout),
            }
        }
    }
} else {
    handle_sql(
        // ... (kode yang sudah ada, tidak diubah)
    );
}
```

- [ ] **Step 3: Jalankan `cargo check`**

```bash
cargo check 2>&1 | tail -20
```

Expected: tidak ada error

- [ ] **Step 4: Jalankan `cargo clippy`**

```bash
cargo clippy 2>&1 | tail -20
```

Expected: tidak ada warning baru

- [ ] **Step 5: Jalankan semua test**

```bash
cargo test 2>&1 | tail -30
```

Expected: semua test PASS, tidak ada regresi

- [ ] **Step 6: Verifikasi `\help` menampilkan entry baru**

```bash
cargo build 2>&1 | tail -5
```

Kemudian jalankan REPL (jika ada koneksi DB tersedia) dan ketik `\help`. Pastikan `\export <id> <path>` muncul di output.

Jika tidak ada DB: verifikasi dengan unit test bahwa `repl_help_text()` mengandung `\export`:

```bash
cargo test help_text 2>&1 | tail -10
```

Tambahkan test ini jika belum ada:

```rust
#[test]
fn help_text_mentions_export_command() {
    let text = repl_help_text();
    assert!(text.contains("\\export"), "help should mention \\export, got: {text}");
}
```

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): wire \\export command into REPL loop"
```
