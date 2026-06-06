use std::collections::HashMap;

use crate::query::alias::{build_alias_map, extract_join_context, AliasMap, SQL_KEYWORDS};
use crate::services::schema::service::SchemaService;

use super::completions::{Completion, CompletionKind};

const TABLE_TRIGGERS: &[&str] = &["FROM", "JOIN", "INTO", "UPDATE"];
const COLUMN_TRIGGERS: &[&str] = &["SELECT", "WHERE", "ON", "SET", "BY"];

pub struct QueryCompletionService {
    schema: SchemaService,
    table_freq: HashMap<String, u64>,
    column_freq: HashMap<String, u64>,
}

impl QueryCompletionService {
    pub fn new(
        schema: SchemaService,
        table_freq: HashMap<String, u64>,
        column_freq: HashMap<String, u64>,
    ) -> Self {
        Self { schema, table_freq, column_freq }
    }

    pub fn completions(&self, query: &str, cursor: usize) -> Vec<Completion> {
        let input = &query[..cursor.min(query.len())];
        let alias_map = build_alias_map(query);

        if let Some(result) = self.try_complete_qualified(input, &alias_map) {
            return result;
        }

        let (effective_trigger, current_word) = resolve_trigger_and_word(input);
        let candidates =
            self.candidates_for_trigger(&effective_trigger, &query.to_uppercase(), &alias_map);
        self.filter_and_sort(candidates, &effective_trigger, &current_word)
    }

    fn table_completions(&self, prefix: &str) -> Vec<Completion> {
        self.schema
            .tables()
            .iter()
            .filter(|t| t.to_uppercase().starts_with(&prefix.to_uppercase()))
            .map(|t| Completion { value: t.clone(), kind: CompletionKind::Table })
            .collect()
    }

    fn column_completions(&self, table_refs: &[String], prefix: &str) -> Vec<Completion> {
        table_refs
            .iter()
            .flat_map(|t| self.schema.columns_for(t).iter().cloned())
            .filter(|c| c.to_uppercase().starts_with(&prefix.to_uppercase()))
            .map(|c| Completion { value: c, kind: CompletionKind::Column })
            .collect()
    }

    fn qualified_column_completions(
        &self,
        table_refs: &[String],
        alias_map: &AliasMap,
        prefix: &str,
    ) -> Vec<Completion> {
        let mut results = Vec::new();
        for table in table_refs {
            let aliases = alias_map.aliases_for_table(table);
            for col in self.schema.columns_for(table) {
                if !col.to_uppercase().starts_with(&prefix.to_uppercase()) {
                    continue;
                }
                results.push(Completion { value: col.clone(), kind: CompletionKind::Column });
                results.push(Completion { value: format!("{table}.{col}"), kind: CompletionKind::Column });
                for alias in &aliases {
                    results.push(Completion { value: format!("{alias}.{col}"), kind: CompletionKind::Column });
                }
            }
        }
        results
    }

    fn keyword_completions(&self, prefix: &str) -> Vec<Completion> {
        SQL_KEYWORDS
            .iter()
            .filter(|k| k.starts_with(prefix.to_uppercase().as_str()))
            .map(|k| Completion { value: k.to_string(), kind: CompletionKind::Keyword })
            .collect()
    }

    fn try_complete_qualified(&self, input: &str, alias_map: &AliasMap) -> Option<Vec<Completion>> {
        let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
        let token = &input[last_ws..];
        let dot_pos = token.rfind('.')?;
        let table_name = token[..dot_pos]
            .split('.')
            .next_back()
            .unwrap_or(&token[..dot_pos])
            .to_lowercase();
        let col_prefix = token[dot_pos + 1..].to_uppercase();
        Some(self.complete_qualified(&table_name, &col_prefix, alias_map))
    }

    fn complete_qualified(&self, table_name: &str, col_prefix: &str, alias_map: &AliasMap) -> Vec<Completion> {
        let resolved = alias_map.resolve(table_name).unwrap_or(table_name);
        let cols = self.schema.columns_for(resolved);
        if !cols.is_empty() {
            cols.iter()
                .filter(|c| c.to_uppercase().starts_with(&col_prefix.to_uppercase()))
                .map(|c| Completion { value: c.to_string(), kind: CompletionKind::Column })
                .collect()
        } else {
            self.schema
                .tables()
                .iter()
                .flat_map(|t| self.schema.columns_for(t).iter().cloned())
                .filter(|c| c.to_uppercase().starts_with(&col_prefix.to_uppercase()))
                .map(|c| Completion { value: c, kind: CompletionKind::Column })
                .collect()
        }
    }

