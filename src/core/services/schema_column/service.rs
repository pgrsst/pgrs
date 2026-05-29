use std::sync::Arc;

use crate::core::domain::error::DomainError;
use crate::core::domain::schema_column::SchemaColumn;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::schema_column_repository::SchemaColumnRepository;

pub struct SchemaColumnCreateInput {
    pub connection_name: String,
    pub table_name: String,
    pub column_name: String,
    pub data_type: Option<String>,
    pub cached_at: i64,
}

pub trait SchemaColumnSvc: Send + Sync {
    fn save(&self, input: SchemaColumnCreateInput) -> Result<(), DomainError>;
    fn list_by_connection(&self, connection_name: &str) -> Result<Vec<SchemaColumn>, DomainError>;
    fn delete_by_connection(&self, connection_name: &str) -> Result<(), DomainError>;
}

pub struct SchemaColumnService {
    connection_repo: Arc<dyn ConnectionRepository>,
    repository: Arc<dyn SchemaColumnRepository>,
}

impl SchemaColumnService {
    pub fn new(
        connection_repo: Arc<dyn ConnectionRepository>,
        repository: Arc<dyn SchemaColumnRepository>,
    ) -> Self {
        Self { connection_repo, repository }
    }
}

impl SchemaColumnSvc for SchemaColumnService {
    fn save(&self, input: SchemaColumnCreateInput) -> Result<(), DomainError> {
        let connection_id = self.connection_repo
            .get_connection(&input.connection_name)?
            .id
            .ok_or_else(|| DomainError::StorageError("connection has no id".to_string()))?;
        let entity = SchemaColumn {
            connection_id,
            table_name: input.table_name,
            column_name: input.column_name,
            data_type: input.data_type,
            cached_at: input.cached_at,
        };
        self.repository.save(&entity)
    }

    fn list_by_connection(&self, connection_name: &str) -> Result<Vec<SchemaColumn>, DomainError> {
        let connection_id = self.connection_repo
            .get_connection(connection_name)?
            .id
            .ok_or_else(|| DomainError::StorageError("connection has no id".to_string()))?;
        Ok(self.repository.list_by_connection(connection_id))
    }

    fn delete_by_connection(&self, connection_name: &str) -> Result<(), DomainError> {
        let connection_id = self.connection_repo
            .get_connection(connection_name)?
            .id
            .ok_or_else(|| DomainError::StorageError("connection has no id".to_string()))?;
        self.repository.delete_by_connection(connection_id)
    }
}
