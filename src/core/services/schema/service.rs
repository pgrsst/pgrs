use std::collections::HashMap;
use std::sync::Arc;

use crate::core::ports::schema_port::SchemaPort;
use crate::core::services::schema_cache::service::SchemaCacheService;

#[derive(Clone)]
pub struct SchemaService {
    cache: Option<Arc<SchemaCacheService>>,
    tables: Vec<String>,
    columns: HashMap<String, Vec<String>>,
}

impl SchemaService {
    pub fn new(cache: Option<Arc<SchemaCacheService>>) -> Self {
        Self { cache, tables: vec![], columns: HashMap::new() }
    }

    // Uses `dyn SchemaPort` (not a generic) because callers pass `Box<dyn ReplPort>`,
    // whose concrete type is already erased by the time it reaches this function.
    pub fn load(&mut self, conn: &dyn SchemaPort, connection_name: &str) -> Result<(), String> {
        if let Some(cache) = &self.cache
            && let Some(columns) = cache.load(connection_name)
        {
            self.tables = columns.keys().cloned().collect();
            self.tables.sort();
            self.columns = columns;
            return Ok(());
        }
        let columns = conn.list_columns()?;
        if let Some(cache) = &self.cache {
            cache.save(connection_name, &columns);
        }
        self.tables = columns.keys().cloned().collect();
        self.tables.sort();
        self.columns = columns;
        Ok(())
    }

    pub fn refresh(&mut self, conn: &dyn SchemaPort, connection_name: &str) -> Result<(), String> {
        if let Some(cache) = &self.cache {
            cache.invalidate(connection_name);
        }
        self.load(conn, connection_name)
    }

    pub fn tables(&self) -> &[String] {
        &self.tables
    }

