# SQLite Connections Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate named connections from `~/.pgrs/connections.json` ke tabel `connections` di `pgrs.db`, dan hubungkan semua tabel analytics + schema cache ke connections via foreign key.

**Architecture:** `SqliteRepository` menjadi satu-satunya repository — implementasi `ConnectionRepository`, `AnalyticsPort`, dan `SchemaCachePort` sekaligus. Semua tabel analytics/schema pakai `connection_id INTEGER FK REFERENCES connections(id) ON DELETE CASCADE`. `AnalyticsPort` dan `SchemaCachePort` traits rename parameter `db_name` → `connection_name` dan resolve ke `connection_id` secara internal.

**Tech Stack:** Rust, rusqlite, reedline, tempfile (tests)

---

## File Map

| File | Status | Perubahan |
|------|--------|-----------|
| `src/adapters/driven/sqlite_repository.rs` | modify | Schema rewrite, tambah ConnectionRepository impl, update SchemaCachePort + AnalyticsPort impls |
| `src/core/ports/analytics_port.rs` | modify | Rename param `db_name` → `connection_name` |
| `src/core/ports/schema_cache_port.rs` | modify | Rename param `db_name` → `connection_name` |
| `src/core/services/schema/service.rs` | modify | Rename param di `load_with_cache` + update MockCache |
| `src/adapters/driving/repl/mod.rs` | modify | Tambah `connection_name` param, pisahkan dari `db_name` (display only) |
| `src/app.rs` | modify | Ganti `FileConnectionRepository` dengan `SqliteRepository`, pass `conn.name` |
| `src/adapters/driven/file_connection_repository.rs` | delete | Tidak lagi digunakan |
| `src/adapters/driven/mod.rs` | modify | Hapus pub mod file_connection_repository |

---

## Task 1: Tambah `connections` table ke SCHEMA_V1 + impl `ConnectionRepository`

**Files:**
- Modify: `src/adapters/driven/sqlite_repository.rs`

### Step 1.1: Tulis failing tests untuk ConnectionRepository

Di bagian `#[cfg(test)]` di akhir `sqlite_repository.rs`, tambahkan:

```rust
// --- ConnectionRepository tests ---

fn sample_conn(name: &str) -> Connection {
    Connection {
        name: name.to_string(),
        host: "localhost".to_string(),
        port: 5432,
        username: "user".to_string(),
        password: "pass".to_string(),
        database: "db".to_string(),
        tls: TlsMode::Disable,
        environment: None,
        id: None,
    }
}

#[test]
fn add_connection_and_list() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "prod");
    assert_eq!(list[0].host, "localhost");
}

#[test]
fn add_duplicate_returns_error() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    let err = repo.add(sample_conn("prod")).unwrap_err();
    assert_eq!(err, "connection 'prod' already exists");
}

#[test]
fn list_returns_empty_initially() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    assert!(repo.list().unwrap().is_empty());
}

#[test]
fn delete_removes_connection() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    repo.delete("prod").unwrap();
    assert!(repo.list().unwrap().is_empty());
}

#[test]
fn delete_returns_error_when_not_found() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let err = repo.delete("ghost").unwrap_err();
    assert_eq!(err, "connection 'ghost' not found");
}

#[test]
fn get_connection_by_name() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    let c = repo.get_connection("prod").unwrap();
    assert_eq!(c.name, "prod");
    assert_eq!(c.port, 5432);
}

#[test]
fn get_connection_returns_error_when_not_found() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let err = repo.get_connection("ghost").unwrap_err();
    assert_eq!(err, "connection 'ghost' not found");
}

#[test]
fn update_connection() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    let mut updated = sample_conn("prod");
    updated.database = "newdb".to_string();
    repo.update(updated).unwrap();
    let c = repo.get_connection("prod").unwrap();
    assert_eq!(c.database, "newdb");
    assert_eq!(c.host, "localhost");
}

#[test]
fn update_returns_error_when_not_found() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let err = repo.update(sample_conn("ghost")).unwrap_err();
    assert_eq!(err, "connection 'ghost' not found");
}

#[test]
fn rename_connection() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    repo.rename("prod", "production").unwrap();
    assert!(repo.get_connection("production").is_ok());
    assert_eq!(repo.get_connection("prod").unwrap_err(), "connection 'prod' not found");
}

#[test]
fn rename_returns_error_when_not_found() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let err = repo.rename("ghost", "new").unwrap_err();
    assert_eq!(err, "connection 'ghost' not found");
}

#[test]
fn rename_returns_error_when_new_name_exists() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    repo.add(sample_conn("prod")).unwrap();
    repo.add(sample_conn("staging")).unwrap();
    let err = repo.rename("prod", "staging").unwrap_err();
    assert_eq!(err, "connection 'staging' already exists");
}

#[test]
fn connection_with_tls_and_environment_round_trips() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let c = Connection {
        name: "secure".to_string(),
        host: "db.example.com".to_string(),
        port: 5433,
        username: "admin".to_string(),
        password: "secret".to_string(),
        database: "prod_db".to_string(),
        tls: TlsMode::VerifyFull,
        environment: Some("production".to_string()),
        id: Some("abc123".to_string()),
    };
    repo.add(c.clone()).unwrap();
    let loaded = repo.get_connection("secure").unwrap();
    assert_eq!(loaded.tls, TlsMode::VerifyFull);
    assert_eq!(loaded.environment, Some("production".to_string()));
    assert_eq!(loaded.id, Some("abc123".to_string()));
    assert_eq!(loaded.port, 5433);
}
```

