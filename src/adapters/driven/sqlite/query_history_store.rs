use crate::core::domain::error::DomainError;
use crate::core::domain::query_history::QueryHistory;
use crate::core::ports::query_history_repository::QueryHistoryRepository;
use super::SqliteRepository;

impl QueryHistoryRepository for SqliteRepository {
    fn upsert(&self, connection_name: &str, query: &str, executed_at: i64) -> Result<i64, DomainError> {
        let conn = self.conn.lock().unwrap();
        let connection_id = SqliteRepository::connection_id_for(&conn, connection_name)
            .ok_or_else(|| DomainError::NotFound(connection_name.to_string()))?;
        conn.execute(
            "INSERT INTO query_history (connection_id, query, executed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(connection_id, query) DO UPDATE SET executed_at = excluded.executed_at",
            rusqlite::params![connection_id, query, executed_at],
        )
        .map_err(|e| DomainError::StorageError(e.to_string()))?;
        conn.query_row(
            "SELECT id FROM query_history WHERE connection_id = ?1 AND query = ?2",
            rusqlite::params![connection_id, query],
            |r| r.get(0),
        )
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn list_recent(&self, connection_name: &str, limit: usize) -> Vec<QueryHistory> {
        let conn = self.conn.lock().unwrap();
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<QueryHistory>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT id, connection_id, query, executed_at FROM query_history
                 WHERE connection_id = ?1 ORDER BY executed_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id, limit as i64], |r| {
                Ok(QueryHistory {
                    id: r.get(0)?,
                    connection_id: r.get(1)?,
                    query: r.get(2)?,
                    executed_at: r.get(3)?,
                })
            })?;
            rows.collect()
        })();
        match result {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("pgrs: query_history read failed: {e}");
                vec![]
            }
        }
    }
}
