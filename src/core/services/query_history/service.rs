use std::sync::Arc;

use crate::core::domain::error::DomainError;
use crate::core::domain::query_history::QueryHistory;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::query_history_repository::QueryHistoryRepository;
use crate::core::utils::unix_now;

pub struct QueryHistoryCreateInput {
    pub connection_name: String,
    pub query: String,
}

pub trait QueryHistorySvc: Send + Sync {
    fn record(&self, input: QueryHistoryCreateInput) -> Result<i64, DomainError>;
    fn list_recent(&self, connection_name: &str) -> Vec<QueryHistory>;
}

pub struct QueryHistoryService {
    connection_repo: Arc<dyn ConnectionRepository>,
    repository: Arc<dyn QueryHistoryRepository>,
}

impl QueryHistoryService {
    pub fn new(
        connection_repo: Arc<dyn ConnectionRepository>,
        repository: Arc<dyn QueryHistoryRepository>,
    ) -> Self {
        Self { connection_repo, repository }
    }

    pub fn record(&self, input: QueryHistoryCreateInput) -> Result<i64, DomainError> {
        let connection_id = self.connection_repo.find_row_id(&input.connection_name)?;
        let now = unix_now();
        let entity = QueryHistory {
            id: 0,
            connection_id,
            query: input.query,
            executed_at: now,
        };
        self.repository.save(&entity)
    }

    pub fn list_recent(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.repository.list_recent(connection_name, 50)
    }
}

impl QueryHistorySvc for QueryHistoryService {
    fn record(&self, input: QueryHistoryCreateInput) -> Result<i64, DomainError> {
        self.record(input)
    }

    fn list_recent(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.list_recent(connection_name)
    }
}
