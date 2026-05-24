# SQLite Analytics & Schema Cache Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambahkan SQLite sebagai embedded storage untuk query history, frekuensi table/column, dan schema cache di pgrs.

**Architecture:** Dua port baru (`AnalyticsPort`, `SchemaCachePort`) di `core/ports/`, diimplementasikan oleh satu adapter baru `SqliteRepository` di `adapters/driven/`. `app.rs` membuat `Arc<SqliteRepository>`, meng-coerce ke dua trait objects, dan meneruskannya ke `repl::run()` dan `SchemaService`. Semua error dari SQLite diabaikan secara diam-diam.

**Tech Stack:** Rust, rusqlite 0.31 (bundled), std::sync::Mutex, std::sync::Arc

---

## File Map

| File | Status | Perubahan |
|------|--------|-----------|
| `Cargo.toml` | modify | tambah rusqlite |
| `src/core/domain/analytics.rs` | create | `HistoryEntry`, `FreqEntry` |
| `src/core/domain/mod.rs` | modify | export analytics |
| `src/core/ports/analytics_port.rs` | create | `AnalyticsPort` trait |
| `src/core/ports/schema_cache_port.rs` | create | `SchemaCachePort` trait |
| `src/core/ports/mod.rs` | modify | export 2 port baru |
| `src/adapters/driven/sqlite_repository.rs` | create | `SqliteRepository` + impls |
| `src/adapters/driven/mod.rs` | modify | export sqlite_repository |
| `src/core/services/schema/service.rs` | modify | tambah `load_with_cache` |
| `src/adapters/driving/repl/alias.rs` | modify | tambah `extract_referenced_tables` |
| `src/adapters/driving/repl/mod.rs` | modify | terima analytics/cache, `\history`, `\stats` |
| `src/app.rs` | modify | wire SqliteRepository |

---

## Task 1: Tambah dependency rusqlite

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Tambah rusqlite ke Cargo.toml**

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

Tambahkan di bawah baris `serde_json`.

- [ ] **Step 2: Verify compile**

```bash
cargo check
```

Expected: berhasil tanpa error. Jika versi tidak ada, cek `cargo search rusqlite` untuk versi terbaru.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add rusqlite dependency"
```

---

## Task 2: Domain types untuk analytics

**Files:**
- Create: `src/core/domain/analytics.rs`
- Modify: `src/core/domain/mod.rs`

- [ ] **Step 1: Tulis test di file yang akan dibuat**

Buat `src/core/domain/analytics.rs`:

```rust
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub query: String,
    pub executed_at: i64,
}

#[derive(Debug, Clone)]
pub struct FreqEntry {
    pub name: String,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_entry_stores_query_and_timestamp() {
        let entry = HistoryEntry {
            query: "SELECT 1".to_string(),
            executed_at: 1234567890,
        };
        assert_eq!(entry.query, "SELECT 1");
        assert_eq!(entry.executed_at, 1234567890);
    }

    #[test]
    fn freq_entry_stores_name_and_count() {
        let entry = FreqEntry { name: "users".to_string(), count: 42 };
        assert_eq!(entry.name, "users");
        assert_eq!(entry.count, 42);
    }
}
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test domain::analytics
```

Expected: FAIL — module tidak ditemukan.

- [ ] **Step 3: Export di mod.rs**

Edit `src/core/domain/mod.rs`:

```rust
pub mod analytics;
pub mod connection;
```

- [ ] **Step 4: Run test lagi**

```bash
cargo test domain::analytics
```

Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/core/domain/analytics.rs src/core/domain/mod.rs
git commit -m "feat(domain): add HistoryEntry and FreqEntry types"
```

---

## Task 3: AnalyticsPort trait

**Files:**
- Create: `src/core/ports/analytics_port.rs`
- Modify: `src/core/ports/mod.rs`

- [ ] **Step 1: Buat file port**

```rust
use crate::core::domain::analytics::{FreqEntry, HistoryEntry};

pub trait AnalyticsPort: Send + Sync {
    fn record_query(
        &self,
        db_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String /* table */, String /* column */)],
    );
    fn get_history(&self, db_name: &str) -> Vec<HistoryEntry>;
    fn get_frequent_tables(&self, db_name: &str) -> Vec<FreqEntry>;
    fn get_frequent_columns(&self, db_name: &str, table: &str) -> Vec<FreqEntry>;
}
```

- [ ] **Step 2: Export di ports/mod.rs**

```rust
pub mod analytics_port;
pub mod connection_repository;
pub mod db_connection;
pub mod repl_port;
pub mod schema_port;
```

- [ ] **Step 3: Verify compile**

```bash
cargo check
```

Expected: berhasil.

- [ ] **Step 4: Commit**

```bash
git add src/core/ports/analytics_port.rs src/core/ports/mod.rs
git commit -m "feat(ports): add AnalyticsPort trait"
```

---

## Task 4: SchemaCachePort trait

**Files:**
- Create: `src/core/ports/schema_cache_port.rs`
- Modify: `src/core/ports/mod.rs`

- [ ] **Step 1: Buat file port**

```rust
use std::collections::HashMap;

pub trait SchemaCachePort: Send + Sync {
    fn save_schema(&self, db_name: &str, schema: &HashMap<String, Vec<String>>);
    fn load_schema(&self, db_name: &str) -> Option<HashMap<String, Vec<String>>>;
    fn invalidate(&self, db_name: &str);
}
```

