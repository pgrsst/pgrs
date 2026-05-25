use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::error::DomainError;
use crate::core::ports::column_access_repository::ColumnAccessRepository;
use super::SqliteRepository;

impl ColumnAccessRepository for SqliteRepository {
    fn insert(
        &self,
        connection_name: &str,
        table_name: &str,
        column_name: &str,
        query_id: Option<i64>,
        accessed_at: i64,
    ) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let connection_id = SqliteRepository::connection_id_for(&conn, connection_name)
            .ok_or_else(|| DomainError::NotFound(connection_name.to_string()))?;
        conn.execute(
            "INSERT INTO column_access (connection_id, table_name, column_name, query_id, accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![connection_id, table_name, column_name, query_id, accessed_at],
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
