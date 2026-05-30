use crate::domain::error::DomainError;
use crate::domain::schema_column::SchemaColumn;
use crate::ports::schema_column_repository::SchemaColumnRepository;
use super::SqliteRepository;

impl SchemaColumnRepository for SqliteRepository {
    fn save(&self, entity: &SchemaColumn) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO schema_columns
             (connection_id, table_name, column_name, data_type, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entity.connection_id, entity.table_name, entity.column_name, entity.data_type, entity.cached_at],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn list_by_connection(&self, connection_id: i64) -> Vec<SchemaColumn> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<SchemaColumn>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT connection_id, table_name, column_name, data_type, cached_at
                 FROM schema_columns WHERE connection_id = ?1 ORDER BY table_name, rowid",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(SchemaColumn {
                    connection_id: r.get(0)?,
                    table_name: r.get(1)?,
                    column_name: r.get(2)?,
                    data_type: r.get(3)?,
                    cached_at: r.get(4)?,
                })
            })?;
            rows.collect()
        })();
        match result {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("pgrs: schema_column read failed: {e}");
                vec![]
            }
        }
    }

    fn delete_by_connection(&self, connection_id: i64) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM schema_columns WHERE connection_id = ?1",
            rusqlite::params![connection_id],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }
}