- [ ] **Step 2: Export di ports/mod.rs**

```rust
pub mod analytics_port;
pub mod connection_repository;
pub mod db_connection;
pub mod repl_port;
pub mod schema_cache_port;
pub mod schema_port;
```

- [ ] **Step 3: Verify compile**

```bash
cargo check
```

Expected: berhasil.

- [ ] **Step 4: Commit**

```bash
git add src/core/ports/schema_cache_port.rs src/core/ports/mod.rs
git commit -m "feat(ports): add SchemaCachePort trait"
```

---

## Task 5: SqliteRepository — DB init dan migrations

**Files:**
- Create: `src/adapters/driven/sqlite_repository.rs`
- Modify: `src/adapters/driven/mod.rs`

- [ ] **Step 1: Tulis failing test**

Buat `src/adapters/driven/sqlite_repository.rs` dengan hanya test terlebih dahulu:

```rust
use rusqlite::Connection;
use std::sync::Mutex;

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS query_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    query       TEXT    NOT NULL,
    executed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_history_db ON query_history(db_name, executed_at);

CREATE TABLE IF NOT EXISTS table_access (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    query_id    INTEGER REFERENCES query_history(id),
    accessed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_table_access_db ON table_access(db_name, table_name);

CREATE TABLE IF NOT EXISTS column_access (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    column_name TEXT    NOT NULL,
    query_id    INTEGER REFERENCES query_history(id),
    accessed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_column_access_db ON column_access(db_name, table_name);

CREATE TABLE IF NOT EXISTS schema_tables (
    db_name    TEXT    NOT NULL,
    table_name TEXT    NOT NULL,
    cached_at  INTEGER NOT NULL,
    PRIMARY KEY (db_name, table_name)
);

CREATE TABLE IF NOT EXISTS schema_columns (
    db_name     TEXT NOT NULL,
    table_name  TEXT NOT NULL,
    column_name TEXT NOT NULL,
    data_type   TEXT,
    cached_at   INTEGER NOT NULL,
    PRIMARY KEY (db_name, table_name, column_name)
);
";

pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

impl SqliteRepository {
    pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        let repo = Self { conn: Mutex::new(conn) };
        repo.migrate()?;
        Ok(repo)
    }

    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let repo = Self { conn: Mutex::new(conn) };
        repo.migrate()?;
        Ok(repo)
    }

    fn migrate(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", &1)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_schema() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let conn = repo.conn.lock().unwrap();
        // Verify table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='query_history'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_sets_user_version_to_1() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let conn = repo.conn.lock().unwrap();
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn migration_is_idempotent() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        // Calling migrate again should not fail
        repo.migrate().unwrap();
    }
}
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test sqlite_repository
```

Expected: FAIL — module tidak ditemukan.

- [ ] **Step 3: Export di adapters/driven/mod.rs**

```rust
pub mod file_connection_repository;
pub mod postgres_db;
pub mod sqlite_repository;
```

- [ ] **Step 4: Run test**

```bash
cargo test sqlite_repository
```

Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driven/sqlite_repository.rs src/adapters/driven/mod.rs
git commit -m "feat(storage): add SqliteRepository with schema migration"
```

---

## Task 6: Implement SchemaCachePort

**Files:**
- Modify: `src/adapters/driven/sqlite_repository.rs`

- [ ] **Step 1: Tulis failing tests**

Tambahkan di bawah test yang ada di `sqlite_repository.rs`:

```rust
    // --- SchemaCachePort tests ---

    #[test]
    fn save_and_load_schema_round_trip() {
        use std::collections::HashMap;
        let repo = SqliteRepository::open_in_memory().unwrap();
        let mut schema = HashMap::new();
        schema.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        schema.insert("orders".to_string(), vec!["id".to_string(), "user_id".to_string()]);

        repo.save_schema("mydb", &schema);
        let loaded = repo.load_schema("mydb").unwrap();
        assert_eq!(loaded["users"], vec!["id", "email"]);
        assert_eq!(loaded["orders"], vec!["id", "user_id"]);
    }

    #[test]
    fn load_schema_returns_none_for_unknown_db() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        assert!(repo.load_schema("ghost").is_none());
    }

    #[test]
    fn invalidate_removes_schema() {
        use std::collections::HashMap;
        let repo = SqliteRepository::open_in_memory().unwrap();
        let mut schema = HashMap::new();
        schema.insert("users".to_string(), vec!["id".to_string()]);

        repo.save_schema("mydb", &schema);
        assert!(repo.load_schema("mydb").is_some());

        repo.invalidate("mydb");
        assert!(repo.load_schema("mydb").is_none());
    }

    #[test]
    fn save_schema_overwrites_existing() {
        use std::collections::HashMap;
        let repo = SqliteRepository::open_in_memory().unwrap();

        let mut schema_v1 = HashMap::new();
        schema_v1.insert("users".to_string(), vec!["id".to_string()]);
        repo.save_schema("mydb", &schema_v1);

        let mut schema_v2 = HashMap::new();
        schema_v2.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        schema_v2.insert("orders".to_string(), vec!["id".to_string()]);
        repo.save_schema("mydb", &schema_v2);

        let loaded = repo.load_schema("mydb").unwrap();
        assert_eq!(loaded.len(), 2, "second save should replace first");
        assert_eq!(loaded["users"], vec!["id", "email"]);
    }
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test sqlite_repository
```

Expected: 4 tests baru FAIL — methods tidak ada.

- [ ] **Step 3: Implement SchemaCachePort**

Tambahkan di `sqlite_repository.rs` sebelum blok `#[cfg(test)]`:

