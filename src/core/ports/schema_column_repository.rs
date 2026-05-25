use crate::core::domain::error::DomainError;
use crate::core::domain::schema_column::SchemaColumn;

pub trait SchemaColumnRepository: Send + Sync {
    fn save(&self, entity: &SchemaColumn) -> Result<(), DomainError>;
    fn list_by_connection(&self, connection_id: i64) -> Vec<SchemaColumn>;
    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError>;
}
