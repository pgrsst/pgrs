use crate::services::schema::service::SchemaService;

use super::completions::{Completion, CompletionKind};

pub struct CommandCompletionService {
    schema: SchemaService,
}

impl CommandCompletionService {
    pub fn new(schema: SchemaService) -> Self {
        Self { schema }
    }

    /// Returns `Some` jika input adalah backslash command yang dikenali, `None` jika bukan.
    pub fn try_complete(&self, input: &str) -> Option<Vec<Completion>> {
        let table_prefix = input
            .strip_prefix("\\d+ ")
            .or_else(|| input.strip_prefix("\\d "))?;

        let results = self
            .schema
            .tables()
            .iter()
            .filter(|t| t.to_lowercase().starts_with(&table_prefix.to_lowercase()))
            .map(|t| Completion { value: t.clone(), kind: CompletionKind::Table })
            .collect();
        Some(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct TestDb {
        columns: HashMap<String, Vec<String>>,
    }

    impl crate::ports::schema_port::SchemaPort for TestDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, crate::domain::error::DomainError> {
            Ok(self.columns.clone())
        }
    }

    fn schema_with(tables: &[&str]) -> SchemaService {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for &table in tables {
            col_map.entry(table.to_string()).or_default();
        }
        let mut schema = SchemaService::new(None);
        schema.load(&TestDb { columns: col_map }, "test").unwrap();
        schema
    }

    fn service_with(tables: &[&str]) -> CommandCompletionService {
        CommandCompletionService::new(schema_with(tables))
    }

    #[test]
    fn returns_none_for_non_command_input() {
        let svc = service_with(&["users"]);
        assert!(svc.try_complete("SELECT * FROM ").is_none());
        assert!(svc.try_complete("\\dt").is_none());
        assert!(svc.try_complete("\\d").is_none());
    }

    #[test]
    fn d_suggests_all_tables_with_empty_prefix() {
        let svc = service_with(&["users", "orders"]);
        let result = svc.try_complete("\\d ").unwrap();
        let names: Vec<&str> = result.iter().map(|c| c.value.as_str()).collect();
        assert!(names.contains(&"users"), "got: {names:?}");
        assert!(names.contains(&"orders"), "got: {names:?}");
    }

    #[test]
    fn d_plus_suggests_all_tables_with_empty_prefix() {
        let svc = service_with(&["users", "orders"]);
        let result = svc.try_complete("\\d+ ").unwrap();
        let names: Vec<&str> = result.iter().map(|c| c.value.as_str()).collect();
        assert!(names.contains(&"users"), "got: {names:?}");
        assert!(names.contains(&"orders"), "got: {names:?}");
    }

    #[test]
    fn d_filters_by_prefix_case_insensitive() {
        let svc = service_with(&["users", "user_roles", "orders"]);
        let result = svc.try_complete("\\d us").unwrap();
        let names: Vec<&str> = result.iter().map(|c| c.value.as_str()).collect();
        assert!(names.contains(&"users"), "got: {names:?}");
        assert!(names.contains(&"user_roles"), "got: {names:?}");
        assert!(!names.contains(&"orders"), "orders should be filtered, got: {names:?}");
    }

    #[test]
    fn d_plus_filters_by_prefix() {
        let svc = service_with(&["users", "orders"]);
        let result = svc.try_complete("\\d+ ord").unwrap();
        let names: Vec<&str> = result.iter().map(|c| c.value.as_str()).collect();
        assert!(names.contains(&"orders"), "got: {names:?}");
        assert!(!names.contains(&"users"), "got: {names:?}");
    }

    #[test]
    fn completions_are_table_kind() {
        let svc = service_with(&["users"]);
        let result = svc.try_complete("\\d ").unwrap();
        assert!(result.iter().all(|c| matches!(c.kind, CompletionKind::Table)));
    }
}
