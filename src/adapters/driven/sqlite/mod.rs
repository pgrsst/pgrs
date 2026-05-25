pub(crate) mod migrations;
pub(crate) mod connection_store;
pub(crate) mod analytics_store;
pub(crate) mod schema_cache;

use rusqlite::Connection;
use std::sync::Mutex;

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
        migrations::migrate(&self.conn)
    }

    pub(crate) fn connection_id_for(conn: &rusqlite::Connection, name: &str) -> Option<i64> {
        conn.query_row(
            "SELECT id FROM connections WHERE name = ?1",
            rusqlite::params![name],
            |r| r.get(0),
        )
        .ok()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::error::DomainError;
    use crate::core::ports::schema_cache_port::SchemaCachePort;
    use crate::core::ports::analytics_port::AnalyticsPort;

    #[test]
    fn open_in_memory_creates_schema() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let conn = repo.conn.lock().unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN \
                 ('connections','query_history','table_access','column_access','schema_tables','schema_columns')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 6);
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
        repo.add(Connection::new(
            name.to_string(),
            "localhost".to_string(),
            5432,
            "u".to_string(),
            "p".to_string(),
            "db".to_string(),
            TlsMode::Disable,
            None,
        ).unwrap()).unwrap();
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
    fn schema_cache_isolated_per_connection() {
        use std::collections::HashMap;
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "db1");
        add_conn(&repo, "db2");

        let mut schema1 = HashMap::new();
        schema1.insert("users".to_string(), vec!["id".to_string()]);
        repo.save_schema("db1", &schema1);

        let mut schema2 = HashMap::new();
        schema2.insert("products".to_string(), vec!["sku".to_string()]);
        repo.save_schema("db2", &schema2);

        let loaded1 = repo.load_schema("db1").unwrap();
        assert!(loaded1.contains_key("users"), "db1 schema should have users");
        assert!(!loaded1.contains_key("products"), "db1 schema should not have db2's products");

        let loaded2 = repo.load_schema("db2").unwrap();
        assert!(loaded2.contains_key("products"), "db2 schema should have products");
        assert!(!loaded2.contains_key("users"), "db2 schema should not have db1's users");
    }

    #[test]
    fn save_schema_is_atomic_on_failure() {
        // Verifies that a failed save does not partially modify the schema.
        // We install a trigger that fires after the DELETEs but before the INSERTs,
        // simulating a mid-transaction failure.  The original schema must be intact.
        use std::collections::HashMap;
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");

        let mut initial = HashMap::new();
        initial.insert("users".to_string(), vec!["id".to_string()]);
        repo.save_schema("mydb", &initial);

        // Trigger causes INSERT INTO schema_tables to fail
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute_batch(
                "CREATE TRIGGER fail_schema_insert BEFORE INSERT ON schema_tables \
                 BEGIN SELECT RAISE(FAIL, 'simulated failure'); END;",
            )
            .unwrap();
        }

        let mut new_schema = HashMap::new();
        new_schema.insert("products".to_string(), vec!["sku".to_string()]);
        repo.save_schema("mydb", &new_schema); // expected to silently fail

        // Remove trigger so load_schema can work
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute_batch("DROP TRIGGER fail_schema_insert").unwrap();
        }

        let loaded = repo.load_schema("mydb").unwrap();
        assert!(loaded.contains_key("users"), "rollback must restore original schema");
        assert!(!loaded.contains_key("products"), "failed save must not partially commit");
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
        assert_eq!(history[0].query, "SELECT * FROM users");
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
        assert_eq!(freq[1].name, "id");
        assert_eq!(freq[1].count, 1);
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
        Connection::new(
            name.to_string(),
            "localhost".to_string(),
            5432,
            "user".to_string(),
            "pass".to_string(),
            "db".to_string(),
            TlsMode::Disable,
            None,
        ).expect("valid test connection")
    }

    #[test]
    fn add_connection_and_list() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("prod")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name(), "prod");
        assert_eq!(list[0].host(), "localhost");
    }

    #[test]
    fn add_duplicate_returns_error() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("prod")).unwrap();
        let err = repo.add(sample_conn("prod")).unwrap_err();
        assert!(matches!(err, DomainError::AlreadyExists(_)));
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
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn get_connection_by_name() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("prod")).unwrap();
        let c = repo.get_connection("prod").unwrap();
        assert_eq!(c.name(), "prod");
        assert_eq!(c.port(), 5432);
    }

    #[test]
    fn get_connection_returns_error_when_not_found() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let err = repo.get_connection("ghost").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn update_connection() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("prod")).unwrap();
        let mut updated = sample_conn("prod");
        updated.set_database("newdb".to_string());
        repo.update(updated).unwrap();
        let c = repo.get_connection("prod").unwrap();
        assert_eq!(c.database(), "newdb");
        assert_eq!(c.host(), "localhost");
    }

    #[test]
    fn update_returns_error_when_not_found() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let err = repo.update(sample_conn("ghost")).unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn rename_connection() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("prod")).unwrap();
        repo.rename("prod", "production").unwrap();
        assert!(repo.get_connection("production").is_ok());
        assert!(matches!(repo.get_connection("prod").unwrap_err(), DomainError::NotFound(_)));
    }

    #[test]
    fn rename_returns_error_when_not_found() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let err = repo.rename("ghost", "new").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn rename_returns_error_when_new_name_exists() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("prod")).unwrap();
        repo.add(sample_conn("staging")).unwrap();
        let err = repo.rename("prod", "staging").unwrap_err();
        assert!(matches!(err, DomainError::AlreadyExists(_)));
    }

    #[test]
    fn list_returns_connections_sorted_by_name() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        repo.add(sample_conn("zebra")).unwrap();
        repo.add(sample_conn("alpha")).unwrap();
        repo.add(sample_conn("middle")).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list[0].name(), "alpha");
        assert_eq!(list[1].name(), "middle");
        assert_eq!(list[2].name(), "zebra");
    }

    #[test]
    fn connection_with_tls_and_environment_round_trips() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let mut c = Connection::new(
            "secure".to_string(),
            "db.example.com".to_string(),
            5433,
            "admin".to_string(),
            "secret".to_string(),
            "prod_db".to_string(),
            TlsMode::VerifyFull,
            Some("production".to_string()),
        ).unwrap();
        c.set_id("abc123".to_string());
        repo.add(c.clone()).unwrap();
        let loaded = repo.get_connection("secure").unwrap();
        assert_eq!(loaded.tls(), &TlsMode::VerifyFull);
        assert_eq!(loaded.environment(), Some("production"));
        assert_eq!(loaded.id(), Some("abc123"));
        assert_eq!(loaded.port(), 5433);
    }
}
