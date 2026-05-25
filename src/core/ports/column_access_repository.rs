use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::column_access::ColumnAccess;
use crate::core::domain::error::DomainError;

pub trait ColumnAccessRepository: Send + Sync {
    fn save(&self, entity: &ColumnAccess) -> Result<(), DomainError>;
    fn list_frequent_by_table(
        &self,
        connection_name: &str,
        table_name: &str,
        limit: usize,
    ) -> Vec<FreqEntry>;
}
