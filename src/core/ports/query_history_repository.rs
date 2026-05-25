use crate::core::domain::error::DomainError;
use crate::core::domain::query_history::QueryHistory;

pub trait QueryHistoryRepository: Send + Sync {
    fn upsert(&self, connection_name: &str, query: &str, executed_at: i64) -> Result<i64, DomainError>;
    fn list_recent(&self, connection_name: &str, limit: usize) -> Vec<QueryHistory>;
}