- [ ] **Step 1.2: Jalankan tests untuk verifikasi gagal**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo test add_connection_and_list 2>&1 | tail -5
```

Expected: FAIL — `cannot find function 'add' in ... SqliteRepository` atau compile error.

- [ ] **Step 1.3: Tambah `connections` table ke SCHEMA_V1**

Di `sqlite_repository.rs`, update `const SCHEMA_V1` menjadi:

```rust
const SCHEMA_V1: &str = "
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
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    db_name     TEXT    NOT NULL,
    query       TEXT    NOT NULL,
    executed_at INTEGER NOT NULL,
    UNIQUE(db_name, query)
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
```

- [ ] **Step 1.4: Tambah helper methods + implement `ConnectionRepository`**

Tambahkan setelah blok `impl SqliteRepository { ... }` yang sudah ada:

```rust
impl SqliteRepository {
    fn tls_from_str(s: &str) -> crate::core::domain::connection::TlsMode {
        use crate::core::domain::connection::TlsMode;
        match s {
            "require" => TlsMode::Require,
            "verify-full" => TlsMode::VerifyFull,
            _ => TlsMode::Disable,
        }
    }

    fn connection_id_for(conn: &rusqlite::Connection, name: &str) -> Option<i64> {
        conn.query_row(
            "SELECT id FROM connections WHERE name = ?1",
            rusqlite::params![name],
            |r| r.get(0),
        )
        .ok()
    }
}

impl crate::core::ports::connection_repository::ConnectionRepository for SqliteRepository {
    fn add(&self, connection: crate::core::domain::connection::Connection) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO connections (name, host, port, username, password, database, tls, environment, uuid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                connection.name,
                connection.host,
                connection.port as i64,
                connection.username,
                connection.password,
                connection.database,
                connection.tls.to_string(),
                connection.environment,
                connection.id,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                format!("connection '{}' already exists", connection.name)
            } else {
                e.to_string()
            }
        })?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<crate::core::domain::connection::Connection>, String> {
        use crate::core::domain::connection::Connection;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name, host, port, username, password, database, tls, environment, uuid
                 FROM connections ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let tls_str: String = r.get(6)?;
                Ok(Connection {
                    name: r.get(0)?,
                    host: r.get(1)?,
                    port: r.get::<_, i64>(2)? as u16,
                    username: r.get(3)?,
                    password: r.get(4)?,
                    database: r.get(5)?,
                    tls: SqliteRepository::tls_from_str(&tls_str),
                    environment: r.get(7)?,
                    id: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("DELETE FROM connections WHERE name = ?1", rusqlite::params![name])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("connection '{}' not found", name));
        }
        Ok(())
    }

    fn get_connection(&self, name: &str) -> Result<crate::core::domain::connection::Connection, String> {
        use crate::core::domain::connection::Connection;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT name, host, port, username, password, database, tls, environment, uuid
             FROM connections WHERE name = ?1",
            rusqlite::params![name],
            |r| {
                let tls_str: String = r.get(6)?;
                Ok(Connection {
                    name: r.get(0)?,
                    host: r.get(1)?,
                    port: r.get::<_, i64>(2)? as u16,
                    username: r.get(3)?,
                    password: r.get(4)?,
                    database: r.get(5)?,
                    tls: SqliteRepository::tls_from_str(&tls_str),
                    environment: r.get(7)?,
                    id: r.get(8)?,
                })
            },
        )
        .map_err(|e| {
            if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                format!("connection '{}' not found", name)
            } else {
                e.to_string()
            }
        })
    }

    fn update(&self, connection: crate::core::domain::connection::Connection) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE connections SET host=?1, port=?2, username=?3, password=?4,
                 database=?5, tls=?6, environment=?7, uuid=?8 WHERE name=?9",
                rusqlite::params![
                    connection.host,
                    connection.port as i64,
                    connection.username,
                    connection.password,
                    connection.database,
                    connection.tls.to_string(),
                    connection.environment,
                    connection.id,
                    connection.name,
                ],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("connection '{}' not found", connection.name));
        }
        Ok(())
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE connections SET name = ?1 WHERE name = ?2",
                rusqlite::params![new_name, old_name],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint failed") {
                    format!("connection '{}' already exists", new_name)
                } else {
                    e.to_string()
                }
            })?;
        if n == 0 {
            return Err(format!("connection '{}' not found", old_name));
        }
        Ok(())
    }
}
```

Tambahkan juga import di bagian atas file (setelah yang sudah ada):
```rust
use crate::core::domain::connection::{Connection, TlsMode};
```

- [ ] **Step 1.5: Jalankan tests**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo test 2>&1 | tail -20
```

