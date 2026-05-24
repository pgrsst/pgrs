use std::collections::HashMap;
use crate::core::ports::schema_port::SchemaPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;

#[derive(Clone)]
pub struct SchemaService {
    tables: Vec<String>,
    columns: HashMap<String, Vec<String>>,
}

impl SchemaService {
    // Uses `dyn SchemaPort` (not a generic) because callers pass `Box<dyn ReplPort>`,
    // whose concrete type is already erased by the time it reaches this function.
    pub fn load(conn: &dyn SchemaPort) -> Result<Self, String> {
        let columns = conn.list_columns()?;
        let mut tables: Vec<String> = columns.keys().cloned().collect();
        tables.sort();
        Ok(Self { tables, columns })
    }

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
    use crate::core::ports::schema_port::SchemaPort;

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
        let schema = SchemaService::load(&db).unwrap();
        assert_eq!(schema.tables(), &["orders", "users"]);
        assert_eq!(schema.columns_for("users"), &["id", "email"]);
        assert_eq!(schema.columns_for("orders"), &["id", "user_id"]);
    }

    #[test]
    fn columns_for_unknown_table_returns_empty() {
        let db = mock_db();
        let schema = SchemaService::load(&db).unwrap();
        assert_eq!(schema.columns_for("nonexistent"), &[] as &[String]);
    }

    use crate::core::ports::schema_cache_port::SchemaCachePort;
    use std::sync::RwLock;

    struct MockCache {
        stored: RwLock<Option<HashMap<String, Vec<String>>>>,
    }

    impl MockCache {
        fn empty() -> Self {
            Self { stored: RwLock::new(None) }
        }
        fn with_data(schema: HashMap<String, Vec<String>>) -> Self {
            Self { stored: RwLock::new(Some(schema)) }
        }
    }

    impl SchemaCachePort for MockCache {
        fn save_schema(&self, _db: &str, schema: &HashMap<String, Vec<String>>) {
            *self.stored.write().unwrap() = Some(schema.clone());
        }
        fn load_schema(&self, _db: &str) -> Option<HashMap<String, Vec<String>>> {
            self.stored.read().unwrap().clone()
        }
        fn invalidate(&self, _db: &str) {
            *self.stored.write().unwrap() = None;
        }
    }

    #[test]
    fn load_with_cache_uses_cache_when_available() {
        let mut cached = HashMap::new();
        cached.insert("cached_table".to_string(), vec!["id".to_string()]);

        let db = mock_db();
        let cache = MockCache::with_data(cached);

        let schema = SchemaService::load_with_cache(&db, "mydb", Some(&cache)).unwrap();
        assert!(schema.tables().contains(&"cached_table".to_string()));
        assert!(!schema.tables().contains(&"users".to_string()));
    }

    #[test]
    fn load_with_cache_falls_back_to_db_and_saves_when_cache_empty() {
        let db = mock_db();
        let cache = MockCache::empty();

        let schema = SchemaService::load_with_cache(&db, "mydb", Some(&cache)).unwrap();
        assert!(schema.tables().contains(&"users".to_string()));
        assert!(cache.stored.read().unwrap().is_some());
    }

    #[test]
    fn load_with_cache_none_behaves_like_load() {
        let db = mock_db();
        let schema = SchemaService::load_with_cache(&db, "mydb", None).unwrap();
        assert!(schema.tables().contains(&"users".to_string()));
    }
}
