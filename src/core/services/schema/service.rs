use std::collections::HashMap;
use std::sync::Arc;

use crate::core::ports::schema_port::SchemaPort;
use crate::core::services::schema_cache::service::SchemaCacheSvc;

#[derive(Clone)]
pub struct SchemaService {
    cache: Option<Arc<dyn SchemaCacheSvc>>,
    tables: Vec<String>,
    columns: HashMap<String, Vec<String>>,
}

impl SchemaService {
    pub fn new(cache: Option<Arc<dyn SchemaCacheSvc>>) -> Self {
        Self {
            cache,
            tables: vec![],
            columns: HashMap::new(),
        }
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
        columns.insert(
            "users".to_string(),
            vec!["id".to_string(), "email".to_string()],
        );
        columns.insert(
            "orders".to_string(),
            vec!["id".to_string(), "user_id".to_string()],
        );
        MockDb { columns }
    }

    struct StubCache {
        store: RwLock<Option<HashMap<String, Vec<String>>>>,
        saved: RwLock<Vec<HashMap<String, Vec<String>>>>,
    }
    impl StubCache {
        fn empty() -> Arc<Self> {
            Arc::new(Self {
                store: RwLock::new(None),
                saved: RwLock::new(vec![]),
            })
        }
        fn with_data(data: HashMap<String, Vec<String>>) -> Arc<Self> {
            Arc::new(Self {
                store: RwLock::new(Some(data)),
                saved: RwLock::new(vec![]),
            })
        }
    }
    impl SchemaCacheSvc for StubCache {
        fn save(&self, _: &str, schema: &HashMap<String, Vec<String>>) {
            *self.store.write().unwrap() = Some(schema.clone());
            self.saved.write().unwrap().push(schema.clone());
        }
        fn load(&self, _: &str) -> Option<HashMap<String, Vec<String>>> {
            self.store.read().unwrap().clone()
        }
        fn invalidate(&self, _: &str) {
            *self.store.write().unwrap() = None;
        }
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

    #[test]
    fn load_uses_cache_when_available() {
        let mut cached = HashMap::new();
        cached.insert("cached_table".to_string(), vec!["id".to_string()]);
        let cache = StubCache::with_data(cached);
        let db = mock_db();
        let mut schema = SchemaService::new(Some(Arc::clone(&cache) as Arc<dyn SchemaCacheSvc>));
        schema.load(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"cached_table".to_string()));
        assert!(!schema.tables().contains(&"users".to_string()));
    }

    #[test]
    fn load_falls_back_to_db_and_saves_when_cache_empty() {
        let cache = StubCache::empty();
        let db = mock_db();
        let mut schema = SchemaService::new(Some(Arc::clone(&cache) as Arc<dyn SchemaCacheSvc>));
        schema.load(&db, "mydb").unwrap();
        assert!(schema.tables().contains(&"users".to_string()));
        assert!(
            !cache.saved.read().unwrap().is_empty(),
            "save should have been called"
        );
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
        let mut cached = HashMap::new();
        cached.insert("old_table".to_string(), vec!["id".to_string()]);
        let cache = StubCache::with_data(cached);
        let db = mock_db();
        let mut schema = SchemaService::new(Some(Arc::clone(&cache) as Arc<dyn SchemaCacheSvc>));
        schema.load(&db, "mydb").unwrap();
        assert!(
            schema.tables().contains(&"old_table".to_string()),
            "load should use cache"
        );

        schema.refresh(&db, "mydb").unwrap();
        assert!(
            schema.tables().contains(&"users".to_string()),
            "refresh should fetch from DB"
        );
        assert!(!schema.tables().contains(&"old_table".to_string()));
    }
}
