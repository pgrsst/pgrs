use std::fmt;

#[derive(Debug, PartialEq)]
pub enum DomainError {
    NotFound(String),
    AlreadyExists(String),
    ValidationError(String),
    StorageError(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::NotFound(msg)
            | DomainError::AlreadyExists(msg)
            | DomainError::ValidationError(msg)
            | DomainError::StorageError(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<DomainError> for String {
    fn from(e: DomainError) -> Self {
        e.to_string()
    }
}