```rust
use std::collections::HashMap;
use crate::core::ports::schema_cache_port::SchemaCachePort;

impl SchemaCachePort for SqliteRepository {
    fn save_schema(&self, db_name: &str, schema: &HashMap<String, Vec<String>>) {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = (|| -> Result<(), rusqlite::Error> {
            conn.execute(
                "DELETE FROM schema_columns WHERE db_name = ?1",
                rusqlite::params![db_name],
            )?;
            conn.execute(
                "DELETE FROM schema_tables WHERE db_name = ?1",
                rusqlite::params![db_name],
            )?;
            for (table, columns) in schema {
                conn.execute(
                    "INSERT OR REPLACE INTO schema_tables (db_name, table_name, cached_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![db_name, table, now],
                )?;
                for col in columns {
                    conn.execute(
                        "INSERT OR REPLACE INTO schema_columns (db_name, table_name, column_name, cached_at) VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![db_name, table, col, now],
                    )?;
                }
            }
            Ok(())
        })() {
            eprintln!("pgrs: schema cache write failed: {e}");
        }
    }

    fn load_schema(&self, db_name: &str) -> Option<HashMap<String, Vec<String>>> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<(String, String)>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT table_name, column_name FROM schema_columns WHERE db_name = ?1 ORDER BY table_name, column_name",
            )?;
            let rows = stmt.query_map(rusqlite::params![db_name], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            rows.collect()
        })();

        match result {
            Ok(rows) if rows.is_empty() => None,
            Ok(rows) => {
                let mut map: HashMap<String, Vec<String>> = HashMap::new();
                for (table, col) in rows {
                    map.entry(table).or_default().push(col);
                }
                Some(map)
            }
            Err(e) => {
                eprintln!("pgrs: schema cache read failed: {e}");
                None
            }
        }
    }

    fn invalidate(&self, db_name: &str) {
        let conn = self.conn.lock().unwrap();
        if let Err(e) = conn.execute(
            "DELETE FROM schema_columns WHERE db_name = ?1",
            rusqlite::params![db_name],
        ) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
            return;
        }
        if let Err(e) = conn.execute(
            "DELETE FROM schema_tables WHERE db_name = ?1",
            rusqlite::params![db_name],
        ) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
    }
}
```

- [ ] **Step 4: Run semua tests**

```bash
cargo test sqlite_repository
```

Expected: semua tests PASS (termasuk 3 dari task 5 + 4 baru).

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driven/sqlite_repository.rs
git commit -m "feat(storage): implement SchemaCachePort on SqliteRepository"
```

---

## Task 7: Implement AnalyticsPort

**Files:**
- Modify: `src/adapters/driven/sqlite_repository.rs`

- [ ] **Step 1: Tulis failing tests**

Tambahkan di blok `#[cfg(test)]`:

```rust
    // --- AnalyticsPort tests ---

    #[test]
    fn record_query_and_get_history() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.record_query("mydb", "SELECT 1", &[], &[]);
        repo.record_query("mydb", "SELECT 2", &[], &[]);

        let history = repo.get_history("mydb");
        assert_eq!(history.len(), 2);
        // Most recent first
        assert_eq!(history[0].query, "SELECT 2");
    }

    #[test]
    fn get_history_filters_by_db_name() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.record_query("db1", "SELECT 1", &[], &[]);
        repo.record_query("db2", "SELECT 2", &[], &[]);

        let history = repo.get_history("db1");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].query, "SELECT 1");
    }

    #[test]
    fn get_frequent_tables_ordered_by_count() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let users = vec!["users".to_string()];
        let orders = vec!["orders".to_string()];
        repo.record_query("mydb", "SELECT * FROM users", &users, &[]);
        repo.record_query("mydb", "SELECT * FROM users", &users, &[]);
        repo.record_query("mydb", "SELECT * FROM orders", &orders, &[]);

        let freq = repo.get_frequent_tables("mydb");
        assert_eq!(freq[0].name, "users");
        assert_eq!(freq[0].count, 2);
        assert_eq!(freq[1].name, "orders");
        assert_eq!(freq[1].count, 1);
    }

    #[test]
    fn get_frequent_columns_for_table() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let tables = vec!["users".to_string()];
        let cols = vec![
            ("users".to_string(), "email".to_string()),
            ("users".to_string(), "email".to_string()),
            ("users".to_string(), "id".to_string()),
        ];
        repo.record_query("mydb", "SELECT email, id FROM users", &tables, &cols);

        let freq = repo.get_frequent_columns("mydb", "users");
        assert_eq!(freq[0].name, "email");
        assert_eq!(freq[0].count, 2);
        assert_eq!(freq[1].name, "id");
        assert_eq!(freq[1].count, 1);
    }

    #[test]
    fn get_frequent_tables_returns_empty_for_unknown_db() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        assert!(repo.get_frequent_tables("ghost").is_empty());
    }
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test sqlite_repository
```

Expected: 5 tests baru FAIL.

