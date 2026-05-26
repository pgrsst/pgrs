use std::sync::Arc;

use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::error::DomainError;
use crate::core::domain::table_access::TableAccess;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::ports::table_access_repository::TableAccessRepository;
use crate::core::utils::unix_now;

pub struct TableAccessCreateInput {
    pub connection_name: String,
    pub table_name: String,
    pub query_id: Option<i64>,
}

pub trait TableAccessSvc: Send + Sync {
    fn record(&self, input: TableAccessCreateInput) -> Result<(), DomainError>;
    fn get_frequent(&self, connection_name: &str) -> Vec<FreqEntry>;
}

pub struct TableAccessService {
    connection_repo: Arc<dyn ConnectionRepository>,
    repository: Arc<dyn TableAccessRepository>,
}

impl TableAccessService {
    pub fn new(
        connection_repo: Arc<dyn ConnectionRepository>,
        repository: Arc<dyn TableAccessRepository>,
    ) -> Self {
        Self { connection_repo, repository }
    }

    pub fn record(&self, input: TableAccessCreateInput) -> Result<(), DomainError> {
        let connection_id = self.connection_repo.find_row_id(&input.connection_name)?;
        let now = unix_now();
        let entity = TableAccess {
            id: 0,
            connection_id,
            table_name: input.table_name,
            query_id: input.query_id,
            accessed_at: now,
        };
        self.repository.save(&entity)
    }

    pub fn get_frequent(&self, connection_name: &str) -> Vec<FreqEntry> {
        self.repository.list_frequent(connection_name, 100)
    }
}

impl TableAccessSvc for TableAccessService {
    fn record(&self, input: TableAccessCreateInput) -> Result<(), DomainError> {
        self.record(input)
    }

    fn get_frequent(&self, connection_name: &str) -> Vec<FreqEntry> {
        self.get_frequent(connection_name)
    }
}
