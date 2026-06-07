use std::collections::HashMap;

use crate::domain::catalog::TableDescription;
use crate::domain::error::DomainError;
use crate::domain::query_result::QueryResult;
use crate::ports::catalog_port::CatalogPort;
use crate::ports::repl_port::ReplPort;
use crate::ports::schema_port::SchemaPort;

/// Public facade for executing SQL against a live PostgreSQL connection.
///
/// Holds an opened connection as a `ReplPort`. Connections are opened through
/// the `DbConnector` port in the composition root (`Core::connect`), so this
/// facade never references a concrete driver. The UI layer uses it both for
/// running user queries and for the pg_catalog lookups behind `\d`, `\dt`, etc.
pub struct QueryApi {
    db: Box<dyn ReplPort>,
}

impl QueryApi {
    /// Wrap an already-opened connection. Built by `Core::connect` after the
    /// injected `DbConnector` opens the underlying database.
    pub(crate) fn from_port(db: Box<dyn ReplPort>) -> Self {
        Self { db }
    }

    /// Run a SQL statement and return the result set.
    pub fn execute(&self, sql: &str) -> Result<QueryResult, DomainError> {
        self.db.execute(sql)
    }

    /// Describe a table (`\d` / `\d+`): columns, indexes, constraints, triggers.
    /// The pg_catalog SQL lives in the adapter so front-ends only see the result.
    pub fn describe_table(
        &self,
        table: &str,
        extended: bool,
    ) -> Result<TableDescription, DomainError> {
        self.db.describe_table(table, extended)
    }

    /// List user-visible database names (`\l`).
    pub fn list_databases(&self) -> Result<Vec<String>, DomainError> {
        self.db.list_databases()
    }

    /// Build a `QueryApi` from any `ReplPort` implementation (test fakes).
    #[cfg(any(test, feature = "test-support"))]
    pub fn from_repl(db: Box<dyn ReplPort>) -> Self {
        Self::from_port(db)
    }
}

// Lets `SchemaApi::load`/`refresh` accept a `&QueryApi` directly: the schema
// loader only needs the `SchemaPort` capability, which we delegate to the inner db.
impl SchemaPort for QueryApi {
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
        self.db.list_columns()
    }
}
