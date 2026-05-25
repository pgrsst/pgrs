use crate::core::domain::error::DomainError;
use crate::core::domain::schema_table::SchemaTable;
use crate::core::ports::schema_table_repository::SchemaTableRepository;
use super::SqliteRepository;

impl SchemaTableRepository for SqliteRepository {
    fn save(&self, entity: &SchemaTable) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO schema_tables (connection_id, table_name, cached_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![entity.connection_id, entity.table_name, entity.cached_at],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn list_by_connection(&self, connection_id: i64) -> Vec<SchemaTable> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<SchemaTable>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT connection_id, table_name, cached_at FROM schema_tables
                 WHERE connection_id = ?1 ORDER BY table_name",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(SchemaTable {
                    connection_id: r.get(0)?,
                    table_name: r.get(1)?,
                    cached_at: r.get(2)?,
                })
            })?;
            rows.collect()
        })();
        match result {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("pgrs: schema_table read failed: {e}");
                vec![]
            }
        }
    }

    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM schema_tables WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }
}
