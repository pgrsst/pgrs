use std::sync::Arc;

use crate::core::domain::error::DomainError;
use crate::core::domain::schema_table::SchemaTable;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::schema_table_repository::SchemaTableRepository;

pub struct SchemaTableCreateInput {
    pub connection_name: String,
    pub table_name: String,
    pub cached_at: i64,
}

pub struct SchemaTableService {
    connection_repo: Arc<dyn ConnectionRepository>,
    repository: Arc<dyn SchemaTableRepository>,
}

impl SchemaTableService {
    pub fn new(
        connection_repo: Arc<dyn ConnectionRepository>,
        repository: Arc<dyn SchemaTableRepository>,
    ) -> Self {
        Self { connection_repo, repository }
    }

    pub fn save(&self, input: SchemaTableCreateInput) -> Result<(), DomainError> {
        let connection_id = self.connection_repo.find_row_id(&input.connection_name)?;
        let entity = SchemaTable {
            connection_id,
            table_name: input.table_name,
            cached_at: input.cached_at,
        };
        self.repository.save(&entity)
    }

    pub fn list_by_connection(&self, connection_name: &str) -> Result<Vec<SchemaTable>, DomainError> {
        let connection_id = self.connection_repo.find_row_id(connection_name)?;
        Ok(self.repository.list_by_connection(connection_id))
    }

    pub fn delete_by_connection(&self, connection_name: &str) -> Result<(), DomainError> {
        let connection_id = self.connection_repo.find_row_id(connection_name)?;
        self.repository.delete_by_connection(connection_id)
    }
}
