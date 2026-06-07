//! Driven port for system-catalog lookups (`\d`, `\d+`, `\l`).
//!
//! The trait is the capability the application layer depends on; its value
//! types live in `domain::catalog`. The PostgreSQL-specific SQL that fulfils it
//! lives in `adapters::driven::postgres_catalog`, so no DB dialect leaks inward.

use crate::domain::catalog::TableDescription;
use crate::domain::error::DomainError;

/// Capability for reading database metadata behind the REPL's catalog commands.
///
/// Like the other live-connection ports it omits `Send + Sync` by design — see
/// [`crate::ports::db_connection::DbConnection`] for the rationale.
pub trait CatalogPort {
    /// Describe a single table: columns plus indexes, FK/check constraints, and
    /// (when `extended`) triggers.
    fn describe_table(&self, table: &str, extended: bool) -> Result<TableDescription, DomainError>;

    /// List user-visible database names (the `\l` command).
    fn list_databases(&self) -> Result<Vec<String>, DomainError>;
}
