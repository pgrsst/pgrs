use crate::domain::error::DomainError;
use crate::domain::query_history::QueryHistory;
use crate::ports::query_history_repository::QueryHistoryRepository;
use super::SqliteRepository;

impl QueryHistoryRepository for SqliteRepository {
    fn save(&self, entity: &QueryHistory) -> Result<i64, DomainError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO query_history (connection_id, query, executed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(connection_id, query) DO UPDATE SET executed_at = excluded.executed_at",
            rusqlite::params![entity.connection_id, entity.query, entity.executed_at],
        )
        .map_err(|e| DomainError::StorageError(e.to_string()))?;
        conn.query_row(
            "SELECT id FROM query_history WHERE connection_id = ?1 AND query = ?2",
            rusqlite::params![entity.connection_id, entity.query],
            |r| r.get(0),
        )
        .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn list_recent(&self, connection_name: &str, limit: usize) -> Vec<QueryHistory> {
        let Ok(conn) = self.lock() else {
            eprintln!("pgrs: query_history read failed: database lock poisoned");
            return vec![];
        };
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
