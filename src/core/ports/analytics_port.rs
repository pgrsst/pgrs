use crate::core::domain::analytics::{FreqEntry, HistoryEntry};

pub trait AnalyticsPort: Send + Sync {
    fn record_query(
        &self,
        db_name: &str,
        query: &str,
        tables: &[String],
        columns: &[(String, String)],
    );
    fn get_history(&self, db_name: &str) -> Vec<HistoryEntry>;
    fn get_frequent_tables(&self, db_name: &str) -> Vec<FreqEntry>;
    fn get_frequent_columns(&self, db_name: &str, table: &str) -> Vec<FreqEntry>;
}
