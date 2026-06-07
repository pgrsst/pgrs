use crate::domain::error::DomainError;
use crate::domain::schema_table::SchemaTable;
use crate::ports::schema_table_repository::SchemaTableRepository;
use super::SqliteRepository;

impl SchemaTableRepository for SqliteRepository {
    fn save(&self, entity: &SchemaTable) -> Result<(), DomainError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_tables (connection_id, table_name, cached_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![entity.connection_id, entity.table_name, entity.cached_at],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM schema_tables WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }
}
