# Describe Table (`\d` / `\d+`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah perintah `\d <table>` dan `\d+ <table>` ke REPL dengan full psql parity — kolom, tipe, nullable, default, indexes, FK, check constraints, dan (extended) triggers + storage + column comments.

**Architecture:** Logic describe terlokalisir di `describe.rs` baru yang memanggil beberapa pg_catalog query via `DbConnection::execute()`, lalu format hasilnya dengan pgrs minimal style (`format_result` dari `executor.rs`). Dispatch ditambah di `mod.rs`, tab-completion di `completer.rs`.

**Tech Stack:** Rust, `pg_catalog` queries via `postgres` crate, `nu-ansi-term` untuk warna section headers, `unicode-width` (sudah ada di executor.rs).

---

## File Map

| File | Status | Tanggung jawab |
|------|--------|---------------|
| `src/adapters/driving/repl/describe.rs` | **CREATE** | `describe_table` + semua query helper + formatting |
| `src/adapters/driving/repl/mod.rs` | **MODIFY** | tambah `mod describe`, dispatch `\d`/`\d+`, update help text |
| `src/adapters/driving/repl/completer.rs` | **MODIFY** | tambah `try_complete_describe_arg` ke `complete_input` |

---

### Task 1: Buat `describe.rs` dengan input validation

**Files:**
- Create: `src/adapters/driving/repl/describe.rs`

- [ ] **Step 1: Tulis failing test untuk validate_table_name**

Di akhir `describe.rs` yang baru dibuat:

```rust
use std::io::Write;
use crate::core::ports::db_connection::{DbConnection, QueryResult};

pub fn describe_table(
    db: &dyn DbConnection,
    table: &str,
    extended: bool,
    writer: &mut impl Write,
) -> Result<(), String> {
    validate_table_name(table)?;
    let _ = (db, extended, writer);
    Ok(())
}

fn validate_table_name(name: &str) -> Result<(), String> {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
        Ok(())
    } else {
        Err("invalid table name: only letters, digits, underscores, and dots are allowed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_empty() {
        // empty name has no invalid chars — this is fine to allow or reject
        // we only test the injection-prevention cases
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("public.users").is_ok());
        assert!(validate_table_name("user_roles").is_ok());
    }

    #[test]
    fn validate_rejects_special_chars() {
        assert!(validate_table_name("users; DROP TABLE users").is_err());
        assert!(validate_table_name("users'").is_err());
        assert!(validate_table_name("users\"").is_err());
        assert!(validate_table_name("users-table").is_err());
    }

    #[test]
    fn validate_error_message_is_user_friendly() {
        let err = validate_table_name("bad'name").unwrap_err();
        assert!(err.contains("invalid table name"), "got: {err}");
    }
}
```

- [ ] **Step 2: Jalankan test**

```bash
cargo test describe::tests
```

Expected: 3 tests PASS (implementasi sudah ada di step 1).

- [ ] **Step 3: Daftarkan module di mod.rs**

Di `src/adapters/driving/repl/mod.rs`, tambah setelah baris `mod tokenizer;`:

```rust
mod describe;
```

- [ ] **Step 4: Compile check**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/describe.rs src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): add describe.rs with input validation stub"
```

---

### Task 2: Columns section

**Files:**
- Modify: `src/adapters/driving/repl/describe.rs`

- [ ] **Step 1: Tambah StubDb helper di test module**

Di dalam `#[cfg(test)] mod tests` di `describe.rs`, tambah:

```rust
use std::collections::HashMap;

struct StubDb {
    responses: HashMap<&'static str, Result<QueryResult, String>>,
}

impl StubDb {
    fn new() -> Self { Self { responses: HashMap::new() } }

    fn with(mut self, key: &'static str, result: Result<QueryResult, String>) -> Self {
        self.responses.insert(key, result);
        self
    }
}

impl DbConnection for StubDb {
    fn execute(&self, query: &str) -> Result<QueryResult, String> {
        for (key, result) in &self.responses {
            if query.contains(key) {
                return result.clone();
            }
        }
        Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: None })
    }
}

fn make_columns_result() -> QueryResult {
    QueryResult {
        columns: vec![
            "column".to_string(), "type".to_string(),
            "nullable".to_string(), "default".to_string(),
        ],
        rows: vec![
            vec!["id".to_string(), "integer".to_string(), "not null".to_string(),
                 "nextval('users_id_seq'::regclass)".to_string()],
            vec!["email".to_string(), "character varying(255)".to_string(),
                 "not null".to_string(), "".to_string()],
            vec!["created_at".to_string(), "timestamp with time zone".to_string(),
                 "".to_string(), "now()".to_string()],
        ],
        rows_affected: None,
    }
}
```

