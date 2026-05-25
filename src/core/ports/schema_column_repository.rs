use crate::core::domain::error::DomainError;
use crate::core::domain::schema_column::SchemaColumn;

pub trait SchemaColumnRepository: Send + Sync {
    fn save(
        &self,
        connection_id: i64,
        table_name: &str,
        column_name: &str,
        data_type: Option<&str>,
        cached_at: i64,
    ) -> Result<(), DomainError>;
    fn list_by_connection(&self, connection_id: i64) -> Vec<SchemaColumn>;
    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError>;
}
