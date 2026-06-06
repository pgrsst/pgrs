use std::collections::HashMap;
use std::sync::Arc;

use crate::services::schema_column::service::{SchemaColumnCreateInput, SchemaColumnSvc};
use crate::services::schema_table::service::{SchemaTableCreateInput, SchemaTableSvc};

pub trait SchemaCacheSvc: Send + Sync {
    fn save(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>);
    fn load(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>>;
    fn invalidate(&self, connection_name: &str);
}

pub struct SchemaCacheService {
    table_svc: Arc<dyn SchemaTableSvc>,
    column_svc: Arc<dyn SchemaColumnSvc>,
}

impl SchemaCacheService {
    pub fn new(table_svc: Arc<dyn SchemaTableSvc>, column_svc: Arc<dyn SchemaColumnSvc>) -> Self {
        Self { table_svc, column_svc }
    }
}

impl SchemaCacheSvc for SchemaCacheService {
    fn save(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>) {
        let now = crate::utils::unix_now();

        if let Err(e) = self.table_svc.delete_by_connection(connection_name) {
            eprintln!("pgrs: schema cache write failed: {e}");
            return;
        }
        if let Err(e) = self.column_svc.delete_by_connection(connection_name) {
            eprintln!("pgrs: schema cache write failed: {e}");
            return;
        }

        for (table_name, columns) in schema {
            if let Err(e) = self.table_svc.save(SchemaTableCreateInput {
                connection_name: connection_name.to_string(),
                table_name: table_name.clone(),
                cached_at: now,
            }) {
                eprintln!("pgrs: schema cache write failed: {e}");
                return;
            }
            for column_name in columns {
                if let Err(e) = self.column_svc.save(SchemaColumnCreateInput {
                    connection_name: connection_name.to_string(),
                    table_name: table_name.clone(),
                    column_name: column_name.clone(),
                    data_type: None,
                    cached_at: now,
                }) {
                    eprintln!("pgrs: schema cache write failed: {e}");
                    return;
                }
            }
        }
    }

    fn load(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>> {
        let columns = self.column_svc.list_by_connection(connection_name).ok()?;
        if columns.is_empty() {
            return None;
        }
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for col in columns {
            map.entry(col.table_name).or_default().push(col.column_name);
        }
        Some(map)
    }

    fn invalidate(&self, connection_name: &str) {
        if let Err(e) = self.table_svc.delete_by_connection(connection_name) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
        if let Err(e) = self.column_svc.delete_by_connection(connection_name) {
            eprintln!("pgrs: schema cache invalidate failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

    use crate::domain::error::DomainError;
    use crate::domain::schema_column::SchemaColumn;
    use crate::services::schema_column::service::SchemaColumnCreateInput;
    use crate::services::schema_table::service::SchemaTableCreateInput;

    struct StubTableSvc;
    impl SchemaTableSvc for StubTableSvc {
        fn save(&self, _: SchemaTableCreateInput) -> Result<(), DomainError> { Ok(()) }
        fn delete_by_connection(&self, _: &str) -> Result<(), DomainError> { Ok(()) }
    }

    struct StubColumnSvc {
        data: RwLock<Vec<SchemaColumn>>,
    }
    impl StubColumnSvc {
        fn empty() -> Self { Self { data: RwLock::new(vec![]) } }
        fn with_columns(cols: Vec<SchemaColumn>) -> Self { Self { data: RwLock::new(cols) } }
    }
    impl SchemaColumnSvc for StubColumnSvc {
        fn save(&self, input: SchemaColumnCreateInput) -> Result<(), DomainError> {
            self.data.write().unwrap().push(SchemaColumn {
                connection_id: 1,
                table_name: input.table_name,
                column_name: input.column_name,
                data_type: input.data_type,
                cached_at: input.cached_at,
            });
            Ok(())
        }
        fn list_by_connection(&self, _: &str) -> Result<Vec<SchemaColumn>, DomainError> {
            Ok(self.data.read().unwrap().clone())
        }
        fn delete_by_connection(&self, _: &str) -> Result<(), DomainError> {
            self.data.write().unwrap().clear();
            Ok(())
        }
    }

    fn make_svc(col_svc: Arc<StubColumnSvc>) -> SchemaCacheService {
        SchemaCacheService::new(
            Arc::new(StubTableSvc) as Arc<dyn SchemaTableSvc>,
            col_svc as Arc<dyn SchemaColumnSvc>,
        )
    }

    #[test]
    fn load_returns_none_when_empty() {
        let svc = make_svc(Arc::new(StubColumnSvc::empty()));
        assert!(svc.load("mydb").is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let col_svc = Arc::new(StubColumnSvc::empty());
        let svc = make_svc(Arc::clone(&col_svc));

        let mut schema = HashMap::new();
        schema.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        svc.save("mydb", &schema);

        let loaded = svc.load("mydb").unwrap();
        assert!(loaded.contains_key("users"));
        assert_eq!(loaded["users"].len(), 2);
    }

    #[test]
    fn invalidate_clears_cache() {
        let col_svc = Arc::new(StubColumnSvc::with_columns(vec![SchemaColumn {
            connection_id: 1,
            table_name: "users".to_string(),
            column_name: "id".to_string(),
            data_type: None,
            cached_at: 0,
        }]));
        let svc = make_svc(col_svc);
        assert!(svc.load("mydb").is_some());
        svc.invalidate("mydb");
        assert!(svc.load("mydb").is_none());
    }

    #[test]
    fn save_overwrites_existing() {
        let col_svc = Arc::new(StubColumnSvc::empty());
        let svc = make_svc(Arc::clone(&col_svc));

        let mut v1 = HashMap::new();
        v1.insert("users".to_string(), vec!["id".to_string()]);
        svc.save("mydb", &v1);

        let mut v2 = HashMap::new();
        v2.insert("orders".to_string(), vec!["id".to_string()]);
        svc.save("mydb", &v2);

        let loaded = svc.load("mydb").unwrap();
        assert!(!loaded.contains_key("users"), "old data should be replaced");
        assert!(loaded.contains_key("orders"));
    }
}
