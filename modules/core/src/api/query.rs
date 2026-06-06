use std::collections::HashMap;

use crate::adapters::driven::postgres_db::PostgresDb;
use crate::domain::connection::Connection;
use crate::domain::error::DomainError;
use crate::ports::db_connection::QueryResult;
use crate::ports::repl_port::ReplPort;
use crate::ports::schema_port::SchemaPort;
use crate::services::catalog::TableDescription;

/// Public facade for executing SQL against a live PostgreSQL connection.
///
/// Wraps the driven `PostgresDb` adapter. The UI layer uses this both for
/// running user queries and for the pg_catalog lookups behind `\d`, `\dt`, etc.
pub struct QueryApi {
    db: Box<dyn ReplPort>,
}

impl QueryApi {
    /// Open a live connection to the database described by `connection`.
    pub fn connect(connection: &Connection) -> Result<Self, DomainError> {
        let db = PostgresDb::new(connection)?;
        Ok(Self { db: Box::new(db) })
    }

    /// Run a SQL statement and return the result set.
    pub fn execute(&self, sql: &str) -> Result<QueryResult, DomainError> {
        self.db.execute(sql)
    }

    /// Describe a table (`\d` / `\d+`): columns, indexes, constraints, triggers.
    /// The pg_catalog SQL lives in the core so front-ends only see the result.
    pub fn describe_table(
        &self,
        table: &str,
        extended: bool,
    ) -> Result<TableDescription, DomainError> {
        crate::services::catalog::describe_table(&*self.db, table, extended)
    }

    /// List user-visible database names (`\l`).
    pub fn list_databases(&self) -> Result<Vec<String>, DomainError> {
        crate::services::catalog::list_databases(&*self.db)
    }

    /// Build a `QueryApi` from any `ReplPort` implementation (test fakes).
    #[cfg(any(test, feature = "test-support"))]
    pub fn from_repl(db: Box<dyn ReplPort>) -> Self {
        Self { db }
    }
}

// Lets `SchemaApi::load`/`refresh` accept a `&QueryApi` directly: the schema
// loader only needs the `SchemaPort` capability, which we delegate to the inner db.
impl SchemaPort for QueryApi {
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
        self.db.list_columns()
    }
}
