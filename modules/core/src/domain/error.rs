use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    NotFound(String),
    AlreadyExists(String),
    ValidationError(String),
    StorageError(String),
    /// Failure talking to a live database: connecting, executing a query, or
    /// reading schema metadata. Distinct from `StorageError` (the local SQLite
    /// config store) so the port boundary uses one error type, not `String`.
    QueryError(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::NotFound(msg)
            | DomainError::AlreadyExists(msg)
            | DomainError::ValidationError(msg)
            | DomainError::StorageError(msg)
            | DomainError::QueryError(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<DomainError> for String {
    fn from(e: DomainError) -> Self {
        e.to_string()
    }
}