Expected: semua tests pass. Test `open_in_memory_creates_schema` mungkin perlu update — cukup tambahkan pengecekan connections table:

```rust
#[test]
fn open_in_memory_creates_schema() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let conn = repo.conn.lock().unwrap();
    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('query_history','connections')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}
```

- [ ] **Step 1.6: Commit**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && git add src/adapters/driven/sqlite_repository.rs && git commit -m "feat(sqlite): add connections table + implement ConnectionRepository"
```

---

## Task 2: Rewrite analytics/schema tables ke `connection_id` FK + update impls

**Files:**
- Modify: `src/adapters/driven/sqlite_repository.rs`
- Modify: `src/core/ports/analytics_port.rs`
- Modify: `src/core/ports/schema_cache_port.rs`
- Modify: `src/core/services/schema/service.rs`

### Step 2.1: Rename trait params

**`src/core/ports/analytics_port.rs`** — ganti `db_name` → `connection_name`:

```rust
use crate::core::domain::analytics::{FreqEntry, HistoryEntry};

pub trait AnalyticsPort: Send + Sync {
    fn record_query(
        &self,
        connection_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    );
    fn get_history(&self, connection_name: &str) -> Vec<HistoryEntry>;
    fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry>;
    fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry>;
}
```

**`src/core/ports/schema_cache_port.rs`** — ganti `db_name` → `connection_name`:

```rust
use std::collections::HashMap;

