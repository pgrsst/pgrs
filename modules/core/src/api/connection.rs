use std::sync::Arc;

use crate::adapters::driven::sqlite::SqliteRepository;
use crate::domain::connection::Connection;
use crate::domain::error::DomainError;
use crate::ports::connection_repository::ConnectionRepository;
use crate::services::connection::service::{
    AddConnectionInput, ConnectionService, ConnectionSvc, EditConnectionInput,
};

/// Public facade for managing named connection configurations.
///
/// This is the only entry point the UI layer (`pgrs-cli`, future `pgrs-desktop`,
/// `pgrs-web`) should use to manage connections. It hides the underlying
/// `ConnectionService` and repository wiring.
pub struct ConnectionApi {
    svc: Arc<dyn ConnectionSvc>,
}

impl ConnectionApi {
    pub(crate) fn from_sqlite(sqlite: &Arc<SqliteRepository>) -> Self {
        let repo = Arc::clone(sqlite) as Arc<dyn ConnectionRepository>;
        Self::new(repo)
    }

    pub(crate) fn new(repo: Arc<dyn ConnectionRepository>) -> Self {
        Self {
            svc: Arc::new(ConnectionService::new(repo)),
        }
    }

    pub fn add(&self, input: AddConnectionInput) -> Result<(), DomainError> {
        self.svc.add_connection(input)
    }

    pub fn list(&self) -> Result<Vec<Connection>, DomainError> {
        self.svc.list_connections()
    }

    pub fn delete(&self, name: &str) -> Result<(), DomainError> {
        self.svc.delete_connection(name)
    }

    pub fn edit(&self, name: &str, input: EditConnectionInput) -> Result<(), DomainError> {
        self.svc.edit_connection(name, input)
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> Result<(), DomainError> {
        self.svc.rename_connection(old_name, new_name)
    }

    pub fn find(&self, name_or_id: &str) -> Result<Connection, DomainError> {
        self.svc.find_connection(name_or_id)
    }

    pub fn get(&self, name: &str) -> Result<Connection, DomainError> {
        self.svc.get_connection(name)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ConnectionApi {
    /// Build a `ConnectionApi` backed by an in-memory SQLite store.
    /// Available to downstream test suites via the `test-support` feature.
    pub fn in_memory() -> Self {
        let repo = Arc::new(
            SqliteRepository::open_in_memory().expect("open in-memory sqlite for tests"),
        );
        Self::new(repo as Arc<dyn ConnectionRepository>)
    }

    /// Build an in-memory `ConnectionApi` pre-populated with the given connection names.
    pub fn in_memory_with(names: &[&str]) -> Self {
        use crate::enums::tls_mode::TlsMode;
        let api = Self::in_memory();
        for name in names {
            api.add(AddConnectionInput {
                name: name.to_string(),
                host: "localhost".to_string(),
                port: crate::domain::connection::DEFAULT_PORT,
                username: "user".to_string(),
                password: "pass".to_string(),
                database: "db".to_string(),
                tls: TlsMode::Disable,
                environment: None,
            })
            .expect("seed in-memory connection");
        }
        api
    }
}
