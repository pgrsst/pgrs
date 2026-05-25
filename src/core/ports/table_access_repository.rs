use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::error::DomainError;

pub trait TableAccessRepository: Send + Sync {
    fn insert(
        &self,
        connection_name: &str,
        table_name: &str,
        query_id: Option<i64>,
        accessed_at: i64,
    ) -> Result<(), DomainError>;
    fn list_frequent(&self, connection_name: &str, limit: usize) -> Vec<FreqEntry>;
}