pub trait SchemaCachePort: Send + Sync {
    fn save_schema(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>);
    fn load_schema(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>>;
    fn invalidate(&self, connection_name: &str);
}
```

**`src/core/services/schema/service.rs`** — rename param `db_name` → `connection_name` di `load_with_cache` dan update MockCache:

```rust
pub fn load_with_cache(
    conn: &dyn SchemaPort,
    connection_name: &str,
    cache: Option<&dyn SchemaCachePort>,
) -> Result<Self, String> {
    if let Some(cache) = cache
        && let Some(columns) = cache.load_schema(connection_name)
    {
        let mut tables: Vec<String> = columns.keys().cloned().collect();
        tables.sort();
        return Ok(Self { tables, columns });
    }
    let result = Self::load(conn)?;
    if let Some(cache) = cache {
        cache.save_schema(connection_name, &result.columns);
    }
    Ok(result)
}
```

Di tests `schema/service.rs`, update `MockCache` impl (hanya rename param, no logic change):

```rust
impl SchemaCachePort for MockCache {
    fn save_schema(&self, _connection_name: &str, schema: &HashMap<String, Vec<String>>) {
        *self.stored.write().unwrap() = Some(schema.clone());
    }
    fn load_schema(&self, _connection_name: &str) -> Option<HashMap<String, Vec<String>>> {
        self.stored.read().unwrap().clone()
    }
    fn invalidate(&self, _connection_name: &str) {
        *self.stored.write().unwrap() = None;
    }
}
```

- [ ] **Step 2.2: Jalankan cargo check**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo check 2>&1 | grep "^error" | head -20
```

Expected: errors di `sqlite_repository.rs` (masih pakai `db_name` column di SQL), dan di `repl/mod.rs` (pakai param lama). Ini normal — lanjut.

- [ ] **Step 2.3: Rewrite SCHEMA_V1 analytics + schema tables**

Update `const SCHEMA_V1` di `sqlite_repository.rs`:

```rust
const SCHEMA_V1: &str = "
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
";
```

Tambahkan juga `PRAGMA foreign_keys = ON` di `open()` dan `open_in_memory()`:

```rust
pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let repo = Self { conn: Mutex::new(conn) };
    repo.migrate()?;
    Ok(repo)
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let repo = Self { conn: Mutex::new(conn) };
    repo.migrate()?;
    Ok(repo)
}
```

- [ ] **Step 2.4: Update `SchemaCachePort` impl di `SqliteRepository`**

Ganti seluruh `impl SchemaCachePort for SqliteRepository` dengan:

```rust
impl SchemaCachePort for SqliteRepository {
    fn save_schema(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>) {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = Self::connection_id_for(&conn, connection_name) else {
            eprintln!("pgrs: schema cache: unknown connection '{connection_name}'");
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = (|| -> Result<(), rusqlite::Error> {
            conn.execute(
                "DELETE FROM schema_columns WHERE connection_id = ?1",
                rusqlite::params![connection_id],
            )?;
            conn.execute(
                "DELETE FROM schema_tables WHERE connection_id = ?1",
                rusqlite::params![connection_id],
            )?;
            for (table, columns) in schema {
                conn.execute(
                    "INSERT OR REPLACE INTO schema_tables (connection_id, table_name, cached_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![connection_id, table, now],
                )?;
                for col in columns {
                    conn.execute(
                        "INSERT OR REPLACE INTO schema_columns (connection_id, table_name, column_name, cached_at) VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![connection_id, table, col, now],
                    )?;
                }
            }
            Ok(())
        })() {
            eprintln!("pgrs: schema cache write failed: {e}");
        }
    }

    fn load_schema(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>> {
        let conn = self.conn.lock().unwrap();
        let connection_id = Self::connection_id_for(&conn, connection_name)?;
        let result: Result<Vec<(String, String)>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT table_name, column_name FROM schema_columns
                 WHERE connection_id = ?1 ORDER BY table_name, rowid",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
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

    fn invalidate(&self, connection_name: &str) {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = Self::connection_id_for(&conn, connection_name) else {
            return;
        };
        if let Err(e) = conn.execute(
            "DELETE FROM schema_columns WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        ) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
            return;
        }
        if let Err(e) = conn.execute(
            "DELETE FROM schema_tables WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        ) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
    }
}
```

- [ ] **Step 2.5: Update `AnalyticsPort` impl di `SqliteRepository`**

Ganti seluruh `impl AnalyticsPort for SqliteRepository` dengan:

```rust
impl AnalyticsPort for SqliteRepository {
    fn record_query(
        &self,
        connection_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    ) {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = Self::connection_id_for(&conn, connection_name) else {
            eprintln!("pgrs: analytics: unknown connection '{connection_name}'");
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = (|| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO query_history (connection_id, query, executed_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(connection_id, query) DO UPDATE SET executed_at = excluded.executed_at",
                rusqlite::params![connection_id, query, now],
            )?;
            let query_id: i64 = conn.query_row(
                "SELECT id FROM query_history WHERE connection_id = ?1 AND query = ?2",
                rusqlite::params![connection_id, query],
                |r| r.get(0),
            )?;
            for table in tables {
                conn.execute(
                    "INSERT INTO table_access (connection_id, table_name, query_id, accessed_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![connection_id, table, query_id, now],
                )?;
            }
            for (table, column) in columns {
                conn.execute(
                    "INSERT INTO column_access (connection_id, table_name, column_name, query_id, accessed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![connection_id, table, column, query_id, now],
                )?;
            }
            Ok(())
        })() {
            eprintln!("pgrs: analytics write failed: {e}");
        }
    }

    fn get_history(&self, connection_name: &str) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = Self::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<HistoryEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT query, executed_at FROM query_history
                 WHERE connection_id = ?1 ORDER BY executed_at DESC LIMIT 50",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(HistoryEntry { query: r.get(0)?, executed_at: r.get(1)? })
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

    fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = Self::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT table_name, COUNT(*) as cnt FROM table_access
                 WHERE connection_id = ?1 GROUP BY table_name ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(FreqEntry { name: r.get(0)?, count: r.get::<_, i64>(1)? as u64 })
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

    fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = Self::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT column_name, COUNT(*) as cnt FROM column_access
                 WHERE connection_id = ?1 AND table_name = ?2
                 GROUP BY column_name ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id, table], |r| {
                Ok(FreqEntry { name: r.get(0)?, count: r.get::<_, i64>(1)? as u64 })
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

- [ ] **Step 2.6: Update tests di `sqlite_repository.rs` yang pakai analytics/schema**

Semua tests lama pakai `"mydb"` langsung. Sekarang perlu connection dulu. Ganti seluruh bagian `// --- SchemaCachePort tests ---` dan `// --- AnalyticsPort tests ---` dengan versi yang tambah connection lebih dulu:

```rust
// Helper untuk tests
fn add_conn(repo: &SqliteRepository, name: &str) {
    repo.add(Connection {
        name: name.to_string(),
        host: "localhost".to_string(),
        port: 5432,
        username: "u".to_string(),
        password: "p".to_string(),
        database: "db".to_string(),
        tls: TlsMode::Disable,
        environment: None,
        id: None,
    }).unwrap();
}

// --- SchemaCachePort tests ---

#[test]
fn save_and_load_schema_round_trip() {
    use std::collections::HashMap;
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
    let mut schema = HashMap::new();
    schema.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
    schema.insert("orders".to_string(), vec!["id".to_string(), "user_id".to_string()]);

    repo.save_schema("mydb", &schema);
    let loaded = repo.load_schema("mydb").unwrap();
    assert_eq!(loaded["users"], vec!["id", "email"]);
    assert_eq!(loaded["orders"], vec!["id", "user_id"]);
}

#[test]
fn load_schema_returns_none_for_unknown_connection() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    assert!(repo.load_schema("ghost").is_none());
}

#[test]
fn invalidate_removes_schema() {
    use std::collections::HashMap;
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
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
    add_conn(&repo, "mydb");

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

// --- AnalyticsPort tests ---

#[test]
fn record_query_and_get_history() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
    repo.record_query("mydb", "SELECT 1", &[], &[]);
    repo.record_query("mydb", "SELECT 2", &[], &[]);

    let history = repo.get_history("mydb");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].query, "SELECT 2");
}

#[test]
fn record_query_deduplicates_same_query() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
    repo.record_query("mydb", "SELECT * FROM users", &[], &[]);
    repo.record_query("mydb", "SELECT * FROM users", &[], &[]);
    repo.record_query("mydb", "SELECT * FROM users", &[], &[]);

    let history = repo.get_history("mydb");
    assert_eq!(history.len(), 1);
}

#[test]
fn record_query_dedup_updates_executed_at() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
    repo.record_query("mydb", "SELECT 1", &[], &[]);
    repo.record_query("mydb", "SELECT 1", &[], &[]);
    assert_eq!(repo.get_history("mydb").len(), 1);
}

#[test]
fn get_history_filters_by_connection_name() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "db1");
    add_conn(&repo, "db2");
    repo.record_query("db1", "SELECT 1", &[], &[]);
    repo.record_query("db2", "SELECT 2", &[], &[]);

    let history = repo.get_history("db1");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].query, "SELECT 1");
}

#[test]
fn get_history_returns_empty_for_unknown_connection() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    assert!(repo.get_history("ghost").is_empty());
}

#[test]
fn get_frequent_tables_ordered_by_count() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
    let users = vec!["users".to_string()];
    let orders = vec!["orders".to_string()];
    repo.record_query("mydb", "SELECT * FROM users", &users, &[]);
    repo.record_query("mydb", "SELECT * FROM users 2", &users, &[]);
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
    add_conn(&repo, "mydb");
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
}

#[test]
fn get_frequent_tables_returns_empty_for_unknown_connection() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    assert!(repo.get_frequent_tables("ghost").is_empty());
}

#[test]
fn delete_connection_cascades_to_history() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    add_conn(&repo, "mydb");
    repo.record_query("mydb", "SELECT 1", &[], &[]);
    assert_eq!(repo.get_history("mydb").len(), 1);

    repo.delete("mydb").unwrap();
    // connection gone, get_history returns empty (connection_id lookup fails)
    assert!(repo.get_history("mydb").is_empty());
}
```

- [ ] **Step 2.7: Jalankan semua tests**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo test 2>&1 | tail -30
```

Expected: semua pass kecuali mungkin compile errors di `repl/mod.rs` (pakai `db_name` di `SqlOptions` dan `RecordingAnalytics`). Jika ada, lanjut ke step berikutnya.

- [ ] **Step 2.8: Fix `RecordingAnalytics` mock di `repl/mod.rs` tests**

Cari di `repl/mod.rs` (sekitar baris 868):
```rust
fn record_query(&self, db_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
    self.recorded.write().unwrap().push((db_name.to_string(), query.to_string()));
```

Ganti param name:
```rust
fn record_query(&self, connection_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
    self.recorded.write().unwrap().push((connection_name.to_string(), query.to_string()));
```

- [ ] **Step 2.9: Jalankan semua tests**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo test 2>&1 | tail -20
```

Expected: semua pass.

- [ ] **Step 2.10: Commit**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && git add src/adapters/driven/sqlite_repository.rs src/core/ports/analytics_port.rs src/core/ports/schema_cache_port.rs src/core/services/schema/service.rs src/adapters/driving/repl/mod.rs && git commit -m "feat(sqlite): rewrite analytics/schema tables to use connection_id FK"
```

---

## Task 3: Update `repl/mod.rs` — pisahkan `connection_name` dari `db_name`

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 3.1: Tambah `connection_name` ke `SqlOptions` dan `repl::run`**

Ganti `struct SqlOptions`:
```rust
struct SqlOptions<'a> {
    expanded: bool,
    timing: bool,
    connection_name: &'a str,
    analytics: Option<&'a dyn AnalyticsPort>,
    schema_cache: Option<&'a dyn SchemaCachePort>,
}
```

Ganti `fn handle_refresh` signature — rename param `db_name` → `connection_name`:
```rust
fn handle_refresh(
    conn: &dyn SchemaPort,
    connection_name: &str,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    schema_cache: Option<&dyn SchemaCachePort>,
    writer: &mut impl Write,
) {
    if let Some(cache) = schema_cache {
        cache.invalidate(connection_name);
    }
    match SchemaService::load_with_cache(conn, connection_name, schema_cache) {
        Ok(new_schema) => {
            *schema = new_schema.clone();
            rebuild(new_schema);
            writeln!(writer, "Schema refreshed.").ok();
        }
        Err(e) => eprintln!("error: could not refresh schema: {}", e),
    }
}
```

Ganti `fn handle_history` signature:
```rust
fn handle_history(connection_name: &str, analytics: &dyn AnalyticsPort, writer: &mut impl Write) {
    let history = analytics.get_history(connection_name);
    // ... body tidak berubah
```

Ganti `fn handle_stats` signature (baris ~233):
```rust
fn handle_stats(
    connection_name: &str,
    table: Option<&str>,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
) {
    // ganti semua panggilan db_name → connection_name di dalam body
    // analytics.get_frequent_tables(connection_name)
    // analytics.get_frequent_columns(connection_name, tbl)
```

Ganti `fn handle_sql` — update pakai `opts.connection_name`:
```rust
if let Some(analytics) = opts.analytics {
    let tables = extract_referenced_tables(query);
    let columns = extract_column_refs(query, schema);
    analytics.record_query(opts.connection_name, query, &tables, &columns);
}

if is_ddl(query)
    && let Ok(new_schema) = SchemaService::load_with_cache(conn, opts.connection_name, opts.schema_cache)
```

Ganti `pub fn run` — tambah param `connection_name: &str`, keep `db_name: &str` untuk display:
```rust
pub fn run(
    conn: Box<dyn ReplPort>,
    db_name: &str,
    connection_name: &str,
    environment: Option<&str>,
    analytics: Option<Arc<dyn AnalyticsPort>>,
    schema_cache: Option<Arc<dyn SchemaCachePort>>,
) -> Result<(), String> {
    let mut schema = SchemaService::load_with_cache(
        conn.as_ref(),
        connection_name,
        schema_cache.as_deref(),
    )?;
    // PgrsPrompt tetap pakai db_name untuk display
    let prompt = PgrsPrompt {
        db_name: db_name.to_string(),
        environment: environment.map(|s| s.to_string()),
    };
    println!(
        "Connected to '{}'. Type \\help for commands, \\q or Ctrl+D to exit.",
        db_name
    );
    // ...
```

Update semua panggilan di dalam loop `repl::run` yang pakai `db_name` untuk analytics/cache → ganti ke `connection_name`:

```rust
"\\refresh" => handle_refresh(
    conn.as_ref(),
    connection_name,   // <-- ganti
    &mut schema,
    &mut |s| { rl = build_reedline(s); },
    schema_cache.as_deref(),
    &mut stdout,
),
"\\history" => {
    match analytics.as_deref() {
        Some(a) => handle_history(connection_name, a, &mut stdout),   // <-- ganti
        None => { writeln!(stdout, "Analytics not available.").ok(); }
    }
}
"\\stats" => {
    match analytics.as_deref() {
        Some(a) => handle_stats(connection_name, None, a, &mut stdout),   // <-- ganti
        None => { writeln!(stdout, "Analytics not available.").ok(); }
    }
}
// ...
} else if let Some(tbl) = trimmed.strip_prefix("\\stats ") {
    match analytics.as_deref() {
        Some(a) => handle_stats(connection_name, Some(tbl), a, &mut stdout),   // <-- ganti
        ...
    }
} else {
    handle_sql(
        conn.as_ref(),
        trimmed,
        &SqlOptions {
            expanded,
            timing,
            connection_name,   // <-- ganti field name
            analytics: analytics.as_deref(),
            schema_cache: schema_cache.as_deref(),
        },
        ...
    )
}
```

- [ ] **Step 3.2: Update tests di `repl/mod.rs` yang buat `SqlOptions`**

Cari semua `SqlOptions { ... db_name: "mydb" ... }` di tests (sekitar baris 576–625) dan ganti `db_name` → `connection_name`:

```rust
// Sebelum:
&SqlOptions { expanded: false, timing: false, db_name: "mydb", analytics: None, schema_cache: None }
// Sesudah:
&SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }
```

Juga update `handle_history("mydb", ...)` → tetap sama (nama fungsi tidak berubah, tapi param sekarang `connection_name`).

- [ ] **Step 3.3: Jalankan tests**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo test 2>&1 | tail -20
```

Expected: semua pass. (Satu-satunya compile error yang mungkin ada adalah di `app.rs` karena signature `repl::run` berubah — itu normal, selesai di Task 4.)

- [ ] **Step 3.4: Commit**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && git add src/adapters/driving/repl/mod.rs && git commit -m "feat(repl): separate connection_name from db_name for analytics"
```

---

## Task 4: Update `app.rs` + hapus `FileConnectionRepository`

**Files:**
- Modify: `src/app.rs`
- Modify: `src/adapters/driven/mod.rs`
- Delete: `src/adapters/driven/file_connection_repository.rs`

- [ ] **Step 4.1: Update `app.rs`**

Ganti seluruh isi `src/app.rs` dengan:

```rust
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::driven::postgres_db::PostgresDb;
use crate::adapters::driven::sqlite_repository::SqliteRepository;
use crate::adapters::driving::cli::Cli;
use crate::adapters::driving::repl;
use crate::core::ports::analytics_port::AnalyticsPort;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::services::connection::service::ConnectionService;

pub fn run() -> Result<(), String> {
    let data_dir = dirs::home_dir()
        .ok_or("could not determine home directory")?
        .join(".pgrs");

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let args: Vec<String> = env::args().skip(1).collect();
    run_with_dir(data_dir, args)
}

fn run_with_dir(data_dir: PathBuf, args: Vec<String>) -> Result<(), String> {
    let db_path = data_dir.join("pgrs.db");
    let sqlite = Arc::new(
        SqliteRepository::open(db_path.to_str().unwrap_or("pgrs.db"))
            .map_err(|e| format!("pgrs: could not open database: {e}"))?,
    );

    let connection_service = ConnectionService::new(Arc::clone(&sqlite));

    match args.first().map(String::as_str) {
        Some("shell") => run_shell(&args[1..], &connection_service, Arc::clone(&sqlite)),
        Some("test") => run_test(&args[1..], &connection_service),
        _ => {
            let cli = Cli::new(connection_service);
            cli.run(args)
        }
    }
}

fn run_shell<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
    sqlite: Arc<SqliteRepository>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs shell <connection-name>")?;
    let conn = service.find_connection(name)?;
    let db = PostgresDb::new(&conn)?;

    let analytics: Option<Arc<dyn AnalyticsPort>> =
        Some(Arc::clone(&sqlite) as Arc<dyn AnalyticsPort>);
    let schema_cache: Option<Arc<dyn SchemaCachePort>> =
        Some(Arc::clone(&sqlite) as Arc<dyn SchemaCachePort>);

    repl::run(
        Box::new(db),
        &conn.database,
        &conn.name,
        conn.environment.as_deref(),
        analytics,
        schema_cache,
    )
}

fn run_test<R: ConnectionRepository>(
    args: &[String],
    service: &ConnectionService<R>,
) -> Result<(), String> {
    let name = args.first().ok_or("usage: pgrs test <connection-name>")?;
    let conn = service.find_connection(name)?;
    let conn_name = &conn.name;
    let db = PostgresDb::new(&conn)
        .map_err(|e| format!("connection '{}' failed: {}", conn_name, e))?;
    db.execute("SELECT 1")
        .map_err(|e| format!("connection '{}' failed: {}", conn_name, e))?;
    println!("connection '{}' ok", conn_name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_with_dir_no_args_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run_with_dir(dir.path().to_path_buf(), vec![]).is_ok());
    }

    #[test]
    fn run_with_dir_unknown_command_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(dir.path().to_path_buf(), vec!["badcmd".to_string()]).unwrap_err();
        assert!(err.contains("badcmd"), "error should mention the unknown command, got: {err}");
    }

