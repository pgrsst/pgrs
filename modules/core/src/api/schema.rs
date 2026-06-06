use crate::services::schema::service::SchemaService;

use super::query::QueryApi;

/// Public facade over schema metadata (table + column names) used to drive
/// completion. Backed by `SchemaService` with a SQLite-backed cache.
#[derive(Clone)]
pub struct SchemaApi {
    inner: SchemaService,
}

impl SchemaApi {
    /// Wrap an assembled `SchemaService`. Service wiring lives in the
    /// composition root (`Core`); this facade stays a thin delegator.
    pub(crate) fn new(inner: SchemaService) -> Self {
        Self { inner }
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
