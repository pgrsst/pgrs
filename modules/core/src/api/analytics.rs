use std::sync::Arc;

use crate::domain::query_history::QueryHistory;
use crate::query::alias::{extract_column_refs, extract_referenced_tables};
use crate::services::analytics::service::AnalyticsSvc;

use super::schema::SchemaApi;

/// Public facade for usage analytics: records executed queries and exposes
/// access-frequency stats used to rank completions.
pub struct AnalyticsApi {
    svc: Arc<dyn AnalyticsSvc>,
}

impl AnalyticsApi {
    /// Wrap an assembled `AnalyticsSvc`. Service wiring lives in the
    /// composition root (`Core`); this facade stays a thin delegator.
    pub(crate) fn new(svc: Arc<dyn AnalyticsSvc>) -> Self {
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
