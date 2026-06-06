use crate::domain::error::DomainError;
use crate::ports::connection_repository::ConnectionRepository;

/// Resolve a connection's numeric id by name, erroring if the connection is
/// missing or has no persisted id. Shared by the analytics services that need a
/// `connection_id` foreign key before writing access/history rows.
pub(crate) fn resolve_connection_id(
    repo: &dyn ConnectionRepository,
    name: &str,
) -> Result<i64, DomainError> {
    repo.get_connection(name)?
        .id
        .ok_or_else(|| DomainError::StorageError("connection has no id".to_string()))
}

pub mod analytics;
pub mod catalog;
pub mod column_access;
pub mod connection;
pub mod query;
pub mod query_history;
pub mod schema;
pub mod schema_cache;
pub mod schema_column;
pub mod schema_table;
pub mod table_access;
