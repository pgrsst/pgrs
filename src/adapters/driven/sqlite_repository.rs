use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Mutex;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
use crate::core::ports::analytics_port::AnalyticsPort;

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
    pub(crate) conn: Mutex<Connection>,
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

    pub(crate) fn migrate(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", &1)?;
        }
        Ok(())
    }
}

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
                "SELECT table_name, column_name FROM schema_columns WHERE db_name = ?1 ORDER BY table_name, rowid",
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
        repo.migrate().unwrap();
    }

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

    // --- AnalyticsPort tests ---

    #[test]
    fn record_query_and_get_history() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.record_query("mydb", "SELECT 1", &[], &[]);
        repo.record_query("mydb", "SELECT 2", &[], &[]);

        let history = repo.get_history("mydb");
        assert_eq!(history.len(), 2);
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
}
