use std::sync::Arc;

use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::query_history::QueryHistory;
use crate::core::services::column_access::service::{ColumnAccessCreateInput, ColumnAccessService};
use crate::core::services::query_history::service::{QueryHistoryCreateInput, QueryHistoryService};
use crate::core::services::table_access::service::{TableAccessCreateInput, TableAccessService};

pub struct AnalyticsService {
    history: Arc<QueryHistoryService>,
    table_access: Arc<TableAccessService>,
    column_access: Arc<ColumnAccessService>,
}

impl AnalyticsService {
    pub fn new(
        history: Arc<QueryHistoryService>,
        table_access: Arc<TableAccessService>,
        column_access: Arc<ColumnAccessService>,
    ) -> Self {
        Self {
            history,
            table_access,
            column_access,
        }
    }

    pub fn record_query(
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

    pub fn get_history(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.history.list_recent(connection_name)
    }

    pub fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry> {
        self.table_access.get_frequent(connection_name)
    }

    pub fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        self.column_access
            .get_frequent_by_table(connection_name, table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::analytics::FreqEntry;
    use crate::core::domain::column_access::ColumnAccess;
    use crate::core::domain::connection::Connection;
    use crate::core::domain::error::DomainError;
    use crate::core::domain::query_history::QueryHistory;
    use crate::core::domain::table_access::TableAccess;
    use crate::core::ports::column_access_repository::ColumnAccessRepository;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use crate::core::ports::query_history_repository::QueryHistoryRepository;
    use crate::core::ports::table_access_repository::TableAccessRepository;
    use std::sync::Mutex;

    struct StubConnRepo;
    impl ConnectionRepository for StubConnRepo {
        fn add(&self, _: Connection) -> Result<(), DomainError> {
            Ok(())
        }
        fn list(&self) -> Result<Vec<Connection>, DomainError> {
            Ok(vec![])
        }
        fn delete(&self, _: &str) -> Result<(), DomainError> {
            Ok(())
        }
        fn get_connection(&self, name: &str) -> Result<Connection, DomainError> {
            Err(DomainError::NotFound(name.to_string()))
        }
        fn find_row_id(&self, _: &str) -> Result<i64, DomainError> {
            Ok(1)
        }
        fn rename(&self, _: &str, _: &str) -> Result<(), DomainError> {
            Ok(())
        }
        fn update(&self, _: Connection) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct StubHistory {
        upserted: Mutex<Vec<String>>,
        entries: Vec<QueryHistory>,
    }
    impl StubHistory {
        fn new(entries: Vec<QueryHistory>) -> Self {
            Self {
                upserted: Mutex::new(vec![]),
                entries,
            }
        }
    }
    impl QueryHistoryRepository for StubHistory {
        fn save(&self, entity: &QueryHistory) -> Result<i64, DomainError> {
            self.upserted.lock().unwrap().push(entity.query.clone());
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
            Self {
                inserted: Mutex::new(vec![]),
                freq,
            }
        }
    }
    impl TableAccessRepository for StubTableAccess {
        fn save(&self, entity: &TableAccess) -> Result<(), DomainError> {
            self.inserted
                .lock()
                .unwrap()
                .push(entity.table_name.clone());
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
            Self {
                inserted: Mutex::new(vec![]),
                freq,
            }
        }
    }
    impl ColumnAccessRepository for StubColumnAccess {
        fn save(&self, entity: &ColumnAccess) -> Result<(), DomainError> {
            self.inserted
                .lock()
                .unwrap()
                .push(entity.column_name.clone());
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
    ) -> (
        Arc<StubTableAccess>,
        Arc<StubColumnAccess>,
        AnalyticsService,
    ) {
        let conn_repo = Arc::new(StubConnRepo);
        let h = Arc::new(StubHistory::new(history));
        let t = Arc::new(StubTableAccess::new(table_freq));
        let c = Arc::new(StubColumnAccess::new(col_freq));

        let history_svc = Arc::new(QueryHistoryService::new(
            Arc::clone(&conn_repo) as Arc<dyn ConnectionRepository>,
            Arc::clone(&h) as Arc<dyn QueryHistoryRepository>,
        ));
        let table_svc = Arc::new(TableAccessService::new(
            Arc::clone(&conn_repo) as Arc<dyn ConnectionRepository>,
            Arc::clone(&t) as Arc<dyn TableAccessRepository>,
        ));
        let col_svc = Arc::new(ColumnAccessService::new(
            Arc::clone(&conn_repo) as Arc<dyn ConnectionRepository>,
            Arc::clone(&c) as Arc<dyn ColumnAccessRepository>,
        ));

        let svc = AnalyticsService::new(history_svc, table_svc, col_svc);
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
        assert_eq!(t.inserted.lock().unwrap().as_slice(), &["users"]);
        assert_eq!(c.inserted.lock().unwrap().as_slice(), &["id"]);
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
        let freq = vec![FreqEntry {
            name: "users".to_string(),
            count: 5,
        }];
        let (_, _, svc) = make_svc(vec![], freq, vec![]);
        let result = svc.get_frequent_tables("mydb");
        assert_eq!(result[0].name, "users");
        assert_eq!(result[0].count, 5);
    }

    #[test]
    fn get_frequent_columns_delegates_to_repo() {
        let freq = vec![FreqEntry {
            name: "email".to_string(),
            count: 3,
        }];
        let (_, _, svc) = make_svc(vec![], vec![], freq);
        let result = svc.get_frequent_columns("mydb", "users");
        assert_eq!(result[0].name, "email");
    }
}