    fn candidates_for_trigger(&self, trigger: &str, upper_query: &str, alias_map: &AliasMap) -> Vec<Completion> {
        match trigger {
            "FROM" | "JOIN" | "INTO" | "UPDATE" => self.table_completions(""),
            "ON" => {
                if let Some(ctx) = extract_join_context(upper_query, alias_map) {
                    let right_cols: Vec<String> = self.schema.columns_for(&ctx.right_table).to_vec();
                    let left_cols: Vec<String> = ctx
                        .left_tables
                        .iter()
                        .flat_map(|t| self.schema.columns_for(t).iter().cloned())
                        .collect();

                    let left_lower: std::collections::HashSet<String> =
                        left_cols.iter().map(|c| c.to_lowercase()).collect();

                    let mut result: Vec<Completion> = right_cols
                        .iter()
                        .filter(|c| left_lower.contains(&c.to_lowercase()))
                        .map(|c| Completion { value: c.clone(), kind: CompletionKind::Column })
                        .collect();

                    result.extend(
                        right_cols
                            .iter()
                            .filter(|c| !left_lower.contains(&c.to_lowercase()))
                            .map(|c| Completion { value: c.clone(), kind: CompletionKind::Column }),
                    );

                    result.extend(
                        left_cols.iter().map(|c| Completion { value: c.clone(), kind: CompletionKind::Column }),
                    );

                    result
                } else {
                    let table_refs = self.extract_table_refs(upper_query, alias_map);
                    if table_refs.is_empty() {
                        self.keyword_completions("")
                    } else {
                        self.column_completions(&table_refs, "")
                    }
                }
            }
            "SELECT" => {
                let table_refs = self.extract_table_refs(upper_query, alias_map);
                if table_refs.is_empty() {
                    self.table_completions("")
                } else {
                    self.qualified_column_completions(&table_refs, alias_map, "")
                }
            }
            "WHERE" | "SET" | "BY" => {
                let table_refs = self.extract_table_refs(upper_query, alias_map);
                if table_refs.is_empty() {
                    let all_tables: Vec<String> = self.schema.tables().to_vec();
                    if all_tables.is_empty() {
                        self.keyword_completions("")
                    } else {
                        self.qualified_column_completions(&all_tables, alias_map, "")
                    }
                } else {
                    self.qualified_column_completions(&table_refs, alias_map, "")
                }
            }
            _ => self.keyword_completions(""),
        }
    }

    fn filter_and_sort(
        &self,
        candidates: Vec<Completion>,
        effective_trigger: &str,
        current_word: &str,
    ) -> Vec<Completion> {
        let is_trigger = TABLE_TRIGGERS.contains(&current_word)
            || COLUMN_TRIGGERS.contains(&current_word);
        let prefix_upper = if is_trigger {
            String::new()
        } else {
            current_word.to_uppercase()
        };

        if effective_trigger == "ON" {
            let mut seen = std::collections::HashSet::new();
            return candidates
                .into_iter()
                .filter(|c| c.value.to_uppercase().starts_with(&prefix_upper))
                .filter(|c| seen.insert(c.value.clone()))
                .collect();
        }

        let mut results: Vec<Completion> = candidates
            .into_iter()
            .filter(|c| {
                let upper = c.value.to_uppercase();
                upper.starts_with(&prefix_upper)
                    || (!prefix_upper.is_empty()
                        && c.value.contains('.')
                        && upper.split('.').next_back().is_some_and(|p| p.starts_with(&prefix_upper)))
            })
            .collect();

        results.sort_by(|a, b| match (&a.kind, &b.kind) {
            (CompletionKind::Keyword, CompletionKind::Keyword) => a.value.cmp(&b.value),
            (CompletionKind::Table, CompletionKind::Table) if !self.table_freq.is_empty() => {
                let ca = self.table_freq.get(&a.value).copied().unwrap_or(0);
                let cb = self.table_freq.get(&b.value).copied().unwrap_or(0);
                cb.cmp(&ca).then_with(|| a.value.len().cmp(&b.value.len()).then_with(|| a.value.cmp(&b.value)))
            }
            (CompletionKind::Column, CompletionKind::Column)
                if !self.column_freq.is_empty() || !self.table_freq.is_empty() =>
            {
                let ta = a.value.find('.').map(|i| self.table_freq.get(&a.value[..i]).copied().unwrap_or(0)).unwrap_or(0);
                let tb = b.value.find('.').map(|i| self.table_freq.get(&b.value[..i]).copied().unwrap_or(0)).unwrap_or(0);
                let ca = self.column_freq.get(a.value.split('.').next_back().unwrap_or(&a.value)).copied().unwrap_or(0);
                let cb = self.column_freq.get(b.value.split('.').next_back().unwrap_or(&b.value)).copied().unwrap_or(0);
                tb.cmp(&ta)
                    .then_with(|| cb.cmp(&ca))
                    .then_with(|| a.value.len().cmp(&b.value.len()))
                    .then_with(|| a.value.cmp(&b.value))
            }
            _ => a.value.len().cmp(&b.value.len()).then_with(|| a.value.cmp(&b.value)),
        });
        results.dedup_by(|a, b| a.value == b.value && a.kind == b.kind);
        results
    }