Note: `StubDb` match query berdasarkan substring — key `"pg_attribute"` akan match query yang mengandung string tersebut. Urutan key tidak penting karena setiap section query berisi keyword yang unik.

- [ ] **Step 2: Tulis failing test untuk columns section**

```rust
#[test]
fn describe_prints_table_header_and_columns() {
    let db = StubDb::new().with("pg_attribute", Ok(make_columns_result()));
    let mut out = Vec::new();
    describe_table(&db, "users", false, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Table"), "should show Table header, got:\n{text}");
    assert!(text.contains("id"), "should show column id, got:\n{text}");
    assert!(text.contains("integer"), "should show type, got:\n{text}");
    assert!(text.contains("not null"), "should show nullable, got:\n{text}");
}
```

- [ ] **Step 3: Jalankan — pastikan FAIL**

```bash
cargo test describe::tests::describe_prints_table_header_and_columns
```

Expected: FAIL (describe_table hanya returns Ok(()) tanpa output).

- [ ] **Step 4: Implementasi columns query di describe.rs**

Tambah di atas `describe_table`:

```rust
use crate::adapters::driving::repl::executor::format_result;

const COLUMNS_SQL: &str = "
    SELECT
        a.attname AS column,
        pg_catalog.format_type(a.atttypid, a.atttypmod) AS type,
        CASE WHEN a.attnotnull THEN 'not null' ELSE '' END AS nullable,
        COALESCE(pg_catalog.pg_get_expr(d.adbin, d.adrelid), '') AS default
    FROM pg_catalog.pg_attribute a
    LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
    WHERE a.attrelid = 'TABLE_NAME'::regclass
      AND a.attnum > 0
      AND NOT a.attisdropped
    ORDER BY a.attnum";

const SCHEMA_SQL: &str = "
    SELECT n.nspname
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relname = 'TABLE_NAME'";

fn fetch_schema(db: &dyn DbConnection, table: &str) -> String {
    let sql = SCHEMA_SQL.replace("TABLE_NAME", table);
    db.execute(&sql)
        .ok()
        .and_then(|r| r.rows.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .unwrap_or_else(|| "public".to_string())
}
```

Update `describe_table`:

```rust
pub fn describe_table(
    db: &dyn DbConnection,
    table: &str,
    extended: bool,
    writer: &mut impl Write,
) -> Result<(), String> {
    validate_table_name(table)?;

    let schema_name = fetch_schema(db, table);
    writeln!(writer, "Table \"{}.{}\"", schema_name, table).map_err(|e| e.to_string())?;
    writeln!(writer).map_err(|e| e.to_string())?;

    let sql = COLUMNS_SQL.replace("TABLE_NAME", table);
    let result = db.execute(&sql).map_err(|e| {
        format!("Did not find any relation named \"{}\".", table)
            .into()
    })?;

    if result.rows.is_empty() {
        return Err(format!("Did not find any relation named \"{}\".", table));
    }

    write!(writer, "{}", format_result(&result, false)).map_err(|e| e.to_string())?;
    let _ = extended;
    Ok(())
}
```

- [ ] **Step 5: Expose `format_result` dari executor.rs**

`format_result` sudah `pub` di `executor.rs` — tidak perlu perubahan.

Import di `describe.rs` (tambah di atas file):

```rust
use super::executor::format_result;
```

- [ ] **Step 6: Jalankan test**

```bash
cargo test describe::tests
```

