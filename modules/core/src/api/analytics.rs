use std::sync::Arc;

use crate::adapters::driven::sqlite::SqliteRepository;
use crate::domain::query_history::QueryHistory;
use crate::ports::column_access_repository::ColumnAccessRepository;
use crate::ports::connection_repository::ConnectionRepository;
use crate::ports::query_history_repository::QueryHistoryRepository;
use crate::ports::table_access_repository::TableAccessRepository;
use crate::query::alias::{extract_column_refs, extract_referenced_tables};
use crate::services::analytics::service::{AnalyticsService, AnalyticsSvc};
use crate::services::column_access::service::{ColumnAccessService, ColumnAccessSvc};
use crate::services::query_history::service::{QueryHistoryService, QueryHistorySvc};
use crate::services::table_access::service::{TableAccessService, TableAccessSvc};

use super::schema::SchemaApi;

/// Public facade for usage analytics: records executed queries and exposes
/// access-frequency stats used to rank completions.
pub struct AnalyticsApi {
    svc: Arc<dyn AnalyticsSvc>,
}

impl AnalyticsApi {
    pub(crate) fn from_sqlite(sqlite: &Arc<SqliteRepository>) -> Self {
        let conn_repo = Arc::clone(sqlite) as Arc<dyn ConnectionRepository>;
        let query_history = Arc::new(QueryHistoryService::new(
            Arc::clone(&conn_repo),
            Arc::clone(sqlite) as Arc<dyn QueryHistoryRepository>,
        ));
        let table_access = Arc::new(TableAccessService::new(
            Arc::clone(&conn_repo),
            Arc::clone(sqlite) as Arc<dyn TableAccessRepository>,
        ));
        let column_access = Arc::new(ColumnAccessService::new(
            Arc::clone(&conn_repo),
            Arc::clone(sqlite) as Arc<dyn ColumnAccessRepository>,
        ));
        let svc = Arc::new(AnalyticsService::new(
            query_history as Arc<dyn QueryHistorySvc>,
            table_access as Arc<dyn TableAccessSvc>,
            column_access as Arc<dyn ColumnAccessSvc>,
        ));
        Self { svc }
    }

    /// Record an executed query plus the tables/columns it referenced.
    /// Table and column references are extracted from `sql` using `schema`.
    pub fn record_query(&self, connection_name: &str, sql: &str, schema: &SchemaApi) {
        let tables = extract_referenced_tables(sql);
        let schema_view: Vec<(&str, &[String])> = schema
            .tables()
            .iter()
            .map(|t| (t.as_str(), schema.columns_for(t)))
            .collect();
        let columns = extract_column_refs(sql, &schema_view);
        self.svc.record_query(connection_name, sql, &tables, &columns);
    }

    /// Recent query history for a connection (most recent first).
    pub fn history(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.svc.get_history(connection_name)
    }

    pub fn frequent_tables(&self, connection_name: &str) -> Vec<(String, u64)> {
        self.svc
            .get_frequent_tables(connection_name)
            .into_iter()
            .map(|e| (e.name, e.count))
            .collect()
    }

    pub fn frequent_columns(&self, connection_name: &str, table: &str) -> Vec<(String, u64)> {
        self.svc
            .get_frequent_columns(connection_name, table)
            .into_iter()
            .map(|e| (e.name, e.count))
            .collect()
    }
}
