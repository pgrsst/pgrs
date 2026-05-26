pub(crate) mod migrations;
pub(crate) mod connection_store;
pub(crate) mod query_history_store;
pub(crate) mod table_access_store;
pub(crate) mod column_access_store;
pub(crate) mod schema_table_store;
pub(crate) mod schema_column_store;

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
    use crate::core::domain::column_access::ColumnAccess;
    use crate::core::domain::error::DomainError;
    use crate::core::domain::query_history::QueryHistory;
    use crate::core::domain::table_access::TableAccess;
    use crate::core::ports::column_access_repository::ColumnAccessRepository;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use crate::core::ports::query_history_repository::QueryHistoryRepository;
    use crate::core::ports::table_access_repository::TableAccessRepository;

    fn save_history(repo: &SqliteRepository, conn_name: &str, query: &str, ts: i64) -> i64 {
        let conn_id = repo.find_row_id(conn_name).unwrap();
        QueryHistoryRepository::save(repo, &QueryHistory { id: 0, connection_id: conn_id, query: query.to_string(), executed_at: ts }).unwrap()
    }

    fn save_table_access(repo: &SqliteRepository, conn_name: &str, table: &str, qid: Option<i64>, ts: i64) {
        let conn_id = repo.find_row_id(conn_name).unwrap();
        TableAccessRepository::save(repo, &TableAccess { id: 0, connection_id: conn_id, table_name: table.to_string(), query_id: qid, accessed_at: ts }).unwrap();
    }

    fn save_column_access(repo: &SqliteRepository, conn_name: &str, table: &str, col: &str, qid: Option<i64>, ts: i64) {
        let conn_id = repo.find_row_id(conn_name).unwrap();
        ColumnAccessRepository::save(repo, &ColumnAccess { id: 0, connection_id: conn_id, table_name: table.to_string(), column_name: col.to_string(), query_id: qid, accessed_at: ts }).unwrap();
    }

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

    // --- QueryHistoryRepository tests ---

    #[test]
    fn upsert_and_list_recent() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");
        save_history(&repo, "mydb", "SELECT 1", 1000);
        save_history(&repo, "mydb", "SELECT 2", 2000);

        let history = repo.list_recent("mydb", 50);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].query, "SELECT 2");
    }

    #[test]
    fn upsert_deduplicates_same_query() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");
        save_history(&repo, "mydb", "SELECT * FROM users", 1000);
        save_history(&repo, "mydb", "SELECT * FROM users", 2000);
        save_history(&repo, "mydb", "SELECT * FROM users", 3000);

        let history = repo.list_recent("mydb", 50);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].query, "SELECT * FROM users");
    }

    #[test]
    fn upsert_updates_executed_at_on_conflict() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");
        save_history(&repo, "mydb", "SELECT 1", 1000);
        save_history(&repo, "mydb", "SELECT 1", 9999);
        let history = repo.list_recent("mydb", 50);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].executed_at, 9999);
    }

    #[test]
    fn list_recent_filters_by_connection_name() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "db1");
        add_conn(&repo, "db2");
        save_history(&repo, "db1", "SELECT 1", 1000);
        save_history(&repo, "db2", "SELECT 2", 2000);

        let history = repo.list_recent("db1", 50);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].query, "SELECT 1");
    }

    #[test]
    fn list_recent_returns_empty_for_unknown_connection() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        assert!(repo.list_recent("ghost", 50).is_empty());
    }

    // --- TableAccessRepository tests ---

    #[test]
    fn table_access_list_frequent_ordered_by_count() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");
        let qid = save_history(&repo, "mydb", "SELECT * FROM users", 1000);
        save_table_access(&repo, "mydb", "users", Some(qid), 1000);
        let qid2 = save_history(&repo, "mydb", "SELECT * FROM users 2", 2000);
        save_table_access(&repo, "mydb", "users", Some(qid2), 2000);
        let qid3 = save_history(&repo, "mydb", "SELECT * FROM orders", 3000);
        save_table_access(&repo, "mydb", "orders", Some(qid3), 3000);

        let freq = repo.list_frequent("mydb", 100);
        assert_eq!(freq[0].name, "users");
        assert_eq!(freq[0].count, 2);
        assert_eq!(freq[1].name, "orders");
        assert_eq!(freq[1].count, 1);
    }

    #[test]
    fn table_access_list_frequent_returns_empty_for_unknown_connection() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        assert!(repo.list_frequent("ghost", 100).is_empty());
    }

    // --- ColumnAccessRepository tests ---

    #[test]
    fn column_access_list_frequent_by_table() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");
        let qid = save_history(&repo, "mydb", "SELECT email, id FROM users", 1000);
        save_column_access(&repo, "mydb", "users", "email", Some(qid), 1000);
        save_column_access(&repo, "mydb", "users", "email", Some(qid), 1000);
        save_column_access(&repo, "mydb", "users", "id", Some(qid), 1000);

        let freq = repo.list_frequent_by_table("mydb", "users", 100);
        assert_eq!(freq[0].name, "email");
        assert_eq!(freq[0].count, 2);
        assert_eq!(freq[1].name, "id");
        assert_eq!(freq[1].count, 1);
    }

    // --- Cascade delete test ---

    #[test]
    fn delete_connection_cascades_to_history() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        add_conn(&repo, "mydb");
        save_history(&repo, "mydb", "SELECT 1", 1000);
        assert_eq!(repo.list_recent("mydb", 50).len(), 1);

        use crate::core::ports::connection_repository::ConnectionRepository;
        repo.delete("mydb").unwrap();
        assert!(repo.list_recent("mydb", 50).is_empty());
    }

    // --- ConnectionRepository tests ---

    use crate::core::domain::connection::{Connection, TlsMode};

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
