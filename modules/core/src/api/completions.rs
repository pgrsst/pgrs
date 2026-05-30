use std::collections::HashMap;

use crate::services::query::completions::CompletionService;

use super::schema::SchemaApi;

/// Public facade for SQL auto-completion.
///
/// Given the loaded schema and access-frequency hints, produces ranked
/// completion candidates for a query buffer + cursor position.
pub struct CompletionsApi {
    inner: CompletionService,
}

impl CompletionsApi {
    pub fn new(
        schema: &SchemaApi,
        table_freq: HashMap<String, u64>,
        column_freq: HashMap<String, u64>,
    ) -> Self {
        Self {
            inner: CompletionService::new(schema.clone_service(), table_freq, column_freq),
        }
    }

    pub fn completions(&self, query: &str, cursor: usize) -> Vec<crate::Completion> {
        self.inner.completions(query, cursor)
    }
}
