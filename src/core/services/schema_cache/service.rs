use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::services::schema_column::service::{SchemaColumnCreateInput, SchemaColumnService};
use crate::core::services::schema_table::service::{SchemaTableCreateInput, SchemaTableService};

pub struct SchemaCacheService {
    table_svc: Arc<SchemaTableService>,
    column_svc: Arc<SchemaColumnService>,
}

impl SchemaCacheService {
    pub fn new(table_svc: Arc<SchemaTableService>, column_svc: Arc<SchemaColumnService>) -> Self {
        Self { table_svc, column_svc }
    }

    pub fn save(&self, connection_name: &str, schema: &HashMap<String, Vec<String>>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

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

    pub fn load(&self, connection_name: &str) -> Option<HashMap<String, Vec<String>>> {
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

    pub fn invalidate(&self, connection_name: &str) {
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

    use crate::core::domain::connection::Connection;
    use crate::core::domain::error::DomainError;
    use crate::core::domain::schema_column::SchemaColumn;
    use crate::core::domain::schema_table::SchemaTable;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use crate::core::ports::schema_column_repository::SchemaColumnRepository;
    use crate::core::ports::schema_table_repository::SchemaTableRepository;
    use crate::core::services::schema_column::service::SchemaColumnService;
    use crate::core::services::schema_table::service::SchemaTableService;

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
        fn delete_by_connection(&self, _: i64) -> Result<(), DomainError> { Ok(()) }
    }

    struct StubColumnRepo {
        data: RwLock<Vec<SchemaColumn>>,
    }

    impl StubColumnRepo {
        fn empty() -> Self {
            Self { data: RwLock::new(vec![]) }
        }
        fn with_columns(cols: Vec<SchemaColumn>) -> Self {
            Self { data: RwLock::new(cols) }
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

    fn make_svc(col_repo: Arc<StubColumnRepo>) -> SchemaCacheService {
        let conn_repo = Arc::new(StubConnRepo) as Arc<dyn ConnectionRepository>;
        let table_svc = Arc::new(SchemaTableService::new(
            Arc::clone(&conn_repo),
            Arc::new(StubTableRepo) as Arc<dyn SchemaTableRepository>,
        ));
        let column_svc = Arc::new(SchemaColumnService::new(
            conn_repo,
            col_repo as Arc<dyn SchemaColumnRepository>,
        ));
        SchemaCacheService::new(table_svc, column_svc)
    }

    #[test]
    fn load_returns_none_when_empty() {
        let svc = make_svc(Arc::new(StubColumnRepo::empty()));
        assert!(svc.load("mydb").is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let col_repo = Arc::new(StubColumnRepo::empty());
        let svc = make_svc(Arc::clone(&col_repo));

        let mut schema = HashMap::new();
        schema.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        svc.save("mydb", &schema);

        let loaded = svc.load("mydb").unwrap();
        assert!(loaded.contains_key("users"));
        assert_eq!(loaded["users"].len(), 2);
    }

    #[test]
    fn invalidate_clears_cache() {
        let col_repo = Arc::new(StubColumnRepo::with_columns(vec![
            SchemaColumn {
                connection_id: 1,
                table_name: "users".to_string(),
                column_name: "id".to_string(),
                data_type: None,
                cached_at: 0,
            },
        ]));
        let svc = make_svc(col_repo);
        assert!(svc.load("mydb").is_some());
        svc.invalidate("mydb");
        assert!(svc.load("mydb").is_none());
    }

    #[test]
    fn save_overwrites_existing() {
        let col_repo = Arc::new(StubColumnRepo::empty());
        let svc = make_svc(Arc::clone(&col_repo));

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
