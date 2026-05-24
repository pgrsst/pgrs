use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Mutex;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
use crate::core::ports::analytics_port::AnalyticsPort;

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

const SCHEMA_V2: &str = "
DROP TABLE IF EXISTS schema_columns;
DROP TABLE IF EXISTS schema_tables;
DROP TABLE IF EXISTS column_access;
DROP TABLE IF EXISTS table_access;
DROP TABLE IF EXISTS query_history;

CREATE TABLE query_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    query         TEXT    NOT NULL,
    executed_at   INTEGER NOT NULL,
    UNIQUE(connection_id, query)
);
CREATE INDEX IF NOT EXISTS idx_history_conn ON query_history(connection_id, executed_at);

CREATE TABLE table_access (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    query_id      INTEGER REFERENCES query_history(id),
    accessed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_table_access_conn ON table_access(connection_id, table_name);

CREATE TABLE column_access (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    column_name   TEXT    NOT NULL,
    query_id      INTEGER REFERENCES query_history(id),
    accessed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_column_access_conn ON column_access(connection_id, table_name);

CREATE TABLE schema_tables (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    cached_at     INTEGER NOT NULL,
    PRIMARY KEY (connection_id, table_name)
);

CREATE TABLE schema_columns (
    connection_id INTEGER NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    table_name    TEXT    NOT NULL,
    column_name   TEXT    NOT NULL,
    data_type     TEXT,
    cached_at     INTEGER NOT NULL,
    PRIMARY KEY (connection_id, table_name, column_name)
);
";


pub struct SqliteRepository {
    pub(crate) conn: Mutex<Connection>,
}

impl SqliteRepository {
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

    pub(crate) fn migrate(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            conn.execute_batch(SCHEMA_V2)?;
            conn.pragma_update(None, "user_version", 2)?;
        }
        Ok(())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::schema_cache_port::SchemaCachePort;
    use crate::core::ports::analytics_port::AnalyticsPort;

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

    #[test]
    fn migration_sets_user_version_to_2() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let conn = repo.conn.lock().unwrap();
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn migration_is_idempotent() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
    }

    // Helper untuk tests yang butuh connection
    fn add_conn(repo: &SqliteRepository, name: &str) {
        use crate::core::ports::connection_repository::ConnectionRepository;
        use crate::core::domain::connection::{Connection, TlsMode};
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

        use crate::core::ports::connection_repository::ConnectionRepository;
        repo.delete("mydb").unwrap();
        assert!(repo.get_history("mydb").is_empty());
    }

    // --- ConnectionRepository tests ---

    use crate::core::domain::connection::{Connection, TlsMode};
    use crate::core::ports::connection_repository::ConnectionRepository;

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
    fn list_returns_connections_sorted_by_name() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("zebra")).unwrap();
        repo.add(sample_conn("alpha")).unwrap();
        repo.add(sample_conn("middle")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "middle");
        assert_eq!(list[2].name, "zebra");
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
}
