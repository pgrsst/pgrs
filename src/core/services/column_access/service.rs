use std::sync::Arc;

use crate::core::domain::analytics::FreqEntry;
use crate::core::domain::column_access::ColumnAccess;
use crate::core::domain::error::DomainError;
use crate::core::ports::column_access_repository::ColumnAccessRepository;
use crate::core::ports::connection_repository::ConnectionRepository;
use crate::core::utils::unix_now;

pub struct ColumnAccessCreateInput {
    pub connection_name: String,
    pub table_name: String,
    pub column_name: String,
    pub query_id: Option<i64>,
}

pub trait ColumnAccessSvc: Send + Sync {
    fn record(&self, input: ColumnAccessCreateInput) -> Result<(), DomainError>;
    fn get_frequent_by_table(&self, connection_name: &str, table: &str) -> Vec<FreqEntry>;
}

pub struct ColumnAccessService {
    connection_repo: Arc<dyn ConnectionRepository>,
    repository: Arc<dyn ColumnAccessRepository>,
}

impl ColumnAccessService {
    pub fn new(
        connection_repo: Arc<dyn ConnectionRepository>,
        repository: Arc<dyn ColumnAccessRepository>,
    ) -> Self {
        Self { connection_repo, repository }
    }

    pub fn record(&self, input: ColumnAccessCreateInput) -> Result<(), DomainError> {
        let connection_id = self.connection_repo.find_row_id(&input.connection_name)?;
        let now = unix_now();
        let entity = ColumnAccess {
            id: 0,
            connection_id,
            table_name: input.table_name,
            column_name: input.column_name,
            query_id: input.query_id,
            accessed_at: now,
        };
        self.repository.save(&entity)
    }

    pub fn get_frequent_by_table(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        self.repository.list_frequent_by_table(connection_name, table, 100)
    }
}

impl ColumnAccessSvc for ColumnAccessService {
    fn record(&self, input: ColumnAccessCreateInput) -> Result<(), DomainError> {
        self.record(input)
    }

    fn get_frequent_by_table(&self, connection_name: &str, table: &str) -> Vec<FreqEntry> {
        self.get_frequent_by_table(connection_name, table)
    }
}
