use std::collections::HashMap;

use crate::services::schema::service::SchemaService;

use super::command_completion::CommandCompletionService;
use super::query_completion::QueryCompletionService;

#[derive(Debug, Clone, PartialEq)]
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}

#[derive(Debug, Clone)]
pub struct Completion {
    pub value: String,
    pub kind: CompletionKind,
}

pub struct CompletionService {
    command: CommandCompletionService,
    query: QueryCompletionService,
}

impl CompletionService {
    pub fn new(
        schema: SchemaService,
        table_freq: HashMap<String, u64>,
        column_freq: HashMap<String, u64>,
    ) -> Self {
        Self {
            command: CommandCompletionService::new(schema.clone()),
            query: QueryCompletionService::new(schema, table_freq, column_freq),
        }
    }

    pub fn completions(&self, query: &str, cursor: usize) -> Vec<Completion> {
        let input = &query[..cursor.min(query.len())];
        if let Some(result) = self.command.try_complete(input) {
            return result;
        }
        self.query.completions(query, cursor)
    }
}
