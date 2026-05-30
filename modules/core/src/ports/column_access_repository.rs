use crate::domain::analytics::FreqEntry;
use crate::domain::column_access::ColumnAccess;
use crate::domain::error::DomainError;

pub trait ColumnAccessRepository: Send + Sync {
    fn save(&self, entity: &ColumnAccess) -> Result<(), DomainError>;
    fn list_frequent_by_table(
        &self,
        connection_name: &str,
        table_name: &str,
        limit: usize,
    ) -> Vec<FreqEntry>;
}
