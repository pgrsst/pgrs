use crate::domain::error::DomainError;
use crate::domain::saved_query::SavedQuery;

pub trait SavedQueryRepository: Send + Sync {
    /// Persist `sql` under `name` for the named connection. Errors with
    /// [`DomainError::AlreadyExists`] when the name is already taken for that
    /// connection — no silent overwrite.
    fn save(&self, connection_name: &str, name: &str, sql: &str) -> Result<(), DomainError>;
    fn list_by_connection(&self, connection_name: &str) -> Vec<SavedQuery>;
    fn find_by_name(&self, connection_name: &str, name: &str) -> Option<SavedQuery>;
    fn delete(&self, connection_name: &str, name: &str) -> Result<(), DomainError>;
}