    #[test]
    fn run_with_dir_shell_without_name_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(dir.path().to_path_buf(), vec!["shell".to_string()]).unwrap_err();
        assert!(err.contains("usage"), "error should show usage hint, got: {err}");
    }

    #[test]
    fn run_with_dir_test_without_name_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(dir.path().to_path_buf(), vec!["test".to_string()]).unwrap_err();
        assert!(err.contains("usage"), "error should show usage hint, got: {err}");
    }

    #[test]
    fn run_with_dir_shell_unknown_connection_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(
            dir.path().to_path_buf(),
            vec!["shell".to_string(), "ghost".to_string()],
        ).unwrap_err();
        assert!(err.contains("not found"), "error should say not found, got: {err}");
    }

    #[test]
    fn run_with_dir_test_unknown_connection_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with_dir(
            dir.path().to_path_buf(),
            vec!["test".to_string(), "ghost".to_string()],
        ).unwrap_err();
        assert!(err.contains("not found"), "error should say not found, got: {err}");
    }
}
```

Catatan: `ConnectionService::new` perlu menerima `Arc<SqliteRepository>`. Cek apakah signature `ConnectionService::new` saat ini butuh `R: ConnectionRepository` generic — jika iya, `Arc<SqliteRepository>` harus implement `ConnectionRepository`. Kita perlu tambahkan blanket impl atau wrap (lihat step 4.2).

- [ ] **Step 4.2: Pastikan `Arc<SqliteRepository>` implement `ConnectionRepository`**

Di `src/adapters/driven/sqlite_repository.rs`, tambahkan:

```rust
impl crate::core::ports::connection_repository::ConnectionRepository for Arc<SqliteRepository> {
    fn add(&self, connection: Connection) -> Result<(), String> {
        (**self).add(connection)
    }
    fn list(&self) -> Result<Vec<Connection>, String> {
        (**self).list()
    }
    fn delete(&self, name: &str) -> Result<(), String> {
        (**self).delete(name)
    }
    fn get_connection(&self, name: &str) -> Result<Connection, String> {
        (**self).get_connection(name)
    }
    fn update(&self, connection: Connection) -> Result<(), String> {
        (**self).update(connection)
    }
    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), String> {
        (**self).rename(old_name, new_name)
    }
}
```

- [ ] **Step 4.3: Update `src/adapters/driven/mod.rs`**

```rust
pub mod postgres_db;
pub mod sqlite_repository;
```

(Hapus baris `pub mod file_connection_repository;`)

- [ ] **Step 4.4: Hapus `file_connection_repository.rs`**

```bash
rm /home/fakhrulnugroho/work/natakode/pgrs/src/adapters/driven/file_connection_repository.rs
```

- [ ] **Step 4.5: Jalankan semua tests**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && cargo test 2>&1 | tail -30
```

Expected: semua pass.

- [ ] **Step 4.6: Commit**

```bash
cd /home/fakhrulnugroho/work/natakode/pgrs && git add -A && git commit -m "feat(app): migrate connections to SQLite, remove FileConnectionRepository"
```
