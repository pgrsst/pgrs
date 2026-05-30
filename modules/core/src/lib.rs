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
pub use services::connection::service::{AddConnectionInput, EditConnectionInput};
pub use services::query::completions::{Completion, CompletionKind};

// --- SQL text helpers used by the REPL front-end for highlighting/tokenizing ---
pub use query::alias::SQL_KEYWORDS;
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
        AnalyticsApi::from_sqlite(&self.sqlite)
    }

    /// Schema-metadata facade with a SQLite-backed cache.
    pub fn schema_api(&self) -> SchemaApi {
        SchemaApi::from_sqlite(&self.sqlite)
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