- [ ] **Step 3: Implement AnalyticsPort**

Tambahkan di `sqlite_repository.rs` setelah impl `SchemaCachePort`:

```rust
use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
use crate::core::ports::analytics_port::AnalyticsPort;

impl AnalyticsPort for SqliteRepository {
    fn record_query(
        &self,
        db_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    ) {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = (|| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO query_history (db_name, query, executed_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![db_name, query, now],
            )?;
            let query_id = conn.last_insert_rowid();

            for table in tables {
                conn.execute(
                    "INSERT INTO table_access (db_name, table_name, query_id, accessed_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![db_name, table, query_id, now],
                )?;
            }

            for (table, column) in columns {
                conn.execute(
                    "INSERT INTO column_access (db_name, table_name, column_name, query_id, accessed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![db_name, table, column, query_id, now],
                )?;
            }
            Ok(())
        })() {
            eprintln!("pgrs: analytics write failed: {e}");
        }
    }

    fn get_history(&self, db_name: &str) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<HistoryEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT query, executed_at FROM query_history WHERE db_name = ?1 ORDER BY executed_at DESC LIMIT 50",
            )?;
            let rows = stmt.query_map(rusqlite::params![db_name], |r| {
                Ok(HistoryEntry {
                    query: r.get(0)?,
                    executed_at: r.get(1)?,
                })
            })?;
            rows.collect()
        })();

        match result {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("pgrs: analytics read failed: {e}");
                vec![]
            }
        }
    }

    fn get_frequent_tables(&self, db_name: &str) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT table_name, COUNT(*) as cnt FROM table_access WHERE db_name = ?1 GROUP BY table_name ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![db_name], |r| {
                Ok(FreqEntry {
                    name: r.get(0)?,
                    count: r.get::<_, i64>(1)? as u64,
                })
            })?;
            rows.collect()
        })();

        match result {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("pgrs: analytics read failed: {e}");
                vec![]
            }
        }
    }

    fn get_frequent_columns(&self, db_name: &str, table: &str) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT column_name, COUNT(*) as cnt FROM column_access WHERE db_name = ?1 AND table_name = ?2 GROUP BY column_name ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![db_name, table], |r| {
                Ok(FreqEntry {
                    name: r.get(0)?,
                    count: r.get::<_, i64>(1)? as u64,
                })
            })?;
            rows.collect()
        })();

        match result {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("pgrs: analytics read failed: {e}");
                vec![]
            }
        }
    }
}
```

- [ ] **Step 4: Run semua tests**

```bash
cargo test sqlite_repository
```

Expected: semua tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driven/sqlite_repository.rs
git commit -m "feat(storage): implement AnalyticsPort on SqliteRepository"
```

---

## Task 8: Tambah extract_referenced_tables di alias.rs

**Files:**
- Modify: `src/adapters/driving/repl/alias.rs`

- [ ] **Step 1: Tulis failing tests**

Tambahkan di blok `#[cfg(test)]` di `alias.rs`:

```rust
    #[test]
    fn extract_referenced_tables_simple_from() {
        let tables = extract_referenced_tables("SELECT * FROM users");
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn extract_referenced_tables_with_alias() {
        let tables = extract_referenced_tables("SELECT * FROM users u");
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn extract_referenced_tables_join() {
        let mut tables = extract_referenced_tables("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        tables.sort();
        assert_eq!(tables, vec!["orders", "users"]);
    }

    #[test]
    fn extract_referenced_tables_no_from_returns_empty() {
        let tables = extract_referenced_tables("SELECT 1");
        assert!(tables.is_empty());
    }

    #[test]
    fn extract_referenced_tables_schema_qualified() {
        let tables = extract_referenced_tables("SELECT * FROM public.users");
        assert_eq!(tables, vec!["users"]);
    }
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test alias::tests::extract_referenced
```

Expected: FAIL — function tidak ada.

- [ ] **Step 3: Implement function**

Tambahkan di bawah `build_alias_map` di `alias.rs`:

