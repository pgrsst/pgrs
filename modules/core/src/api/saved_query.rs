use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::saved_query::SavedQuery;
use crate::services::saved_query::service::SavedQuerySvc;

/// Public facade for per-connection saved queries (the CLI's "favorites").
/// Thin delegator over an assembled [`SavedQuerySvc`]; wiring lives in `Core`.
pub struct SavedQueryApi {
    svc: Arc<dyn SavedQuerySvc>,
}

impl SavedQueryApi {
    pub(crate) fn new(svc: Arc<dyn SavedQuerySvc>) -> Self {
        Self { svc }
    }

    /// Save `sql` under `name` for `connection_name`. Errors with
    /// [`DomainError::AlreadyExists`] if the name is already taken.
    pub fn save(&self, connection_name: &str, name: &str, sql: &str) -> Result<(), DomainError> {
        self.svc.save(connection_name, name, sql)
    }

    /// All saved queries for a connection, ordered by name.
    pub fn list(&self, connection_name: &str) -> Vec<SavedQuery> {
        self.svc.list(connection_name)
    }

    /// Look up a single saved query by name.
    pub fn get(&self, connection_name: &str, name: &str) -> Option<SavedQuery> {
        self.svc.get(connection_name, name)
    }

    /// Delete a saved query; errors with [`DomainError::NotFound`] if absent.
    pub fn delete(&self, connection_name: &str, name: &str) -> Result<(), DomainError> {
        self.svc.delete(connection_name, name)
    }
}

#[cfg(test)]
mod tests {
    use crate::Core;
    use crate::domain::error::DomainError;
    use crate::services::connection::service::AddConnectionInput;
    use crate::enums::tls_mode::TlsMode;
    use crate::domain::connection::DEFAULT_PORT;

    fn core_with_connection(name: &str) -> Core {
        let core = Core::in_memory();
        core.connection
            .add(AddConnectionInput {
                name: name.to_string(),
                host: "localhost".to_string(),
                port: DEFAULT_PORT,
                username: "u".to_string(),
                password: "p".to_string(),
                database: "db".to_string(),
                tls: TlsMode::Disable,
                environment: None,
            })
            .unwrap();
        core
    }

    #[test]
    fn save_then_get_round_trips() {
        let core = core_with_connection("mydb");
        let api = core.saved_query_api();
        api.save("mydb", "users", "SELECT * FROM users").unwrap();

        let got = api.get("mydb", "users").unwrap();
        assert_eq!(got.name, "users");
        assert_eq!(got.sql, "SELECT * FROM users");
    }

    #[test]
    fn get_returns_none_when_absent() {
        let core = core_with_connection("mydb");
        let api = core.saved_query_api();
        assert!(api.get("mydb", "ghost").is_none());
    }

    #[test]
    fn list_returns_saved_queries() {
        let core = core_with_connection("mydb");
        let api = core.saved_query_api();
        api.save("mydb", "a", "SELECT 1").unwrap();
        api.save("mydb", "b", "SELECT 2").unwrap();
        assert_eq!(api.list("mydb").len(), 2);
    }

    #[test]
    fn save_duplicate_name_errors() {
        let core = core_with_connection("mydb");
        let api = core.saved_query_api();
        api.save("mydb", "dup", "SELECT 1").unwrap();
        let err = api.save("mydb", "dup", "SELECT 2").unwrap_err();
        assert!(matches!(err, DomainError::AlreadyExists(_)));
    }

    #[test]
    fn delete_removes_saved_query() {
        let core = core_with_connection("mydb");
        let api = core.saved_query_api();
        api.save("mydb", "q", "SELECT 1").unwrap();
        api.delete("mydb", "q").unwrap();
        assert!(api.get("mydb", "q").is_none());
    }

    #[test]
    fn delete_absent_errors() {
        let core = core_with_connection("mydb");
        let api = core.saved_query_api();
        let err = api.delete("mydb", "ghost").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