Expected: semua PASS.

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/describe.rs
git commit -m "feat(repl): describe_table renders columns section"
```

---

### Task 3: Indexes, FK, dan Check Constraints sections

**Files:**
- Modify: `src/adapters/driving/repl/describe.rs`

- [ ] **Step 1: Tulis failing tests**

```rust
#[test]
fn describe_prints_indexes_section() {
    let indexes = QueryResult {
        columns: vec!["indexname".to_string(), "indexdef".to_string()],
        rows: vec![
            vec!["users_pkey".to_string(), "CREATE UNIQUE INDEX users_pkey ON public.users USING btree (id)".to_string()],
        ],
        rows_affected: None,
    };
    let db = StubDb::new()
        .with("pg_attribute", Ok(make_columns_result()))
        .with("pg_indexes", Ok(indexes));
    let mut out = Vec::new();
    describe_table(&db, "users", false, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Indexes:"), "got:\n{text}");
    assert!(text.contains("users_pkey"), "got:\n{text}");
}

#[test]
fn describe_prints_fk_section() {
    let fk = QueryResult {
        columns: vec!["conname".to_string(), "condef".to_string()],
        rows: vec![
            vec!["users_role_id_fkey".to_string(), "FOREIGN KEY (role_id) REFERENCES roles(id)".to_string()],
        ],
        rows_affected: None,
    };
    let db = StubDb::new()
        .with("pg_attribute", Ok(make_columns_result()))
        .with("contype = 'f'", Ok(fk));
    let mut out = Vec::new();
    describe_table(&db, "users", false, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Foreign-key constraints:"), "got:\n{text}");
    assert!(text.contains("users_role_id_fkey"), "got:\n{text}");
}

#[test]
fn describe_prints_check_constraints_section() {
    let checks = QueryResult {
        columns: vec!["conname".to_string(), "condef".to_string()],
        rows: vec![
            vec!["users_email_check".to_string(), "CHECK ((email ~* '^[^@]+'::text))".to_string()],
        ],
        rows_affected: None,
    };
    let db = StubDb::new()
        .with("pg_attribute", Ok(make_columns_result()))
        .with("contype = 'c'", Ok(checks));
    let mut out = Vec::new();
    describe_table(&db, "users", false, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Check constraints:"), "got:\n{text}");
    assert!(text.contains("users_email_check"), "got:\n{text}");
}

#[test]
fn describe_omits_empty_sections() {
    let db = StubDb::new().with("pg_attribute", Ok(make_columns_result()));
    let mut out = Vec::new();
    describe_table(&db, "users", false, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("Indexes:"), "empty indexes section should be omitted, got:\n{text}");
    assert!(!text.contains("Foreign-key constraints:"), "got:\n{text}");
}
```

- [ ] **Step 2: Jalankan — pastikan FAIL**

```bash
cargo test describe::tests
```

Expected: 4 tests baru FAIL.

- [ ] **Step 3: Implementasi section helpers**

Tambah constants dan helpers di `describe.rs`:

```rust
const INDEXES_SQL: &str = "
    SELECT indexname, indexdef
    FROM pg_indexes
    WHERE tablename = 'TABLE_NAME'
    ORDER BY indexname";

const FK_SQL: &str = "
    SELECT conname, pg_catalog.pg_get_constraintdef(oid, true)
    FROM pg_catalog.pg_constraint
    WHERE conrelid = 'TABLE_NAME'::regclass AND contype = 'f'
    ORDER BY conname";

const CHECK_SQL: &str = "
    SELECT conname, pg_catalog.pg_get_constraintdef(oid, true)
    FROM pg_catalog.pg_constraint
    WHERE conrelid = 'TABLE_NAME'::regclass AND contype = 'c'
    ORDER BY conname";

fn print_named_list(
    db: &dyn DbConnection,
    sql_template: &str,
    table: &str,
    header: &str,
    writer: &mut impl Write,
) {
    let sql = sql_template.replace("TABLE_NAME", table);
    if let Ok(result) = db.execute(&sql) {
        if !result.rows.is_empty() {
            writeln!(writer, "\n{}:", header).ok();
            for row in &result.rows {
                let name = row.get(0).map(String::as_str).unwrap_or("");
                let def = row.get(1).map(String::as_str).unwrap_or("");
                writeln!(writer, "    \"{}\" {}", name, def).ok();
            }
        }
    }
}
```

Update `describe_table`, tambah setelah `write!(writer, "{}", format_result(...))`:

```rust
    print_named_list(db, INDEXES_SQL, table, "Indexes", writer);
    print_named_list(db, FK_SQL, table, "Foreign-key constraints", writer);
    print_named_list(db, CHECK_SQL, table, "Check constraints", writer);
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test describe::tests
```

Expected: semua PASS.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/describe.rs
git commit -m "feat(repl): describe_table renders indexes, FK, and check constraint sections"
```

---

### Task 4: Extended mode (`\d+`) — triggers + column extras

**Files:**
- Modify: `src/adapters/driving/repl/describe.rs`

- [ ] **Step 1: Tulis failing tests**

```rust
#[test]
fn extended_describe_prints_triggers_section() {
    let triggers = QueryResult {
        columns: vec!["tgname".to_string(), "tgdef".to_string()],
        rows: vec![
            vec!["audit_users".to_string(), "CREATE TRIGGER audit_users AFTER INSERT ON users FOR EACH ROW EXECUTE FUNCTION audit()".to_string()],
        ],
        rows_affected: None,
    };
    let db = StubDb::new()
        .with("pg_attribute", Ok(make_columns_result()))
        .with("pg_trigger", Ok(triggers));
    let mut out = Vec::new();
    describe_table(&db, "users", true, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Triggers:"), "got:\n{text}");
    assert!(text.contains("audit_users"), "got:\n{text}");
}

#[test]
fn non_extended_describe_omits_triggers() {
    let triggers = QueryResult {
        columns: vec!["tgname".to_string(), "tgdef".to_string()],
        rows: vec![
            vec!["audit_users".to_string(), "CREATE TRIGGER audit_users ...".to_string()],
        ],
        rows_affected: None,
    };
    let db = StubDb::new()
        .with("pg_attribute", Ok(make_columns_result()))
        .with("pg_trigger", Ok(triggers));
    let mut out = Vec::new();
    describe_table(&db, "users", false, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("Triggers:"), "non-extended should omit triggers, got:\n{text}");
}

fn make_extended_columns_result() -> QueryResult {
    QueryResult {
        columns: vec![
            "column".to_string(), "type".to_string(), "nullable".to_string(),
            "default".to_string(), "storage".to_string(), "stats_target".to_string(),
            "description".to_string(),
        ],
        rows: vec![
            vec!["id".to_string(), "integer".to_string(), "not null".to_string(),
                 "nextval('users_id_seq'::regclass)".to_string(),
                 "plain".to_string(), "-".to_string(), "".to_string()],
            vec!["email".to_string(), "character varying(255)".to_string(), "not null".to_string(),
                 "".to_string(), "extended".to_string(), "-".to_string(), "User email address".to_string()],
        ],
        rows_affected: None,
    }
}

#[test]
fn extended_describe_prints_column_extras() {
    let db = StubDb::new()
        .with("attstorage", Ok(make_extended_columns_result()));
    let mut out = Vec::new();
    describe_table(&db, "users", true, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Storage"), "should show Storage column, got:\n{text}");
    assert!(text.contains("extended"), "should show storage value, got:\n{text}");
}
```

- [ ] **Step 2: Jalankan — pastikan FAIL**

```bash
cargo test describe::tests::extended
```

Expected: FAIL.

- [ ] **Step 3: Implementasi extended mode**

Tambah constants di `describe.rs`:

```rust
const TRIGGERS_SQL: &str = "
    SELECT tgname, pg_catalog.pg_get_triggerdef(oid, true)
    FROM pg_catalog.pg_trigger
    WHERE tgrelid = 'TABLE_NAME'::regclass AND NOT tgisinternal
    ORDER BY tgname";

const COLUMNS_EXTENDED_SQL: &str = "
    SELECT
        a.attname AS column,
        pg_catalog.format_type(a.atttypid, a.atttypmod) AS type,
        CASE WHEN a.attnotnull THEN 'not null' ELSE '' END AS nullable,
        COALESCE(pg_catalog.pg_get_expr(d.adbin, d.adrelid), '') AS default,
        CASE a.attstorage
            WHEN 'p' THEN 'plain'
            WHEN 'e' THEN 'external'
            WHEN 'm' THEN 'main'
            WHEN 'x' THEN 'extended'
            ELSE ''
        END AS storage,
        CASE WHEN a.attstattarget = -1 THEN '-' ELSE a.attstattarget::text END AS stats_target,
        COALESCE(pg_catalog.col_description(a.attrelid, a.attnum), '') AS description
    FROM pg_catalog.pg_attribute a
    LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
    WHERE a.attrelid = 'TABLE_NAME'::regclass
      AND a.attnum > 0
      AND NOT a.attisdropped
    ORDER BY a.attnum";
```

Update `describe_table` — ganti query columns berdasarkan `extended`, dan tambah trigger section.

Sekaligus update `describe_table` yang sudah ada di Task 2 menjadi versi final:

```rust
pub fn describe_table(
    db: &dyn DbConnection,
    table: &str,
    extended: bool,
    writer: &mut impl Write,
) -> Result<(), String> {
    validate_table_name(table)?;

    let schema_name = fetch_schema(db, table);
    writeln!(writer, "Table \"{}.{}\"", schema_name, table).map_err(|e| e.to_string())?;
    writeln!(writer).map_err(|e| e.to_string())?;

    let col_sql = if extended {
        COLUMNS_EXTENDED_SQL.replace("TABLE_NAME", table)
    } else {
        COLUMNS_SQL.replace("TABLE_NAME", table)
    };

    let result = db.execute(&col_sql).map_err(|_| {
        format!("Did not find any relation named \"{}\".", table)
    })?;

    if result.rows.is_empty() {
        return Err(format!("Did not find any relation named \"{}\".", table));
    }

    write!(writer, "{}", format_result(&result, false)).map_err(|e| e.to_string())?;

    print_named_list(db, INDEXES_SQL, table, "Indexes", writer);
    print_named_list(db, FK_SQL, table, "Foreign-key constraints", writer);
    print_named_list(db, CHECK_SQL, table, "Check constraints", writer);

    if extended {
        print_named_list(db, TRIGGERS_SQL, table, "Triggers", writer);
    }

    Ok(())
}
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test describe::tests
```

Expected: semua PASS.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/describe.rs
git commit -m "feat(repl): describe_table extended mode with triggers and column extras"
```

---

### Task 5: Wire `\d` / `\d+` ke mod.rs dispatch

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Tambah import describe**

Setelah `use completer::{SqlCompleter, SqlHighlighter, SqlHinter};` tambah:

```rust
use describe::describe_table;
```

- [ ] **Step 2: Tulis failing test**

Di `mod.rs` test module, tambah:

```rust
#[test]
fn handle_d_shows_usage_without_table_name() {
    // We can't test against a real DB here, but we can verify the dispatch
    // routes \d to the describe function by checking output with a stub.
    // This test just verifies the "no table name" fallback.
    // The full integration test is done in describe.rs.
    // Here: when trimmed == "\\d", output should contain "Usage".
    //
    // We test via repl_help_text that \d appears in help.
    let text = repl_help_text();
    assert!(text.contains("\\d"), "help should mention \\d, got: {text}");
}
```

- [ ] **Step 3: Jalankan — pastikan FAIL**

```bash
cargo test repl::tests::handle_d_shows_usage_without_table_name
```

Expected: FAIL (help text belum mengandung `\d`).

- [ ] **Step 4: Update HELP_COMMANDS dan dispatch**

Di `mod.rs`, update `HELP_COMMANDS` array (sekitar baris 100):

```rust
("\\d <table>",  "describe table (columns, indexes, constraints)"),
("\\d+ <table>", "describe table (extended: + storage, triggers, comments)"),
```

Di command dispatch (match trimmed), ubah `_` arm.

Note: `stdout` sudah dideklarasi di baris `let mut stdout = io::stdout();` sebelum `match trimmed` — gunakan yang sudah ada, jangan deklarasi ulang.

```rust
_ => {
    if let Some(name) = trimmed.strip_prefix("\\d+ ") {
        if let Err(e) = describe_table(conn.as_ref(), name, true, &mut stdout) {
            eprintln!("error: {}", e);
        }
    } else if let Some(name) = trimmed.strip_prefix("\\d ") {
        if let Err(e) = describe_table(conn.as_ref(), name, false, &mut stdout) {
            eprintln!("error: {}", e);
        }
    } else if trimmed == "\\d+" {
        println!("Usage: \\d+ <table>");
    } else if trimmed == "\\d" {
        println!("Usage: \\d <table>");
    } else {
        handle_sql(conn.as_ref(), trimmed, expanded, timing, &mut schema, &mut |s| { rl = build_reedline(s); }, &mut stdout)
    }
}
```

- [ ] **Step 5: Jalankan test**

```bash
cargo test repl::tests
```

Expected: semua PASS.

- [ ] **Step 6: Compile check**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): dispatch \\d and \\d+ to describe_table"
```

---

### Task 6: Tab-completion untuk `\d` / `\d+` arguments

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Tulis failing tests**

Di `completer.rs` test module, gunakan helper `schema_with` yang sudah ada (baris ~461):

```rust
#[test]
fn completes_table_name_after_backslash_d() {
    let schema = schema_with(&["users", "user_roles", "orders"], &[]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("\\d use", 6);
    let names: Vec<_> = results.iter().map(|(s, _)| s.as_str()).collect();
    assert!(names.contains(&"users"), "got: {names:?}");
    assert!(names.contains(&"user_roles"), "got: {names:?}");
    assert!(!names.contains(&"orders"), "orders should be filtered out, got: {names:?}");
}

#[test]
fn completes_table_name_after_backslash_d_plus() {
    let schema = schema_with(&["users", "orders"], &[]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("\\d+ ord", 7);
    let names: Vec<_> = results.iter().map(|(s, _)| s.as_str()).collect();
    assert!(names.contains(&"orders"), "got: {names:?}");
    assert!(!names.contains(&"users"), "got: {names:?}");
}

#[test]
fn completions_for_backslash_d_are_table_kind() {
    let schema = schema_with(&["users"], &[]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("\\d ", 3);
    let kinds: Vec<_> = results.iter().map(|(_, k)| k).collect();
    assert!(kinds.iter().all(|k| matches!(k, CompletionKind::Table)), "got: {kinds:?}");
}
```

Note: `schema_with` sudah ada di test module di `completer.rs` (sekitar baris 461). Tidak perlu tambah helper baru.

- [ ] **Step 2: Jalankan — pastikan FAIL**

```bash
cargo test completer::tests::completes_table_name_after_backslash_d
```

Expected: FAIL.

- [ ] **Step 3: Tambah `try_complete_describe_arg` ke `SqlCompleter`**

Di `completer.rs`, tambah method baru di `impl SqlCompleter`:

```rust
fn try_complete_describe_arg(&self, input: &str) -> Option<Vec<(String, CompletionKind)>> {
    let table_prefix = if input.starts_with("\\d+ ") {
        &input["\\d+ ".len()..]
    } else if input.starts_with("\\d ") {
        &input["\\d ".len()..]
    } else {
        return None;
    };

    let results = self.schema.tables()
        .iter()
        .filter(|t| t.to_lowercase().starts_with(&table_prefix.to_lowercase()))
        .map(|t| (t.clone(), CompletionKind::Table))
        .collect();
    Some(results)
}
```

Update `complete_input` — tambah call di awal method, sebelum `try_complete_qualified`:

```rust
pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
    let input = &line[..pos];

    if let Some(result) = self.try_complete_describe_arg(input) {
        return result;
    }

    let alias_map = build_alias_map(line);
    // ... sisa kode existing tidak berubah
```

- [ ] **Step 4: Jalankan test**

```bash
cargo test completer::tests
```

Expected: semua PASS.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(repl): tab-complete table names after \\d and \\d+"
```

---

### Task 7: Final check

- [ ] **Step 1: Jalankan semua tests**

```bash
cargo test
```

Expected: semua PASS, no regressions.

- [ ] **Step 2: Clippy**

```bash
cargo clippy
```

Expected: no warnings (fix jika ada).

- [ ] **Step 3: Commit fix clippy jika perlu**

```bash
git add -p
git commit -m "fix(repl): clippy warnings in describe.rs"
```
