use std::sync::Arc;

use crate::domain::analytics::FreqEntry;
use crate::domain::query_history::QueryHistory;
use crate::services::column_access::service::{ColumnAccessCreateInput, ColumnAccessSvc};
use crate::services::query_history::service::{QueryHistoryCreateInput, QueryHistorySvc};
use crate::services::table_access::service::{TableAccessCreateInput, TableAccessSvc};

pub trait AnalyticsSvc: Send + Sync {
    fn record_query(&self, connection_name: &str, query: &str, tables: &[String], columns: &[(String, String)]);
    fn get_history(&self, connection_name: &str) -> Vec<QueryHistory>;
    fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry>;
    fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry>;
}

pub struct AnalyticsService {
    history: Arc<dyn QueryHistorySvc>,
    table_access: Arc<dyn TableAccessSvc>,
    column_access: Arc<dyn ColumnAccessSvc>,
}

impl AnalyticsService {
    pub fn new(
        history: Arc<dyn QueryHistorySvc>,
        table_access: Arc<dyn TableAccessSvc>,
        column_access: Arc<dyn ColumnAccessSvc>,
    ) -> Self {
        Self {
            history,
            table_access,
            column_access,
        }
    }
}

impl AnalyticsSvc for AnalyticsService {
    fn record_query(
        &self,
        connection_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    ) {
        let input = QueryHistoryCreateInput {
            connection_name: connection_name.to_string(),
            query: query.to_string(),
        };
        match self.history.record(input) {
            Ok(query_id) => {
                for table in tables {
                    let input = TableAccessCreateInput {
                        connection_name: connection_name.to_string(),
                        table_name: table.clone(),
                        query_id: Some(query_id),
                    };
                    if let Err(e) = self.table_access.record(input) {
                        eprintln!("pgrs: analytics write failed: {e:?}");
                    }
                }
                for (table, col) in columns {
                    let input = ColumnAccessCreateInput {
                        connection_name: connection_name.to_string(),
                        table_name: table.clone(),
                        column_name: col.clone(),
                        query_id: Some(query_id),
                    };
                    if let Err(e) = self.column_access.record(input) {
                        eprintln!("pgrs: analytics write failed: {e:?}");
                    }
                }
            }
            Err(e) => eprintln!("pgrs: analytics write failed: {e:?}"),
        }
    }

    fn get_history(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.history.list_recent(connection_name)
    }

    fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry> {
        self.table_access.get_frequent(connection_name)
    }

    fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        self.column_access.get_frequent_by_table(connection_name, table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analytics::FreqEntry;
    use crate::domain::error::DomainError;
    use crate::domain::query_history::QueryHistory;
    use crate::services::column_access::service::ColumnAccessCreateInput;
    use crate::services::query_history::service::QueryHistoryCreateInput;
    use crate::services::table_access::service::TableAccessCreateInput;
    use std::sync::Mutex;

    struct StubHistory {
        recorded: Mutex<Vec<String>>,
        entries: Vec<QueryHistory>,
    }
    impl StubHistory {
        fn new(entries: Vec<QueryHistory>) -> Self {
            Self { recorded: Mutex::new(vec![]), entries }
        }
    }
    impl QueryHistorySvc for StubHistory {
        fn record(&self, input: QueryHistoryCreateInput) -> Result<i64, DomainError> {
            self.recorded.lock().unwrap().push(input.query);
            Ok(1)
        }
        fn list_recent(&self, _: &str) -> Vec<QueryHistory> {
            self.entries.clone()
        }
    }

    struct StubTableAccess {
        recorded: Mutex<Vec<String>>,
        freq: Vec<FreqEntry>,
    }
    impl StubTableAccess {
        fn new(freq: Vec<FreqEntry>) -> Self {
            Self { recorded: Mutex::new(vec![]), freq }
        }
    }
    impl TableAccessSvc for StubTableAccess {
        fn record(&self, input: TableAccessCreateInput) -> Result<(), DomainError> {
            self.recorded.lock().unwrap().push(input.table_name);
            Ok(())
        }
        fn get_frequent(&self, _: &str) -> Vec<FreqEntry> {
            self.freq.clone()
        }
    }

    struct StubColumnAccess {
        recorded: Mutex<Vec<String>>,
        freq: Vec<FreqEntry>,
    }
    impl StubColumnAccess {
        fn new(freq: Vec<FreqEntry>) -> Self {
            Self { recorded: Mutex::new(vec![]), freq }
        }
    }
    impl ColumnAccessSvc for StubColumnAccess {
        fn record(&self, input: ColumnAccessCreateInput) -> Result<(), DomainError> {
            self.recorded.lock().unwrap().push(input.column_name);
            Ok(())
        }
        fn get_frequent_by_table(&self, _: &str, _: &str) -> Vec<FreqEntry> {
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
            Arc::clone(&h) as Arc<dyn QueryHistorySvc>,
            Arc::clone(&t) as Arc<dyn TableAccessSvc>,
            Arc::clone(&c) as Arc<dyn ColumnAccessSvc>,
        );
        (t, c, svc)
    }

    #[test]
    fn record_query_writes_to_all_repos() {
        let (t, c, svc) = make_svc(vec![], vec![], vec![]);
        svc.record_query(
            "mydb",
            "SELECT 1",
            &["users".to_string()],
            &[("users".to_string(), "id".to_string())],
        );
        assert_eq!(t.recorded.lock().unwrap().as_slice(), &["users"]);
        assert_eq!(c.recorded.lock().unwrap().as_slice(), &["id"]);
    }

    #[test]
    fn get_history_delegates_to_repo() {
        let entry = QueryHistory {
            id: 1,
            connection_id: 1,
            query: "SELECT 1".to_string(),
            executed_at: 0,
        };
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
