use crate::domain::error::DomainError;
use crate::domain::query_result::QueryResult;

/// Capability to run a SQL statement against a live database.
///
/// Thread-safety: this and the other live-connection ports (`SchemaPort`,
/// `CatalogPort`, `ReplPort`) deliberately omit `Send + Sync`. A connection is
/// owned and used by the single-threaded REPL only, and the Postgres adapter
/// keeps its client in a `RefCell` (cheaper than a `Mutex` for that use). The
/// *config-store* repository ports require `Send + Sync` instead, because they
/// are shared across the app via `Arc`; if a future multi-threaded front-end
/// needs to share a connection, swap the adapter's `RefCell` for a `Mutex` and
/// add the bounds here.
pub trait DbConnection {
    fn execute(&self, query: &str) -> Result<QueryResult, DomainError>;
}
