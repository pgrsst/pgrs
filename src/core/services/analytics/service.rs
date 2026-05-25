use std::sync::Arc;

use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::query_history::QueryHistory;
use crate::core::ports::column_access_repository::ColumnAccessRepository;
use crate::core::ports::query_history_repository::QueryHistoryRepository;
use crate::core::ports::table_access_repository::TableAccessRepository;

pub struct AnalyticsService {
    history: Arc<dyn QueryHistoryRepository>,
    table_access: Arc<dyn TableAccessRepository>,
    column_access: Arc<dyn ColumnAccessRepository>,
}

impl AnalyticsService {
    pub fn new(
        history: Arc<dyn QueryHistoryRepository>,
        table_access: Arc<dyn TableAccessRepository>,
        column_access: Arc<dyn ColumnAccessRepository>,
    ) -> Self {
        Self { history, table_access, column_access }
    }

    pub fn record_query(
        &self,
        connection_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        match self.history.upsert(connection_name, query, now) {
            Ok(query_id) => {
                for table in tables {
                    if let Err(e) = self.table_access.insert(connection_name, table, Some(query_id), now) {
                        eprintln!("pgrs: analytics write failed: {e:?}");
                    }
                }
                for (table, col) in columns {
                    if let Err(e) = self.column_access.insert(connection_name, table, col, Some(query_id), now) {
                        eprintln!("pgrs: analytics write failed: {e:?}");
                    }
                }
            }
            Err(e) => eprintln!("pgrs: analytics write failed: {e:?}"),
        }
    }

    pub fn get_history(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.history.list_recent(connection_name, 50)
    }

    pub fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry> {
        self.table_access.list_frequent(connection_name, 100)
    }

    pub fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        self.column_access.list_frequent_by_table(connection_name, table, 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::error::DomainError;
    use crate::core::domain::query_history::QueryHistory;
    use std::sync::Mutex;

    struct StubHistory {
        upserted: Mutex<Vec<(String, String)>>,
        entries: Vec<QueryHistory>,
    }
    impl StubHistory {
        fn new(entries: Vec<QueryHistory>) -> Self {
            Self { upserted: Mutex::new(vec![]), entries }
        }
    }
    impl QueryHistoryRepository for StubHistory {
        fn upsert(&self, connection_name: &str, query: &str, _: i64) -> Result<i64, DomainError> {
            self.upserted.lock().unwrap().push((connection_name.to_string(), query.to_string()));
            Ok(1)
        }
        fn list_recent(&self, _: &str, _: usize) -> Vec<QueryHistory> {
            self.entries.clone()
        }
    }

    struct StubTableAccess {
        inserted: Mutex<Vec<String>>,
        freq: Vec<FreqEntry>,
    }
    impl StubTableAccess {
        fn new(freq: Vec<FreqEntry>) -> Self {
            Self { inserted: Mutex::new(vec![]), freq }
        }
    }
    impl TableAccessRepository for StubTableAccess {
        fn insert(&self, _: &str, table_name: &str, _: Option<i64>, _: i64) -> Result<(), DomainError> {
            self.inserted.lock().unwrap().push(table_name.to_string());
            Ok(())
        }
        fn list_frequent(&self, _: &str, _: usize) -> Vec<FreqEntry> {
            self.freq.clone()
        }
    }

    struct StubColumnAccess {
        inserted: Mutex<Vec<String>>,
        freq: Vec<FreqEntry>,
    }
    impl StubColumnAccess {
        fn new(freq: Vec<FreqEntry>) -> Self {
            Self { inserted: Mutex::new(vec![]), freq }
        }
    }
    impl ColumnAccessRepository for StubColumnAccess {
        fn insert(&self, _: &str, _: &str, col: &str, _: Option<i64>, _: i64) -> Result<(), DomainError> {
            self.inserted.lock().unwrap().push(col.to_string());
            Ok(())
        }
        fn list_frequent_by_table(&self, _: &str, _: &str, _: usize) -> Vec<FreqEntry> {
            self.freq.clone()
        }
    }

    fn make_svc(
        history: Vec<QueryHistory>,
        table_freq: Vec<FreqEntry>,
        col_freq: Vec<FreqEntry>,
    ) -> (Arc<StubTableAccess>, Arc<StubColumnAccess>, AnalyticsService) {
        let h = Arc::new(StubHistory::new(history));
        let t = Arc::new(StubTableAccess::new(table_freq));
        let c = Arc::new(StubColumnAccess::new(col_freq));
        let svc = AnalyticsService::new(
            Arc::clone(&h) as Arc<dyn QueryHistoryRepository>,
            Arc::clone(&t) as Arc<dyn TableAccessRepository>,
            Arc::clone(&c) as Arc<dyn ColumnAccessRepository>,
        );
        (t, c, svc)
    }

    #[test]
    fn record_query_writes_to_all_repos() {
        let (t, c, svc) = make_svc(vec![], vec![], vec![]);
        svc.record_query("mydb", "SELECT 1", &["users".to_string()], &[("users".to_string(), "id".to_string())]);
        assert_eq!(t.inserted.lock().unwrap().as_slice(), &["users"]);
        assert_eq!(c.inserted.lock().unwrap().as_slice(), &["id"]);
    }

    #[test]
    fn get_history_delegates_to_repo() {
        let entry = QueryHistory { id: 1, connection_id: 1, query: "SELECT 1".to_string(), executed_at: 0 };
        let (_, _, svc) = make_svc(vec![entry], vec![], vec![]);
        let history = svc.get_history("mydb");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].query, "SELECT 1");
    }

    #[test]
    fn get_frequent_tables_delegates_to_repo() {
        let freq = vec![FreqEntry { name: "users".to_string(), count: 5 }];
        let (_, _, svc) = make_svc(vec![], freq, vec![]);
        let result = svc.get_frequent_tables("mydb");
        assert_eq!(result[0].name, "users");
        assert_eq!(result[0].count, 5);
    }

    #[test]
    fn get_frequent_columns_delegates_to_repo() {
        let freq = vec![FreqEntry { name: "email".to_string(), count: 3 }];
        let (_, _, svc) = make_svc(vec![], vec![], freq);
        let result = svc.get_frequent_columns("mydb", "users");
        assert_eq!(result[0].name, "email");
    }
}