```rust
pub fn extract_referenced_tables(line: &str) -> Vec<String> {
    let mut tables: Vec<String> = Vec::new();
    let mut state = AliasState::Idle;

    for token in tokenize(line) {
        if let SqlToken::Other(c) = token {
            if c.is_whitespace() { continue; }
            state = match (state, c) {
                (AliasState::ExpectTable, '(') => AliasState::InSubquery { depth: 1 },
                (AliasState::ExpectAlias { ref candidate }, '.') => {
                    // schema-qualified: "schema." — discard prefix, expect real table
                    let _ = candidate;
                    AliasState::ExpectQualifiedTable
                }
                (AliasState::ExpectAlias { candidate }, ',') => {
                    tables.push(candidate);
                    AliasState::ExpectTable
                }
                (AliasState::PostAlias, ',') => AliasState::ExpectTable,
                (AliasState::InSubquery { depth }, '(') => AliasState::InSubquery { depth: depth + 1 },
                (AliasState::InSubquery { depth }, ')') => {
                    if depth == 1 { AliasState::ExpectSubqueryAlias } else { AliasState::InSubquery { depth: depth - 1 } }
                }
                (AliasState::InSubquery { depth }, _) => AliasState::InSubquery { depth },
                (s, _) => s,
            };
            continue;
        }
        state = match (state, token) {
            (AliasState::Idle, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::ExpectTable, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                AliasState::ExpectAlias { candidate: w.to_lowercase() }
            }
            (AliasState::ExpectTable, _) => AliasState::Idle,
            (AliasState::ExpectAlias { candidate }, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                tables.push(candidate.clone());
                AliasState::ExpectAliasName { candidate }
            }
            (AliasState::ExpectAlias { candidate }, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                tables.push(candidate);
                AliasState::PostAlias
            }
            (AliasState::ExpectAlias { candidate }, _) => {
                tables.push(candidate);
                AliasState::Idle
            }
            (AliasState::ExpectQualifiedTable, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                AliasState::ExpectAlias { candidate: w.to_lowercase() }
            }
            (AliasState::ExpectQualifiedTable, _) => AliasState::Idle,
            (AliasState::ExpectAliasName { .. }, SqlToken::Word(_)) => AliasState::PostAlias,
            (AliasState::ExpectAliasName { .. }, _) => AliasState::Idle,
            (AliasState::PostAlias, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::PostAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                AliasState::ExpectSubqueryAliasName
            }
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(_)) => AliasState::PostAlias,
            (AliasState::ExpectSubqueryAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAliasName, SqlToken::Word(_)) => AliasState::PostAlias,
            (AliasState::ExpectSubqueryAliasName, _) => AliasState::Idle,
            (s, _) => s,
        };
    }

    if let AliasState::ExpectAlias { candidate } = state {
        tables.push(candidate);
    }

    tables.dedup();
    tables
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test alias::tests::extract_referenced
```

Expected: 5 tests PASS.

- [ ] **Step 5: Run semua tests untuk verifikasi tidak ada regresi**

```bash
cargo test
```

Expected: semua tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/alias.rs
git commit -m "feat(repl): add extract_referenced_tables for analytics tracking"
```

---

## Task 9: Update SchemaService — load_with_cache

**Files:**
- Modify: `src/core/services/schema/service.rs`

- [ ] **Step 1: Tulis failing tests**

Tambahkan di blok `#[cfg(test)]` di `service.rs`:

```rust
    use crate::core::ports::schema_cache_port::SchemaCachePort;
    use std::cell::RefCell;

    struct MockCache {
        stored: RefCell<Option<HashMap<String, Vec<String>>>>,
        loaded: RefCell<bool>,
        invalidated: RefCell<bool>,
    }

    impl MockCache {
        fn empty() -> Self {
            Self {
                stored: RefCell::new(None),
                loaded: RefCell::new(false),
                invalidated: RefCell::new(false),
            }
        }
        fn with_data(schema: HashMap<String, Vec<String>>) -> Self {
            Self {
                stored: RefCell::new(Some(schema)),
                loaded: RefCell::new(false),
                invalidated: RefCell::new(false),
            }
        }
    }

    impl SchemaCachePort for MockCache {
        fn save_schema(&self, _db: &str, schema: &HashMap<String, Vec<String>>) {
            *self.stored.borrow_mut() = Some(schema.clone());
        }
        fn load_schema(&self, _db: &str) -> Option<HashMap<String, Vec<String>>> {
            *self.loaded.borrow_mut() = true;
            self.stored.borrow().clone()
        }
        fn invalidate(&self, _db: &str) {
            *self.invalidated.borrow_mut() = true;
        }
    }

    #[test]
    fn load_with_cache_uses_cache_when_available() {
        let mut cached = HashMap::new();
        cached.insert("cached_table".to_string(), vec!["id".to_string()]);

        let db = mock_db(); // has "users" and "orders"
        let cache = MockCache::with_data(cached);

        let schema = SchemaService::load_with_cache(&db, "mydb", Some(&cache)).unwrap();
        // Should come from cache, not db
        assert!(schema.tables().contains(&"cached_table".to_string()));
        assert!(!schema.tables().contains(&"users".to_string()));
    }

    #[test]
    fn load_with_cache_falls_back_to_db_and_saves_when_cache_empty() {
        let db = mock_db();
        let cache = MockCache::empty();

        let schema = SchemaService::load_with_cache(&db, "mydb", Some(&cache)).unwrap();
        // Should come from db
        assert!(schema.tables().contains(&"users".to_string()));
        // Should have been saved to cache
        assert!(cache.stored.borrow().is_some());
    }

    #[test]
    fn load_with_cache_none_behaves_like_load() {
        let db = mock_db();
        let schema = SchemaService::load_with_cache(&db, "mydb", None).unwrap();
        assert!(schema.tables().contains(&"users".to_string()));
    }
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test schema::service
```

Expected: 3 tests baru FAIL — method tidak ada.

- [ ] **Step 3: Implement load_with_cache**

Tambahkan di dalam `impl SchemaService` di `service.rs`:

```rust
use crate::core::ports::schema_cache_port::SchemaCachePort;

// Tambahkan di dalam impl SchemaService, setelah fn load:
pub fn load_with_cache(
    conn: &dyn SchemaPort,
    db_name: &str,
    cache: Option<&dyn SchemaCachePort>,
) -> Result<Self, String> {
    if let Some(cache) = cache {
        if let Some(columns) = cache.load_schema(db_name) {
            let mut tables: Vec<String> = columns.keys().cloned().collect();
            tables.sort();
            return Ok(Self { tables, columns });
        }
    }
    let result = Self::load(conn)?;
    if let Some(cache) = cache {
        cache.save_schema(db_name, &result.columns);
    }
    Ok(result)
}
```