    pub fn columns_for(&self, table: &str) -> &[String] {
        self.columns.get(table).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

    use crate::core::domain::connection::Connection;
    use crate::core::domain::error::DomainError;
    use crate::core::domain::schema_column::SchemaColumn;
    use crate::core::domain::schema_table::SchemaTable;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use crate::core::ports::schema_column_repository::SchemaColumnRepository;
    use crate::core::ports::schema_table_repository::SchemaTableRepository;
    use crate::core::services::schema_cache::service::SchemaCacheService;
    use crate::core::services::schema_column::service::SchemaColumnService;
    use crate::core::services::schema_table::service::SchemaTableService;

    struct MockDb {
        columns: HashMap<String, Vec<String>>,
    }

    impl SchemaPort for MockDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            Ok(self.columns.clone())
        }
    }

    fn mock_db() -> MockDb {
        let mut columns = HashMap::new();
        columns.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        columns.insert("orders".to_string(), vec!["id".to_string(), "user_id".to_string()]);
        MockDb { columns }
    }

    #[test]
    fn load_populates_tables_and_columns() {
        let db = mock_db();
        let mut schema = SchemaService::new(None);
        schema.load(&db, "any").unwrap();
        assert_eq!(schema.tables(), &["orders", "users"]);
        assert_eq!(schema.columns_for("users"), &["id", "email"]);
        assert_eq!(schema.columns_for("orders"), &["id", "user_id"]);
    }

    #[test]
    fn columns_for_unknown_table_returns_empty() {
        let db = mock_db();
        let mut schema = SchemaService::new(None);
        schema.load(&db, "any").unwrap();
        assert_eq!(schema.columns_for("nonexistent"), &[] as &[String]);
    }

    struct StubConnRepo;
    impl ConnectionRepository for StubConnRepo {
        fn add(&self, _: Connection) -> Result<(), DomainError> { Ok(()) }
        fn list(&self) -> Result<Vec<Connection>, DomainError> { Ok(vec![]) }
        fn delete(&self, _: &str) -> Result<(), DomainError> { Ok(()) }
        fn get_connection(&self, n: &str) -> Result<Connection, DomainError> {
            Err(DomainError::NotFound(n.to_string()))
        }
        fn find_row_id(&self, _: &str) -> Result<i64, DomainError> { Ok(1) }
        fn rename(&self, _: &str, _: &str) -> Result<(), DomainError> { Ok(()) }
        fn update(&self, _: Connection) -> Result<(), DomainError> { Ok(()) }
    }

    struct StubTableRepo;
    impl SchemaTableRepository for StubTableRepo {
        fn save(&self, _: &SchemaTable) -> Result<(), DomainError> { Ok(()) }
        fn list_by_connection(&self, _: i64) -> Vec<SchemaTable> { vec![] }
        fn delete_by_connection(&self, _: i64) -> Result<(), DomainError> { Ok(()) }
    }

    struct StubColumnRepo {
        data: RwLock<Vec<SchemaColumn>>,
    }

    impl StubColumnRepo {
        fn empty() -> Arc<Self> { Arc::new(Self { data: RwLock::new(vec![]) }) }
        fn with_columns(cols: Vec<SchemaColumn>) -> Arc<Self> {
            Arc::new(Self { data: RwLock::new(cols) })
        }
    }

    impl SchemaColumnRepository for StubColumnRepo {
        fn save(&self, entity: &SchemaColumn) -> Result<(), DomainError> {
            self.data.write().unwrap().push(entity.clone());
            Ok(())
        }
        fn list_by_connection(&self, _: i64) -> Vec<SchemaColumn> {
            self.data.read().unwrap().clone()
        }
        fn delete_by_connection(&self, _: i64) -> Result<(), DomainError> {
            self.data.write().unwrap().clear();
            Ok(())
        }
    }

    fn make_cache(col_repo: Arc<StubColumnRepo>) -> Arc<SchemaCacheService> {
        let conn_repo = Arc::new(StubConnRepo) as Arc<dyn ConnectionRepository>;
        let table_svc = Arc::new(SchemaTableService::new(
            Arc::clone(&conn_repo),
            Arc::new(StubTableRepo) as Arc<dyn SchemaTableRepository>,
        ));
        let column_svc = Arc::new(SchemaColumnService::new(
            conn_repo,
            col_repo as Arc<dyn SchemaColumnRepository>,
        ));
        Arc::new(SchemaCacheService::new(table_svc, column_svc))
    }

    #[test]
    fn load_uses_cache_when_available() {
        let col_repo = StubColumnRepo::with_columns(vec![
            SchemaColumn {
                connection_id: 1,
                table_name: "cached_table".to_string(),
                column_name: "id".to_string(),
                data_type: None,
                cached_at: 0,
            },
        ]);
        let db = mock_db();
        let mut schema = SchemaService::new(Some(make_cache(col_repo)));
        schema.load(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"cached_table".to_string()));
        assert!(!schema.tables().contains(&"users".to_string()));
    }

    #[test]
    fn load_falls_back_to_db_and_saves_when_cache_empty() {
        let col_repo = StubColumnRepo::empty();
        let mut schema = SchemaService::new(Some(make_cache(Arc::clone(&col_repo))));
        let db = mock_db();
        schema.load(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"users".to_string()));
        assert!(!col_repo.data.read().unwrap().is_empty(), "save should have written to repo");
    }

    #[test]
    fn load_none_cache_behaves_like_load_from_db() {
        let db = mock_db();
        let mut schema = SchemaService::new(None);
        schema.load(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"users".to_string()));
    }

    #[test]
    fn refresh_invalidates_cache_then_reloads() {
        let col_repo = StubColumnRepo::with_columns(vec![
            SchemaColumn {
                connection_id: 1,
                table_name: "old_table".to_string(),
                column_name: "id".to_string(),
                data_type: None,
                cached_at: 0,
            },
        ]);
        let db = mock_db();
        let mut schema = SchemaService::new(Some(make_cache(Arc::clone(&col_repo))));
        schema.load(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"old_table".to_string()), "load should use cache");

        schema.refresh(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"users".to_string()), "refresh should fetch from DB");
        assert!(!schema.tables().contains(&"old_table".to_string()));
    }
}
