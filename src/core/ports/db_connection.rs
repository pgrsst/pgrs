use std::collections::HashMap;

pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub rows_affected: Option<u64>,
}

pub trait DbConnection {
    fn execute(&self, query: &str) -> Result<QueryResult, String>;
    fn list_tables(&self) -> Result<Vec<String>, String>;
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDb;

    impl DbConnection for MockDb {
        fn execute(&self, _query: &str) -> Result<QueryResult, String> {
            Ok(QueryResult {
                columns: vec!["id".to_string()],
                rows: vec![vec!["1".to_string()]],
                rows_affected: None,
            })
        }

        fn list_tables(&self) -> Result<Vec<String>, String> {
            Ok(vec!["users".to_string()])
        }

        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            let mut m = HashMap::new();
            m.insert("users".to_string(), vec!["id".to_string(), "email".to_string()]);
            Ok(m)
        }
    }

    #[test]
    fn mock_db_implements_trait() {
        let db = MockDb;
        let tables = db.list_tables().unwrap();
        assert_eq!(tables, vec!["users"]);

        let cols = db.list_columns().unwrap();
        assert_eq!(cols["users"], vec!["id", "email"]);

        let result = db.execute("SELECT 1").unwrap();
        assert_eq!(result.columns, vec!["id"]);
        assert_eq!(result.rows[0][0], "1");
    }
}