Dan tambahkan import di bagian atas file:

```rust
use crate::core::ports::schema_cache_port::SchemaCachePort;
```

- [ ] **Step 4: Run tests**

```bash
cargo test schema::service
```

Expected: semua tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/core/services/schema/service.rs
git commit -m "feat(schema): add load_with_cache for SQLite-backed schema caching"
```

---

## Task 10: Update REPL — analytics recording dan new commands

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

Ini adalah task terbesar. Beberapa perubahan sekaligus:
1. Signature `run()` baru dengan `analytics` dan `schema_cache` params
2. `handle_sql` record analytics setelah eksekusi
3. `handle_refresh` invalidate cache sebelum refresh
4. Tambah `handle_history` dan `handle_stats`
5. Tambah `\history` dan `\stats` ke command dispatch dan REPL_COMMANDS

- [ ] **Step 1: Tulis failing tests**

Tambahkan di blok `#[cfg(test)]` di `mod.rs`:

```rust
    use std::cell::RefCell;
    use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
    use crate::core::ports::analytics_port::AnalyticsPort;

    struct RecordingAnalytics {
        recorded: RefCell<Vec<(String, String)>>, // (db_name, query)
    }
    impl RecordingAnalytics {
        fn new() -> Self { Self { recorded: RefCell::new(vec![]) } }
    }
    impl AnalyticsPort for RecordingAnalytics {
        fn record_query(&self, db_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
            self.recorded.borrow_mut().push((db_name.to_string(), query.to_string()));
        }
        fn get_history(&self, _: &str) -> Vec<HistoryEntry> {
            vec![
                HistoryEntry { query: "SELECT 1".to_string(), executed_at: 1000 },
                HistoryEntry { query: "SELECT 2".to_string(), executed_at: 999 },
            ]
        }
        fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> {
            vec![FreqEntry { name: "users".to_string(), count: 5 }]
        }
        fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> {
            vec![FreqEntry { name: "email".to_string(), count: 3 }]
        }
    }

    #[test]
    fn handle_history_shows_queries() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_history("mydb", &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("SELECT 1"), "expected query in history, got: {text}");
        assert!(text.contains("SELECT 2"), "expected query in history, got: {text}");
    }

    #[test]
    fn handle_stats_no_table_shows_tables() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_stats("mydb", None, &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected table name, got: {text}");
        assert!(text.contains("5"), "expected count, got: {text}");
    }

    #[test]
    fn handle_stats_with_table_shows_columns() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_stats("mydb", Some("users"), &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("email"), "expected column name, got: {text}");
        assert!(text.contains("3"), "expected count, got: {text}");
    }
```

- [ ] **Step 2: Run test untuk verifikasi fail**

```bash
cargo test repl::tests::handle_history
cargo test repl::tests::handle_stats
```

Expected: FAIL — functions tidak ada.

- [ ] **Step 3: Tambah use statements di atas mod.rs**

Di bagian `use` yang ada, tambahkan:

```rust
use std::sync::Arc;
use crate::core::domain::analytics::FreqEntry;
use crate::core::ports::analytics_port::AnalyticsPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use alias::extract_referenced_tables;
```

- [ ] **Step 4: Tambah handle_history dan handle_stats**

Tambahkan setelah `fn handle_l(...)`:

```rust
fn handle_history(db_name: &str, analytics: &dyn AnalyticsPort, writer: &mut impl Write) {
    let history = analytics.get_history(db_name);
    if history.is_empty() {
        writeln!(writer, "No query history.").ok();
        return;
    }
    for entry in &history {
        writeln!(writer, "  {}", entry.query).ok();
    }
    writeln!(writer, "({} entries)", history.len()).ok();
}

fn handle_stats(
    db_name: &str,
    table: Option<&str>,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
) {
    match table {
        None => {
            let freq = analytics.get_frequent_tables(db_name);
            if freq.is_empty() {
                writeln!(writer, "No table statistics yet.").ok();
                return;
            }
            let name_w = freq.iter().map(|e| e.name.len()).max().unwrap_or(0);
            for entry in &freq {
                writeln!(writer, "  {:<name_w$}  {}", entry.name, entry.count).ok();
            }
        }
        Some(tbl) => {
            let freq = analytics.get_frequent_columns(db_name, tbl);
            if freq.is_empty() {
                writeln!(writer, "No column statistics for '{}'.", tbl).ok();
                return;
            }
            let name_w = freq.iter().map(|e| e.name.len()).max().unwrap_or(0);
            for entry in &freq {
                writeln!(writer, "  {:<name_w$}  {}", entry.name, entry.count).ok();
            }
        }
    }
}
```

- [ ] **Step 5: Update handle_sql untuk record analytics**

Ubah signature `handle_sql` dan tambahkan pemanggilan `record_query` setelah query berhasil:

```rust
fn handle_sql(
    conn: &dyn ReplPort,
    query: &str,
    expanded: bool,
    timing: bool,
    db_name: &str,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    analytics: Option<&dyn AnalyticsPort>,
    schema_cache: Option<&dyn SchemaCachePort>,
    writer: &mut impl Write,
) {
    let start = std::time::Instant::now();
    match conn.execute(query) {
        Ok(result) => {
            write!(writer, "{}", format_result(&result, expanded)).ok();
            if timing {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                if ms >= 1000.0 {
                    writeln!(writer, "Time: {:.3} s", ms / 1000.0).ok();
                } else {
                    writeln!(writer, "Time: {:.3} ms", ms).ok();
                }
            }

            // Record analytics
            if let Some(analytics) = analytics {
                let tables = extract_referenced_tables(query);
                let columns = extract_column_refs(query, schema);
                analytics.record_query(db_name, query, &tables, &columns);
            }

            if is_ddl(query)
                && let Ok(new_schema) = SchemaService::load_with_cache(conn, db_name, schema_cache)
            {
                *schema = new_schema.clone();
                rebuild(new_schema);
                writeln!(writer, "(schema refreshed)").ok();
            }
        }
        Err(e) => eprintln!("error: {}", e),
    }
}
```

- [ ] **Step 6: Tambah extract_column_refs**

Tambahkan fungsi helper di `mod.rs` (sebelum `handle_sql`):

```rust
fn extract_column_refs(query: &str, schema: &SchemaService) -> Vec<(String, String)> {
    use tokenizer::{SqlToken, tokenize};
    use alias::SQL_KEYWORDS;

    let mut in_select = false;
    let mut candidates: Vec<String> = Vec::new();

    for token in tokenize(query) {
        if let SqlToken::Word(w) = token {
            let upper = w.to_uppercase();
            if upper == "SELECT" { in_select = true; continue; }
            if upper == "FROM" { in_select = false; break; }
            if in_select && !SQL_KEYWORDS.contains(&upper.as_str()) && w != "*" {
                candidates.push(w.to_lowercase());
            }
        }
    }

    let mut refs = Vec::new();
    for col in candidates {
        for table in schema.tables() {
            if schema.columns_for(table).iter().any(|c| c == &col) {
                refs.push((table.to_string(), col.clone()));
                break;
            }
        }
    }
    refs
}
```

- [ ] **Step 7: Update handle_refresh**

```rust
fn handle_refresh(
    conn: &dyn SchemaPort,
    db_name: &str,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    schema_cache: Option<&dyn SchemaCachePort>,
    writer: &mut impl Write,
) {
    if let Some(cache) = schema_cache {
        cache.invalidate(db_name);
    }
    match SchemaService::load_with_cache(conn, db_name, schema_cache) {
        Ok(new_schema) => {
            *schema = new_schema.clone();
            rebuild(new_schema);
            writeln!(writer, "Schema refreshed.").ok();
        }
        Err(e) => eprintln!("error: could not refresh schema: {}", e),
    }
}
```

- [ ] **Step 8: Update REPL_COMMANDS**

```rust
const REPL_COMMANDS: &[(&str, &str)] = &[
    ("\\d",              "list all tables"),
    ("\\dt",             "list all tables with extended information (column count)"),
    ("\\d <table>",      "describe table (columns, indexes, constraints)"),
    ("\\d+ <table>",     "describe table (extended: + storage, triggers, comments)"),
    ("\\l",              "list databases"),
    ("\\x",              "toggle expanded display"),
    ("\\timing",         "toggle query execution time"),
    ("\\refresh",        "reload schema (after CREATE/DROP/ALTER TABLE)"),
    ("\\history",        "show recent query history"),
    ("\\stats",          "show most frequently queried tables"),
    ("\\stats <table>",  "show most frequently queried columns for table"),
    ("\\help, \\?",      "show this help"),
    ("\\q, exit",        "quit (or Ctrl+D)"),
];
```

- [ ] **Step 9: Update repl::run signature dan dispatch**

Ubah signature dan loop utama di `run()`:

```rust
pub fn run(
    conn: Box<dyn ReplPort>,
    db_name: &str,
    environment: Option<&str>,
    analytics: Option<Arc<dyn AnalyticsPort>>,
    schema_cache: Option<Arc<dyn SchemaCachePort>>,
) -> Result<(), String> {
    let mut schema = SchemaService::load_with_cache(
        conn.as_ref(),
        db_name,
        schema_cache.as_deref(),
    )?;
    let mut rl = build_reedline(schema.clone());

    let prompt = PgrsPrompt {
        db_name: db_name.to_string(),
        environment: environment.map(|s| s.to_string()),
    };

    println!(
        "Connected to '{}'. Type \\help for commands, \\q or Ctrl+D to exit.",
        db_name
    );

    let mut expanded = false;
    let mut timing = false;

    loop {
        match rl.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();
                let mut stdout = io::stdout();
                match trimmed {
                    "\\q" | "exit" => break,
                    "\\help" | "\\?" => println!("{}", repl_help_text()),
                    "\\dt" => handle_dt(&schema, &mut stdout),
                    "\\l" => handle_l(conn.as_ref(), expanded, &mut stdout),
                    "\\x" => {
                        expanded = !expanded;
                        println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                    }
                    "\\timing" => {
                        timing = !timing;
                        println!("Timing is {}.", if timing { "on" } else { "off" });
                    }
                    "\\refresh" => handle_refresh(
                        conn.as_ref(),
                        db_name,
                        &mut schema,
                        &mut |s| { rl = build_reedline(s); },
                        schema_cache.as_deref(),
                        &mut stdout,
                    ),
                    "\\history" => {
                        match analytics.as_deref() {
                            Some(a) => handle_history(db_name, a, &mut stdout),
                            None => { writeln!(stdout, "Analytics not available.").ok(); }
                        }
                    }
                    "\\stats" => {
                        match analytics.as_deref() {
                            Some(a) => handle_stats(db_name, None, a, &mut stdout),
                            None => { writeln!(stdout, "Analytics not available.").ok(); }
                        }
                    }
                    "" => {}
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
                            handle_d(&schema, &mut stdout);
                        } else if let Some(tbl) = trimmed.strip_prefix("\\stats ") {
                            match analytics.as_deref() {
                                Some(a) => handle_stats(db_name, Some(tbl), a, &mut stdout),
                                None => { writeln!(stdout, "Analytics not available.").ok(); }
                            }
                        } else {
                            handle_sql(
                                conn.as_ref(),
                                trimmed,
                                expanded,
                                timing,
                                db_name,
                                &mut schema,
                                &mut |s| { rl = build_reedline(s); },
                                analytics.as_deref(),
                                schema_cache.as_deref(),
                                &mut stdout,
                            );
                        }
                    }
                }
            }
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) | Ok(Signal::ExternalBreak(_)) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
    }

    println!("Bye.");
    Ok(())
}
```

