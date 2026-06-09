use std::collections::HashMap;

use crate::domain::catalog::TableDescription;
use crate::domain::error::DomainError;
use crate::domain::explain::ExplainPlan;
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

    /// Run `EXPLAIN` (`\explain`) or `EXPLAIN ANALYZE` (`\explain+`) and return
    /// the parsed plan tree. The pg-specific SQL/JSON lives in the adapter.
    pub fn explain(&self, sql: &str, analyze: bool) -> Result<ExplainPlan, DomainError> {
        self.db.explain(sql, analyze)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::DomainError;
    use crate::ports::db_connection::DbConnection;
    use crate::ports::schema_port::SchemaPort;
    use std::collections::HashMap;

    struct StubDb {
        json: String,
    }

    impl DbConnection for StubDb {
        fn execute(&self, _sql: &str) -> Result<QueryResult, DomainError> {
            Ok(QueryResult {
                columns: vec!["QUERY PLAN".into()],
                rows: vec![vec![self.json.clone()]],
                rows_affected: None,
            })
        }
    }

    impl SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
            Ok(HashMap::new())
        }
    }

    #[test]
    fn explain_delegates_to_port_and_returns_plan() {
        let json = r#"[{"Plan":{"Node Type":"Seq Scan","Total Cost":1.0,"Plan Rows":1}}]"#;
        let api = QueryApi::from_port(Box::new(StubDb { json: json.to_string() }));
        let plan = api.explain("SELECT 1", false).unwrap();
        assert_eq!(plan.root.node_type, "Seq Scan");
    }
}
