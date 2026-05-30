use std::sync::Arc;

use crate::adapters::driven::sqlite::SqliteRepository;
use crate::domain::query_history::QueryHistory;
use crate::ports::column_access_repository::ColumnAccessRepository;
use crate::ports::connection_repository::ConnectionRepository;
use crate::ports::query_history_repository::QueryHistoryRepository;
use crate::ports::table_access_repository::TableAccessRepository;
use crate::query::alias::extract_referenced_tables;
use crate::services::analytics::service::{AnalyticsService, AnalyticsSvc};
use crate::services::column_access::service::{ColumnAccessService, ColumnAccessSvc};
use crate::services::query_history::service::{QueryHistoryService, QueryHistorySvc};
use crate::services::table_access::service::{TableAccessService, TableAccessSvc};

use super::schema::SchemaApi;

/// Public facade for usage analytics: records executed queries and exposes
/// access-frequency stats used to rank completions.
pub struct AnalyticsApi {
    svc: Arc<dyn AnalyticsSvc>,
}

impl AnalyticsApi {
    pub(crate) fn from_sqlite(sqlite: &Arc<SqliteRepository>) -> Self {
        let conn_repo = Arc::clone(sqlite) as Arc<dyn ConnectionRepository>;
        let query_history = Arc::new(QueryHistoryService::new(
            Arc::clone(&conn_repo),
            Arc::clone(sqlite) as Arc<dyn QueryHistoryRepository>,
        ));
        let table_access = Arc::new(TableAccessService::new(
            Arc::clone(&conn_repo),
            Arc::clone(sqlite) as Arc<dyn TableAccessRepository>,
        ));
        let column_access = Arc::new(ColumnAccessService::new(
            Arc::clone(&conn_repo),
            Arc::clone(sqlite) as Arc<dyn ColumnAccessRepository>,
        ));
        let svc = Arc::new(AnalyticsService::new(
            query_history as Arc<dyn QueryHistorySvc>,
            table_access as Arc<dyn TableAccessSvc>,
            column_access as Arc<dyn ColumnAccessSvc>,
        ));
        Self { svc }
    }

    /// Record an executed query plus the tables/columns it referenced.
    /// Table and column references are extracted from `sql` using `schema`.
    pub fn record_query(&self, connection_name: &str, sql: &str, schema: &SchemaApi) {
        let tables = extract_referenced_tables(sql);
        let columns = extract_column_refs(sql, schema);
        self.svc.record_query(connection_name, sql, &tables, &columns);
    }

    /// Recent query history for a connection (most recent first).
    pub fn history(&self, connection_name: &str) -> Vec<QueryHistory> {
        self.svc.get_history(connection_name)
    }

    pub fn frequent_tables(&self, connection_name: &str) -> Vec<(String, u64)> {
        self.svc
            .get_frequent_tables(connection_name)
            .into_iter()
            .map(|e| (e.name, e.count))
            .collect()
    }

    pub fn frequent_columns(&self, connection_name: &str, table: &str) -> Vec<(String, u64)> {
        self.svc
            .get_frequent_columns(connection_name, table)
            .into_iter()
            .map(|e| (e.name, e.count))
            .collect()
    }
}

/// Resolve column references in a SELECT projection against the known schema,
/// returning `(table, column)` pairs. Mirrors the previous REPL-side helper but
/// lives in core so analytics extraction is a single responsibility of the API.
fn extract_column_refs(query: &str, schema: &SchemaApi) -> Vec<(String, String)> {
    use sqlparser::ast::{Expr, SelectItem, SetExpr, Statement};
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    let candidates: Vec<String> = Parser::parse_sql(&PostgreSqlDialect {}, query)
        .ok()
        .and_then(|mut stmts| if stmts.is_empty() { None } else { Some(stmts.remove(0)) })
        .and_then(|stmt| match stmt {
            Statement::Query(q) => match *q.body {
                SetExpr::Select(sel) => Some(sel.projection),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| match item {
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => Some(ident.value.to_lowercase()),
            SelectItem::ExprWithAlias { expr: Expr::Identifier(ident), .. } => {
                Some(ident.value.to_lowercase())
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                parts.last().map(|i| i.value.to_lowercase())
            }
            _ => None,
        })
        .collect();

    let mut refs = Vec::new();
    for col in candidates {
        for table in schema.tables() {
            if schema.columns_for(table).iter().any(|c| c.to_lowercase() == col) {
                refs.push((table.to_string(), col.clone()));
                break;
            }
        }
    }
    refs
}
