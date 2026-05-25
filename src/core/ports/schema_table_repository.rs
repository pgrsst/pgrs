use crate::core::domain::error::DomainError;
use crate::core::domain::schema_table::SchemaTable;

pub trait SchemaTableRepository: Send + Sync {
    fn save(&self, entity: &SchemaTable) -> Result<(), DomainError>;
    fn list_by_connection(&self, connection_id: i64) -> Vec<SchemaTable>;
    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError>;
}
