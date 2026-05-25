use crate::core::domain::connection::{Connection as DomainConnection, TlsMode};
use crate::core::domain::error::DomainError;
use crate::core::ports::connection_repository::ConnectionRepository;
use super::SqliteRepository;

pub(super) fn tls_from_str(s: &str) -> TlsMode {
    match s {
        "require" => TlsMode::Require,
        "verify-full" => TlsMode::VerifyFull,
        _ => TlsMode::Disable,
    }
}

impl ConnectionRepository for SqliteRepository {
    fn add(&self, connection: DomainConnection) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO connections (name, host, port, username, password, database, tls, environment, uuid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                connection.name(),
                connection.host(),
                connection.port() as i64,
                connection.username(),
                connection.password(),
                connection.database(),
                connection.tls().to_string(),
                connection.environment(),
                connection.id(),
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                DomainError::AlreadyExists(format!("connection '{}' already exists", connection.name()))
            } else {
                DomainError::StorageError(e.to_string())
            }
        })?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<DomainConnection>, DomainError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name, host, port, username, password, database, tls, environment, uuid
                 FROM connections ORDER BY name",
            )
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                let tls_str: String = r.get(6)?;
                Ok(DomainConnection::from_storage(
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, i64>(2)? as u16,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    tls_from_str(&tls_str),
                    r.get(7)?,
                    r.get(8)?,
                ))
            })
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::StorageError(e.to_string()))
    }

    fn delete(&self, name: &str) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute("DELETE FROM connections WHERE name = ?1", rusqlite::params![name])
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("connection '{}' not found", name)));
        }
        Ok(())
    }

    fn get_connection(&self, name: &str) -> Result<DomainConnection, DomainError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT name, host, port, username, password, database, tls, environment, uuid
             FROM connections WHERE name = ?1",
            rusqlite::params![name],
            |r| {
                let tls_str: String = r.get(6)?;
                Ok(DomainConnection::from_storage(
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, i64>(2)? as u16,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    tls_from_str(&tls_str),
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .map_err(|e| {
            if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                DomainError::NotFound(format!("connection '{}' not found", name))
            } else {
                DomainError::StorageError(e.to_string())
            }
        })
    }

    fn update(&self, connection: DomainConnection) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE connections SET host=?1, port=?2, username=?3, password=?4,
                 database=?5, tls=?6, environment=?7, uuid=?8 WHERE name=?9",
                rusqlite::params![
                    connection.host(),
                    connection.port() as i64,
                    connection.username(),
                    connection.password(),
                    connection.database(),
                    connection.tls().to_string(),
                    connection.environment(),
                    connection.id(),
                    connection.name(),
                ],
            )
            .map_err(|e| DomainError::StorageError(e.to_string()))?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("connection '{}' not found", connection.name())));
        }
        Ok(())
    }

    fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
        let conn = self.conn.lock().unwrap();
        let n = conn
            .execute(
                "UPDATE connections SET name = ?1 WHERE name = ?2",
                rusqlite::params![new_name, old_name],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint failed") {
                    DomainError::AlreadyExists(format!("connection '{}' already exists", new_name))
                } else {
                    DomainError::StorageError(e.to_string())
                }
            })?;
        if n == 0 {
            return Err(DomainError::NotFound(format!("connection '{}' not found", old_name)));
        }
        Ok(())
    }
}
