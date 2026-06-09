use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::saved_query::SavedQuery;
use crate::ports::saved_query_repository::SavedQueryRepository;

pub trait SavedQuerySvc: Send + Sync {
    fn save(&self, connection_name: &str, name: &str, sql: &str) -> Result<(), DomainError>;
    fn list(&self, connection_name: &str) -> Vec<SavedQuery>;
    fn get(&self, connection_name: &str, name: &str) -> Option<SavedQuery>;
    fn delete(&self, connection_name: &str, name: &str) -> Result<(), DomainError>;
}

pub struct SavedQueryService {
    repository: Arc<dyn SavedQueryRepository>,
}

impl SavedQueryService {
    pub fn new(repository: Arc<dyn SavedQueryRepository>) -> Self {
        Self { repository }
    }
}

impl SavedQuerySvc for SavedQueryService {
    fn save(&self, connection_name: &str, name: &str, sql: &str) -> Result<(), DomainError> {
        self.repository.save(connection_name, name, sql)
    }

    fn list(&self, connection_name: &str) -> Vec<SavedQuery> {
        self.repository.list_by_connection(connection_name)
    }

    fn get(&self, connection_name: &str, name: &str) -> Option<SavedQuery> {
        self.repository.find_by_name(connection_name, name)
    }

    fn delete(&self, connection_name: &str, name: &str) -> Result<(), DomainError> {
        self.repository.delete(connection_name, name)
    }
}
