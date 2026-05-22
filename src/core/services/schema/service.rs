use std::collections::HashMap;
use crate::core::ports::db_connection::DbConnection;

#[derive(Clone)]
pub struct SchemaService {
    pub tables: Vec<String>,
    pub columns: HashMap<String, Vec<String>>,
}

impl SchemaService {
    pub fn load(conn: &dyn DbConnection) -> Result<Self, String> {
        let tables = conn.list_tables()?;
        let columns = conn.list_columns()?;
        Ok(Self { tables, columns })
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
    use crate::core::ports::db_connection::QueryResult;

    struct MockDb {
        tables: Vec<String>,
        columns: HashMap<String, Vec<String>>,
    }

    impl DbConnection for MockDb {
        fn execute(&self, _: &str) -> Result<QueryResult, String> {
            Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: None })
        }

        fn list_tables(&self) -> Result<Vec<String>, String> {
            Ok(self.tables.clone())
        }

        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            Ok(self.columns.clone())
        }
    }

    fn mock_db() -> MockDb {
        let mut columns = HashMap::new();
        columns.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
        columns.insert("orders".to_string(), vec!["id".to_string(), "user_id".to_string()]);
        MockDb {
            tables: vec!["users".to_string(), "orders".to_string()],
            columns,
        }
    }

    #[test]
    fn load_populates_tables_and_columns() {
        let db = mock_db();
        let schema = SchemaService::load(&db).unwrap();
        assert_eq!(schema.tables(), &["users", "orders"]);
        assert_eq!(schema.columns_for("users"), &["id", "email"]);
        assert_eq!(schema.columns_for("orders"), &["id", "user_id"]);
    }

    #[test]
    fn columns_for_unknown_table_returns_empty() {
        let db = mock_db();
        let schema = SchemaService::load(&db).unwrap();
        assert_eq!(schema.columns_for("nonexistent"), &[] as &[String]);
    }
}