**Catatan:** `Option<Arc<dyn Trait>>` → `Option<&dyn Trait>` menggunakan `.as_deref()` (Rust 1.70+). `Arc<dyn Trait>` implements `Deref<Target = dyn Trait>`, jadi ini valid.

- [ ] **Step 10: Update existing tests yang memanggil handle_refresh dan handle_sql**

Semua test di `mod.rs` yang memanggil fungsi-fungsi ini perlu diupdate. Contoh perubahan:

**handle_sql (sebelum):**
```rust
handle_sql(&stub, "SELECT 1", false, false, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
```
**handle_sql (sesudah):**
```rust
handle_sql(&stub, "SELECT 1", false, false, "mydb", &mut schema, &mut |_| { rebuilt = true; }, None, None, &mut out);
```

**handle_refresh (sebelum):**
```rust
handle_refresh(&stub, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
```
**handle_refresh (sesudah):**
```rust
handle_refresh(&stub, "mydb", &mut schema, &mut |_| { rebuilt = true; }, None, &mut out);
```

Terapkan pola yang sama untuk semua pemanggilan fungsi tersebut di blok test.

- [ ] **Step 11: Run semua tests**

```bash
cargo test
```

Expected: semua tests PASS. Fix compile errors yang muncul dari signature changes.

- [ ] **Step 12: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): add analytics recording, \\history, and \\stats commands"
```

---

## Task 11: Wire SqliteRepository di app.rs

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Tambah use statements**

```rust
use std::sync::Arc;
use crate::adapters::driven::sqlite_repository::SqliteRepository;
use crate::core::ports::analytics_port::AnalyticsPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;
```

- [ ] **Step 2: Update run_with_dir**

Ubah `run_with_dir` untuk membuat `SqliteRepository` dan pass ke `run_shell`:

```rust
fn run_with_dir(data_dir: PathBuf, args: Vec<String>) -> Result<(), String> {
    let repository = FileConnectionRepository::new(data_dir.join("connections.json"));
    let connection_service = ConnectionService::new(repository);

    let db_path = data_dir.join("pgrs.db");
    let sqlite = SqliteRepository::open(db_path.to_str().unwrap_or("pgrs.db"))
        .map_err(|e| {
            eprintln!("pgrs: SQLite unavailable: {e}");
        })
        .ok()
        .map(Arc::new);

    match args.first().map(String::as_str) {
        Some("shell") => run_shell(&args[1..], &connection_service, sqlite),
        Some("test") => run_test(&args[1..], &connection_service),
        _ => {
            let cli = Cli::new(connection_service);
            cli.run(args)
        }
    }
}
```

- [ ] **Step 3: Update run_shell**

```rust
fn run_shell<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
    sqlite: Option<Arc<SqliteRepository>>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = service.find_connection(name)?;
    let db = PostgresDb::new(&conn)?;

    let analytics: Option<Arc<dyn AnalyticsPort>> =
        sqlite.as_ref().map(|r| Arc::clone(r) as Arc<dyn AnalyticsPort>);
    let schema_cache: Option<Arc<dyn SchemaCachePort>> =
        sqlite.as_ref().map(|r| Arc::clone(r) as Arc<dyn SchemaCachePort>);

    repl::run(Box::new(db), &conn.database, conn.environment.as_deref(), analytics, schema_cache)
}
```

- [ ] **Step 4: Run semua tests**

```bash
cargo test
```

Expected: semua tests PASS.

- [ ] **Step 5: Build release untuk verifikasi**

```bash
cargo build --release
```

Expected: berhasil tanpa warning.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): wire SqliteRepository for analytics and schema caching"
```

---

## Selesai

Setelah semua task selesai:

```bash
cargo test
cargo clippy
cargo build --release
```

Semua harus bersih. Binary `target/release/pgrs` sekarang:
- Menyimpan query history per db-name di `~/.pgrs/pgrs.db`
- Tracking frekuensi table dan column otomatis
- Cache schema ke disk, startup lebih cepat pada sesi berikutnya
- Mendukung `\history`, `\stats`, `\stats <table>` di REPL
