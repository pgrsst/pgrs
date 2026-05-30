use std::sync::Arc;

use crate::adapters::driven::sqlite::SqliteRepository;
use crate::ports::connection_repository::ConnectionRepository;
use crate::ports::schema_column_repository::SchemaColumnRepository;
use crate::ports::schema_table_repository::SchemaTableRepository;
use crate::services::schema::service::SchemaService;
use crate::services::schema_cache::service::{SchemaCacheService, SchemaCacheSvc};
use crate::services::schema_column::service::{SchemaColumnService, SchemaColumnSvc};
use crate::services::schema_table::service::{SchemaTableService, SchemaTableSvc};

use super::query::QueryApi;

/// Public facade over schema metadata (table + column names) used to drive
/// completion. Backed by `SchemaService` with a SQLite-backed cache.
#[derive(Clone)]
pub struct SchemaApi {
    inner: SchemaService,
}

impl SchemaApi {
    pub(crate) fn from_sqlite(sqlite: &Arc<SqliteRepository>) -> Self {
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
        Self {
            inner: SchemaService::new(Some(cache as Arc<dyn SchemaCacheSvc>)),
        }
    }

    /// Load schema metadata for `connection_name`, using the cache when present.
    pub fn load(&mut self, query: &QueryApi, connection_name: &str) -> Result<(), String> {
        self.inner.load(query, connection_name)
    }

    /// Invalidate the cache and reload schema metadata from the database.
    pub fn refresh(&mut self, query: &QueryApi, connection_name: &str) -> Result<(), String> {
        self.inner.refresh(query, connection_name)
    }

    pub fn tables(&self) -> &[String] {
        self.inner.tables()
    }

    pub fn columns_for(&self, table: &str) -> &[String] {
        self.inner.columns_for(table)
    }

    pub(crate) fn clone_service(&self) -> SchemaService {
        self.inner.clone()
    }

    /// Build a `SchemaApi` directly from a `table -> columns` map, without a
    /// live database or cache. For downstream test suites (`test-support`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test(columns: std::collections::HashMap<String, Vec<String>>) -> Self {
        use crate::ports::schema_port::SchemaPort;

        struct InMemory(std::collections::HashMap<String, Vec<String>>);
        impl SchemaPort for InMemory {
            fn list_columns(
                &self,
            ) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
                Ok(self.0.clone())
            }
        }

        let mut inner = SchemaService::new(None);
        inner
            .load(&InMemory(columns), "test")
            .expect("load in-memory schema for tests");
        Self { inner }
    }
}