    fn extract_table_refs(&self, upper_query: &str, alias_map: &AliasMap) -> Vec<String> {
        let tokens: Vec<&str> = upper_query.split_whitespace().collect();
        let trigger = ["FROM", "JOIN", "UPDATE"];
        let mut refs: Vec<String> = tokens
            .windows(2)
            .filter_map(|w| {
                if !trigger.contains(&w[0]) { return None; }
                let raw = w[1].to_lowercase();
                Some(raw.rsplit('.').next().unwrap_or(&raw).to_string())
            })
            .collect();
        for real_table in alias_map.real_tables() {
            if !refs.iter().any(|r| r == real_table) {
                refs.push(real_table.to_string());
            }
        }
        refs
    }
}

fn resolve_trigger_and_word(input: &str) -> (String, String) {
    let upper = input.to_uppercase();
    let tokens: Vec<&str> = upper.split_whitespace().collect();

    let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
        String::new()
    } else {
        tokens.last().copied().unwrap_or("").to_string()
    };

    let effective_trigger = if TABLE_TRIGGERS.contains(&current_word.as_str())
        || COLUMN_TRIGGERS.contains(&current_word.as_str())
    {
        current_word.clone()
    } else if input.ends_with(char::is_whitespace) {
        tokens.last().copied().unwrap_or("").to_string()
    } else if tokens.len() >= 2 {
        tokens[tokens.len() - 2].to_string()
    } else {
        String::new()
    };

    (effective_trigger, current_word)
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

    fn schema_with(tables: &[&str], columns: &[(&str, &[&str])]) -> SchemaService {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for &table in tables {
            col_map.entry(table.to_string()).or_default();
        }
        for (table, cols) in columns {
            col_map.insert(
                table.to_string(),
                cols.iter().map(|c| c.to_string()).collect(),
            );
        }
        let mut schema = SchemaService::new(None);
        schema.load(&TestDb { columns: col_map }, "test").unwrap();
        schema
    }

    fn service_with(tables: &[&str], columns: &[(&str, &[&str])]) -> QueryCompletionService {
        QueryCompletionService::new(schema_with(tables, columns), HashMap::new(), HashMap::new())
    }

    #[test]
    fn completions_returns_empty_when_no_prefix_match() {
        let svc = service_with(&["users", "orders"], &[]);
        let input = "SELECT * FROM zzz";
        let results = svc.completions(input, input.len());
        assert!(results.is_empty(), "prefix that matches nothing should yield no completions");
    }

    #[test]
    fn keyword_completions_prefix_filter() {
        let svc = service_with(&[], &[]);
        let results = svc.keyword_completions("SEL");
        assert!(results.iter().any(|c| c.value == "SELECT"));
        assert!(results.iter().all(|c| c.value.starts_with("SEL")));
        assert!(results.iter().all(|c| matches!(c.kind, CompletionKind::Keyword)));
    }

    #[test]
    fn keyword_completions_case_insensitive() {
        let svc = service_with(&[], &[]);
        let results = svc.keyword_completions("sel");
        assert!(results.iter().any(|c| c.value == "SELECT"), "lowercase prefix should still match");
    }

    #[test]
    fn table_completions_prefix_filter() {
        let svc = service_with(&["users", "orders", "user_roles"], &[]);
        let results = svc.table_completions("user");
        let names: Vec<&str> = results.iter().map(|c| c.value.as_str()).collect();
        assert!(names.contains(&"users"), "got: {names:?}");
        assert!(names.contains(&"user_roles"), "got: {names:?}");
        assert!(!names.contains(&"orders"), "orders should be filtered, got: {names:?}");
        assert!(results.iter().all(|c| matches!(c.kind, CompletionKind::Table)));
    }

    #[test]
    fn column_completions_scoped_to_table_refs() {
        let svc = service_with(
            &["users", "orders"],
            &[("users", &["id", "email"]), ("orders", &["id", "status"])],
        );
        let results = svc.column_completions(&["users".to_string()], "");
        let names: Vec<&str> = results.iter().map(|c| c.value.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"email"));
        assert!(!names.contains(&"status"), "orders column should not appear");
    }

    #[test]
    fn qualified_completions_bare_table_and_alias_variants() {
        let svc = service_with(&["users"], &[("users", &["id", "email"])]);
        // alias_map from "SELECT  FROM users u" knows u → users
        let input = "SELECT  FROM users u";
        let results = svc.completions(input, 7);
        let values: Vec<&str> = results.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"id"),       "bare column id expected");
        assert!(values.contains(&"users.id"), "table-qualified users.id expected");
        assert!(values.contains(&"u.id"),     "alias-qualified u.id expected");
        assert!(values.contains(&"email"),       "bare column email expected");
        assert!(values.contains(&"users.email"), "table-qualified users.email expected");
        assert!(values.contains(&"u.email"),     "alias-qualified u.email expected");
    }

    #[test]
    fn qualified_completions_no_alias_shows_bare_and_table() {
        let svc = service_with(&["users"], &[("users", &["id"])]);
        let input = "SELECT  FROM users";
        let results = svc.completions(input, 7);
        let values: Vec<&str> = results.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"id"),       "bare column expected");
        assert!(values.contains(&"users.id"), "table-qualified expected");
        assert!(!values.iter().any(|v| v.starts_with("users.") && v.contains(".id") && v.len() > "users.id".len()),
            "no spurious qualified names expected");
    }

    #[test]
    fn qualified_completions_dot_path_unchanged() {
        // When user already typed "users.", should still return bare column names only (span replaces suffix)
        let svc = service_with(&["users"], &[("users", &["id", "email"])]);
        let input = "SELECT users. FROM users";
        let results = svc.completions(input, 13);
        let values: Vec<&str> = results.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"id"),    "bare id expected after users.");
        assert!(values.contains(&"email"), "bare email expected after users.");
        assert!(!values.iter().any(|v| v.starts_with("users.")), "no re-qualified users.id expected");
    }

    #[test]
    fn completions_suggests_tables_after_from() {
        let svc = service_with(&["users", "orders"], &[]);
        let input = "SELECT * FROM ";
        let results = svc.completions(input, input.len());
        assert!(results.iter().any(|c| c.value == "users"));
        assert!(results.iter().any(|c| c.value == "orders"));
    }

    #[test]
    fn completions_suggests_tables_after_select_without_from() {
        let svc = service_with(&["users", "orders"], &[("users", &["id", "email"])]);
        let input = "SELECT ";
        let results = svc.completions(input, input.len());
        assert!(results.iter().any(|c| c.value == "users" && matches!(c.kind, CompletionKind::Table)), "expected users table in {:?}", results.iter().map(|c| &c.value).collect::<Vec<_>>());
        assert!(results.iter().any(|c| c.value == "orders" && matches!(c.kind, CompletionKind::Table)));
        assert!(!results.iter().any(|c| matches!(c.kind, CompletionKind::Column)), "columns should not appear without FROM");
    }

    #[test]
    fn completions_suggests_columns_after_select_with_from() {
        let svc = service_with(&["users"], &[("users", &["id", "email"])]);
        let input = "SELECT  FROM users";
        let results = svc.completions(input, 7);
        assert!(results.iter().any(|c| c.value == "id"), "expected id in {:?}", results.iter().map(|c| &c.value).collect::<Vec<_>>());
        assert!(results.iter().any(|c| c.value == "email"));
    }

    #[test]
    fn completions_suggests_keywords_when_no_context() {
        let svc = service_with(&[], &[]);
        let results = svc.completions("SEL", 3);
        assert!(results.iter().any(|c| c.value == "SELECT" && matches!(c.kind, CompletionKind::Keyword)));
    }
}
