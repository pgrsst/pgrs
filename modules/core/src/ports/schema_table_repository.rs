use crate::domain::error::DomainError;
use crate::domain::schema_table::SchemaTable;

pub trait SchemaTableRepository: Send + Sync {
    fn save(&self, entity: &SchemaTable) -> Result<(), DomainError>;
    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError>;
}
