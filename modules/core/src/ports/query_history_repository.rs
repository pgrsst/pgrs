use crate::domain::error::DomainError;
use crate::domain::query_history::QueryHistory;

pub trait QueryHistoryRepository: Send + Sync {
    fn save(&self, entity: &QueryHistory) -> Result<i64, DomainError>;
    fn list_recent(&self, connection_name: &str, limit: usize) -> Vec<QueryHistory>;
}
