use crate::core::domain::analytics::{FreqEntry, HistoryEntry};

pub trait AnalyticsPort: Send + Sync {
    fn record_query(
        &self,
        connection_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    );
    fn get_history(&self, connection_name: &str) -> Vec<HistoryEntry>;
    fn get_frequent_tables(&self, connection_name: &str) -> Vec<FreqEntry>;
    fn get_frequent_columns(&self, connection_name: &str, table: &str) -> Vec<FreqEntry>;
}
