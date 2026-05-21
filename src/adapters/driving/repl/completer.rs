use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use crate::core::services::schema::service::SchemaService;

const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "ON", "AND", "OR", "NOT", "IN", "IS", "NULL", "AS", "DISTINCT",
    "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "INSERT", "INTO",
    "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "DROP", "ALTER",
    "BEGIN", "COMMIT", "ROLLBACK",
];

pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}

pub struct SqlCompleter {
    schema: SchemaService,
}

impl SqlCompleter {
    pub fn new(schema: SchemaService) -> Self {
        Self { schema }
    }

    pub fn schema(&self) -> &SchemaService {
        &self.schema
    }

    pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
        let input = &line[..pos];
        let upper = input.to_uppercase();
        let tokens: Vec<&str> = upper.split_whitespace().collect();

        let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
            ""
        } else {
            tokens.last().copied().unwrap_or("")
        };

        let table_triggers = ["FROM", "JOIN", "INTO", "UPDATE"];
        let col_triggers = ["SELECT", "WHERE", "ON", "SET", "BY"];

        let effective_trigger = if table_triggers.contains(&current_word) || col_triggers.contains(&current_word) {
            current_word
        } else if input.ends_with(char::is_whitespace) {
            tokens.last().copied().unwrap_or("")
        } else if tokens.len() >= 2 {
            tokens[tokens.len() - 2]
        } else {
            ""
        };

        let full_upper = line.to_uppercase();

        let candidates: Vec<(String, CompletionKind)> = match effective_trigger {
            "FROM" | "JOIN" | "INTO" | "UPDATE" => self
                .schema
                .tables()
                .iter()
                .map(|t| (t.to_string(), CompletionKind::Table))
                .collect(),
            "SELECT" | "WHERE" | "ON" | "SET" | "BY" => {
                let table_refs = self.extract_table_refs(&full_upper);
                if table_refs.is_empty() {
                    SQL_KEYWORDS
                        .iter()
                        .map(|k| (k.to_string(), CompletionKind::Keyword))
                        .collect()
                } else {
                    table_refs
                        .iter()
                        .flat_map(|t| {
                            let t_lower = t.to_lowercase();
                            self.schema
                                .columns_for(&t_lower)
                                .iter()
                                .map(|c| (c.to_string(), CompletionKind::Column))
                        })
                        .collect()
                }
            }
            _ => SQL_KEYWORDS
                .iter()
                .map(|k| (k.to_string(), CompletionKind::Keyword))
                .collect(),
        };

        let is_trigger = table_triggers.contains(&current_word) || col_triggers.contains(&current_word);
        let prefix_upper = if is_trigger { "".to_string() } else { current_word.to_uppercase() };

        let mut results: Vec<(String, CompletionKind)> = candidates
            .into_iter()
            .filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))
            .collect();

        results.sort_by(|a, b| a.0.cmp(&b.0));
        results.dedup_by_key(|item| item.0.clone());
        results
    }

    fn extract_table_refs<'a>(&self, upper_query: &'a str) -> Vec<&'a str> {
        let tokens: Vec<&str> = upper_query.split_whitespace().collect();
        let mut tables = vec![];
        let trigger = ["FROM", "JOIN", "UPDATE"];
        for window in tokens.windows(2) {
            if trigger.contains(&window[0]) {
                tables.push(window[1]);
            }
        }
        tables
    }
}

impl Completer for SqlCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let word_start = line[..pos]
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);

        let candidates = self.complete_input(line, pos);
        let pairs = candidates
            .into_iter()
            .map(|(c, _)| Pair {
                display: c.clone(),
                replacement: c,
            })
            .collect();

        Ok((word_start, pairs))
    }
}

impl Hinter for SqlCompleter {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for SqlCompleter {}
impl Validator for SqlCompleter {}
impl Helper for SqlCompleter {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn schema_with(tables: &[&str], columns: &[(&str, &[&str])]) -> SchemaService {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table, cols) in columns {
            col_map.insert(
                table.to_string(),
                cols.iter().map(|c| c.to_string()).collect(),
            );
        }
        SchemaService {
            tables: tables.iter().map(|t| t.to_string()).collect(),
            columns: col_map,
        }
    }

    #[test]
    fn suggests_keywords_at_start_of_input() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SEL", 3);
        assert!(
            results.iter().any(|(r, _)| r == "SELECT"),
            "expected SELECT in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn suggests_table_names_after_from() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(results.iter().any(|(r, _)| r == "users"));
        assert!(results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn suggests_table_names_after_join() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM users JOIN ", 24);
        assert!(results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn suggests_columns_after_select_when_table_known() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(results.iter().any(|(r, _)| r == "id"), "expected id in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>());
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn filters_by_current_word_prefix() {
        let schema = schema_with(&["users", "user_sessions"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM user", 18);
        assert!(results.iter().any(|(r, _)| r == "users"));
        assert!(results.iter().any(|(r, _)| r == "user_sessions"));
        assert!(!results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn no_duplicate_suggestions() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 14);
        let names: Vec<&str> = results.iter().map(|(r, _)| r.as_str()).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "duplicates found: {:?}", names);
    }

    #[test]
    fn tags_keywords_with_keyword_kind() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SEL", 3);
        assert!(
            results.iter().any(|(r, k)| r == "SELECT" && matches!(k, CompletionKind::Keyword)),
            "expected SELECT [keyword] in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tags_tables_with_table_kind() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(
            results.iter().any(|(r, k)| r == "users" && matches!(k, CompletionKind::Table)),
            "expected users [table]"
        );
    }

    #[test]
    fn tags_columns_with_column_kind() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id [column]"
        );
    }
}
