//! `pgrs-core` — all connection/query/schema/completion/analytics logic for pgrs.
//!
//! UI front-ends (`pgrs-cli`, future `pgrs-desktop`/`pgrs-web`) depend only on
//! the [`api`] facade plus the re-exported value types below. Internal modules
//! (`ports`, `services`, `adapters`, `query`) are crate-private so the boundary
//! is enforced by the compiler, not convention.

pub mod api;
pub mod domain;
pub mod enums;

pub(crate) mod adapters;
pub(crate) mod ports;
pub(crate) mod query;
pub(crate) mod services;
pub(crate) mod utils;

use std::sync::Arc;

use crate::adapters::driven::sqlite::SqliteRepository;
use crate::ports::column_access_repository::ColumnAccessRepository;
use crate::ports::connection_repository::ConnectionRepository;
use crate::ports::query_history_repository::QueryHistoryRepository;
use crate::ports::schema_column_repository::SchemaColumnRepository;
use crate::ports::schema_table_repository::SchemaTableRepository;
use crate::ports::table_access_repository::TableAccessRepository;
use crate::services::analytics::service::{AnalyticsService, AnalyticsSvc};
use crate::services::column_access::service::{ColumnAccessService, ColumnAccessSvc};
use crate::services::query_history::service::{QueryHistoryService, QueryHistorySvc};
use crate::services::schema::service::SchemaService;
use crate::services::schema_cache::service::{SchemaCacheService, SchemaCacheSvc};
use crate::services::schema_column::service::{SchemaColumnService, SchemaColumnSvc};
use crate::services::schema_table::service::{SchemaTableService, SchemaTableSvc};
use crate::services::table_access::service::{TableAccessService, TableAccessSvc};

// --- Public API surface ---
pub use api::analytics::AnalyticsApi;
pub use api::completions::CompletionsApi;
pub use api::connection::ConnectionApi;
pub use api::query::QueryApi;
pub use api::schema::SchemaApi;

// --- Value types used in API signatures ---
pub use domain::connection::{Connection, DEFAULT_PORT};
pub use domain::error::DomainError;
pub use domain::query_history::QueryHistory;
pub use enums::tls_mode::TlsMode;
pub use ports::db_connection::{DbConnection, QueryResult};
pub use ports::repl_port::ReplPort;
pub use ports::schema_port::SchemaPort;
pub use services::catalog::{NamedDef, TableDescription};
pub use services::connection::service::{AddConnectionInput, EditConnectionInput};
pub use services::query::completions::{Completion, CompletionKind};

// --- SQL text helpers used by the REPL front-end for highlighting/tokenizing ---
pub use query::alias::SQL_KEYWORDS;
pub use query::classify::{is_ddl, is_dml};
pub use query::tokenizer::{SqlToken, tokenize};

/// Root composition object. Owns the shared SQLite store and hands out API
/// facades wired against it. Construct once at process start via [`Core::init`].
pub struct Core {
    sqlite: Arc<SqliteRepository>,
    /// Connection management facade (always available; needs no live DB).
    pub connection: ConnectionApi,
}

impl Core {
    /// Open (and migrate) the SQLite store at `db_path` and wire up the facades.
    pub fn init(db_path: &str) -> Result<Self, String> {
        let sqlite = Arc::new(
            SqliteRepository::open(db_path).map_err(|e| format!("could not open database: {e}"))?,
        );
        let connection = ConnectionApi::from_sqlite(&sqlite);
        Ok(Self { sqlite, connection })
    }

    /// Analytics facade (query history + access frequency), backed by the store.
    pub fn analytics_api(&self) -> AnalyticsApi {
        build_analytics_api(&self.sqlite)
    }

    /// Schema-metadata facade with a SQLite-backed cache.
    pub fn schema_api(&self) -> SchemaApi {
        build_schema_api(&self.sqlite)
    }

    /// Build a `Core` backed by an in-memory SQLite store. All facades handed
    /// out share the same store, so connections seeded via `connection` are
    /// visible to `analytics_api`/`schema_api`. For downstream test suites.
    #[cfg(any(test, feature = "test-support"))]
    pub fn in_memory() -> Self {
        let sqlite = Arc::new(
            SqliteRepository::open_in_memory().expect("open in-memory sqlite for tests"),
        );
        let connection = ConnectionApi::from_sqlite(&sqlite);
        Self { sqlite, connection }
    }
}

// --- Service composition ---
// The SQLite repository implements every driven-port trait, so each facade is
// wired by casting one shared `Arc<SqliteRepository>` into the ports its
// services need. Keeping this assembly in the composition root leaves the API
// facades as thin delegators.

/// Assemble the analytics service tree (query history + table/column access)
/// and wrap it in an [`AnalyticsApi`] facade.
fn build_analytics_api(sqlite: &Arc<SqliteRepository>) -> AnalyticsApi {
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
    AnalyticsApi::new(svc as Arc<dyn AnalyticsSvc>)
}

/// Assemble the schema service (with a SQLite-backed cache) and wrap it in a
/// [`SchemaApi`] facade.
fn build_schema_api(sqlite: &Arc<SqliteRepository>) -> SchemaApi {
    let conn_repo = Arc::clone(sqlite) as Arc<dyn ConnectionRepository>;
    let table_svc = Arc::new(SchemaTableService::new(
        Arc::clone(&conn_repo),
        Arc::clone(sqlite) as Arc<dyn SchemaTableRepository>,
    ));
    let column_svc = Arc::new(SchemaColumnService::new(
        Arc::clone(&conn_repo),
        Arc::clone(sqlite) as Arc<dyn SchemaColumnRepository>,
    ));
    let cache = Arc::new(SchemaCacheService::new(
        table_svc as Arc<dyn SchemaTableSvc>,
        column_svc as Arc<dyn SchemaColumnSvc>,
    ));
    SchemaApi::new(SchemaService::new(Some(cache as Arc<dyn SchemaCacheSvc>)))
}
