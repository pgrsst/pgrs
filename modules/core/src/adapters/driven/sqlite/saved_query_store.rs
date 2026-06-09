use crate::domain::error::DomainError;
use crate::domain::saved_query::SavedQuery;
use crate::ports::saved_query_repository::SavedQueryRepository;
use crate::utils::unix_now;
use super::SqliteRepository;

impl SavedQueryRepository for SqliteRepository {
    fn save(&self, connection_name: &str, name: &str, sql: &str) -> Result<(), DomainError> {
        let conn = self.lock()?;
        let connection_id = SqliteRepository::connection_id_for(&conn, connection_name)
            .ok_or_else(|| DomainError::NotFound(format!("connection '{connection_name}' not found")))?;
        conn.execute(
            "INSERT INTO saved_queries (connection_id, name, sql, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![connection_id, name, sql, unix_now()],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                DomainError::AlreadyExists(format!("saved query '{name}' already exists"))
            } else {
                DomainError::StorageError(e.to_string())
            }
        })?;
        Ok(())
    }

    fn list_by_connection(&self, connection_name: &str) -> Vec<SavedQuery> {
        let Ok(conn) = self.lock() else {
            eprintln!("pgrs: saved_queries read failed: database lock poisoned");
            return vec![];
        };
        let Some(connection_id) = SqliteRepository::connection_id_for(&conn, connection_name) else {
            return vec![];
        };
        let result: Result<Vec<SavedQuery>, rusqlite::Error> = (|| {
            let mut stmt = conn.prepare(
                "SELECT id, name, sql, created_at FROM saved_queries
                 WHERE connection_id = ?1 ORDER BY name",
            )?;
            let rows = stmt.query_map(rusqlite::params![connection_id], |r| {
                Ok(SavedQuery {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    sql: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })?;
            rows.collect()
        })();
        match result {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("pgrs: saved_queries read failed: {e}");
                vec![]
            }
        }
    }

    fn find_by_name(&self, connection_name: &str, name: &str) -> Option<SavedQuery> {
        let conn = self.lock().ok()?;
        let connection_id = SqliteRepository::connection_id_for(&conn, connection_name)?;
        conn.query_row(
            "SELECT id, name, sql, created_at FROM saved_queries
             WHERE connection_id = ?1 AND name = ?2",
            rusqlite::params![connection_id, name],
            |r| {
                Ok(SavedQuery {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    sql: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )
        .ok()
    }

    fn delete(&self, connection_name: &str, name: &str) -> Result<(), DomainError> {
        let conn = self.lock()?;
        let connection_id = SqliteRepository::connection_id_for(&conn, connection_name)
            .ok_or_else(|| DomainError::NotFound(format!("connection '{connection_name}' not found")))?;
        let n = conn
            .execute(
                "DELETE FROM saved_queries WHERE connection_id = ?1 AND name = ?2",
                rusqlite::params![connection_id, name],
            )
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("saved query '{name}' not found")));
        }
        Ok(())
    }
}
