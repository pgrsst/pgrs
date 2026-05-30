use crate::domain::analytics::FreqEntry;
use crate::domain::column_access::ColumnAccess;
use crate::domain::error::DomainError;
use crate::ports::column_access_repository::ColumnAccessRepository;
use super::SqliteRepository;

impl ColumnAccessRepository for SqliteRepository {
    fn save(&self, entity: &ColumnAccess) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_access (connection_id, table_name, column_name, query_id, accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entity.connection_id, entity.table_name, entity.column_name, entity.query_id, entity.accessed_at],
        )
        .map(|_| ())
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn list_frequent_by_table(
        &self,
        connection_name: &str,
        table_name: &str,
        limit: usize,
    ) -> Vec<FreqEntry> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<FreqEntry>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT column_name, COUNT(*) as cnt FROM column_access
                 WHERE connection_id = ?1 AND table_name = ?2
                 GROUP BY column_name ORDER BY cnt DESC LIMIT ?3",
            )?;
            let rows = stmt.query_map(
                rusqlite::params![connection_id, table_name, limit as i64],
                |r| Ok(FreqEntry { name: r.get(0)?, count: r.get::<_, i64>(1)? as u64 }),
            )?;
            rows.collect()
        })();
        match result {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("pgrs: column_access read failed: {e}");
                vec![]
            }
        }
    }
}
