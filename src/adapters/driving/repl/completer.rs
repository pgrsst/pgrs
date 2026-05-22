use nu_ansi_term::{Color, Style};
use reedline::{Completer, Highlighter, Span, StyledText, Suggestion};

use crate::core::services::schema::service::SchemaService;

const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "ON", "AND", "OR", "NOT", "IN", "IS", "NULL", "AS", "DISTINCT",
    "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "INSERT", "INTO",
    "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "DROP", "ALTER",
    "BEGIN", "COMMIT", "ROLLBACK",
];

#[derive(Debug, Clone, PartialEq)]
pub enum CompletionKind {
    Keyword,
    Table,
    Column,
}

impl CompletionKind {
    fn label(&self) -> &'static str {
        match self {
            CompletionKind::Keyword => "[keyword]",
            CompletionKind::Table   => "[table]",
            CompletionKind::Column  => "[column]",
        }
    }

    fn style(&self) -> Style {
        match self {
            CompletionKind::Keyword => Style::new().fg(Color::Cyan).bold(),
            CompletionKind::Table   => Style::new().fg(Color::Yellow).bold(),
            CompletionKind::Column  => Style::new().fg(Color::Green),
        }
    }
}

#[cfg(test)]
pub fn highlight_sql(line: &str, tables: &[String], columns: &[String]) -> String {
    let mut out = String::with_capacity(line.len() * 2);
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Line comment: --
        if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            let span: String = chars[start..i].iter().collect();
            out.push_str(&format!("\x1b[2m{}\x1b[0m", span));
        }
        // String literal: '...' with '' escape for embedded single quote
        else if chars[i] == '\'' {
            let start = i;
            i += 1;
            loop {
                if i >= len {
                    break; // unterminated — highlight what we have
                }
                if chars[i] == '\'' {
                    i += 1; // consume the quote
                    if i < len && chars[i] == '\'' {
                        i += 1; // '' escape: skip second quote and continue
                    } else {
                        break; // closing quote
                    }
                } else {
                    i += 1;
                }
            }
            let span: String = chars[start..i].iter().collect();
            out.push_str(&format!("\x1b[33m{}\x1b[0m", span));
        }
        // Number: digit
        else if chars[i].is_ascii_digit() {
            let start = i;
            let mut has_dot = false;
            while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                if chars[i] == '.' { has_dot = true; }
                i += 1;
            }
            let span: String = chars[start..i].iter().collect();
            out.push_str(&format!("\x1b[35m{}\x1b[0m", span));
        }
        // Word: letter or underscore
        else if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            if SQL_KEYWORDS.contains(&upper.as_str()) {
                out.push_str(&format!("\x1b[1;36m{}\x1b[0m", word));
            } else if tables.iter().any(|t| t.eq_ignore_ascii_case(&word)) {
                out.push_str(&format!("\x1b[1;33m{}\x1b[0m", word));
            } else if columns.iter().any(|c| c.eq_ignore_ascii_case(&word)) {
                out.push_str(&format!("\x1b[32m{}\x1b[0m", word));
            } else {
                out.push_str(&word);
            }
        }
        // Everything else
        else {
            out.push(chars[i]);
            i += 1;
        }
    }

    out
}

fn word_start(line: &str, pos: usize) -> usize {
    let input = &line[..pos];
    let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let word = &input[last_ws..];
    if let Some(dot_pos) = word.rfind('.') {
        last_ws + dot_pos + 1
    } else {
        last_ws
    }
}

pub struct SqlCompleter {
    schema: SchemaService,
}

impl SqlCompleter {
    pub fn new(schema: SchemaService) -> Self {
        Self { schema }
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
        results.dedup_by(|a, b| a.0 == b.0);
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
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let start = word_start(line, pos);
        self.complete_input(line, pos)
            .into_iter()
            .map(|(value, kind)| Suggestion {
                value,
                display_override: None,
                description: Some(kind.label().to_string()),
                style: Some(kind.style()),
                span: Span::new(start, pos),
                extra: None,
                append_whitespace: false,
                match_indices: None,
            })
            .collect()
    }
}

pub struct SqlHighlighter {
    tables: Vec<String>,
    columns: Vec<String>,
}

impl SqlHighlighter {
    pub fn new(schema: SchemaService) -> Self {
        let tables = schema.tables().to_vec();
        let columns: Vec<String> = schema
            .tables()
            .iter()
            .flat_map(|t| schema.columns_for(t).iter().cloned())
            .collect();
        Self { tables, columns }
    }
}

impl Highlighter for SqlHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
                let start = i;
                while i < len && chars[i] != '\n' { i += 1; }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().dimmed(), span));
            } else if chars[i] == '\'' {
                let start = i;
                i += 1;
                loop {
                    if i >= len { break; }
                    if chars[i] == '\'' {
                        i += 1;
                        if i < len && chars[i] == '\'' { i += 1; } else { break; }
                    } else { i += 1; }
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().fg(Color::Yellow), span));
            } else if chars[i].is_ascii_digit() {
                let start = i;
                let mut has_dot = false;
                while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                    if chars[i] == '.' { has_dot = true; }
                    i += 1;
                }
                let span: String = chars[start..i].iter().collect();
                styled.push((Style::new().fg(Color::Magenta), span));
            } else if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
                let word: String = chars[start..i].iter().collect();
                let upper = word.to_uppercase();
                let style = if SQL_KEYWORDS.contains(&upper.as_str()) {
                    Style::new().fg(Color::Cyan).bold()
                } else if self.tables.iter().any(|t| t.eq_ignore_ascii_case(&word)) {
                    Style::new().fg(Color::Yellow).bold()
                } else if self.columns.iter().any(|c| c.eq_ignore_ascii_case(&word)) {
                    Style::new().fg(Color::Green)
                } else {
                    Style::new()
                };
                styled.push((style, word));
            } else {
                styled.push((Style::new(), chars[i].to_string()));
                i += 1;
            }
        }

        styled
    }
}

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

    #[test]
    fn highlight_keyword_bold_cyan() {
        let result = highlight_sql("SELECT", &[], &[]);
        assert!(result.contains("\x1b[1;36m"), "expected bold cyan escape");
        assert!(result.contains("SELECT"));
        assert!(result.contains("\x1b[0m"), "expected reset");
    }

    #[test]
    fn highlight_string_literal_yellow() {
        let result = highlight_sql("'hello'", &[], &[]);
        assert!(result.contains("\x1b[33m"), "expected yellow escape");
        assert!(result.contains("'hello'"));
    }

    #[test]
    fn highlight_number_magenta() {
        let result = highlight_sql("42", &[], &[]);
        assert!(result.contains("\x1b[35m"), "expected magenta escape");
        assert!(result.contains("42"));
    }

    #[test]
    fn highlight_comment_dim() {
        let result = highlight_sql("-- comment", &[], &[]);
        assert!(result.contains("\x1b[2m"), "expected dim escape");
        assert!(result.contains("-- comment"));
    }

    #[test]
    fn highlight_table_name_bold_yellow() {
        let tables = vec!["users".to_string()];
        let result = highlight_sql("users", &tables, &[]);
        assert!(result.contains("\x1b[1;33m"), "expected bold yellow for table");
    }

    #[test]
    fn highlight_column_name_green() {
        let columns = vec!["email".to_string()];
        let result = highlight_sql("email", &[], &columns);
        assert!(result.contains("\x1b[32m"), "expected green for column");
    }

    #[test]
    fn highlight_plain_word_no_escape() {
        let result = highlight_sql("foo", &[], &[]);
        assert!(!result.contains("\x1b["), "expected no escape for unknown word");
    }

    #[test]
    fn highlight_number_trailing_dot_not_consumed() {
        // "10." — dot is punctuation, not part of the number
        let result = highlight_sql("10.", &[], &[]);
        assert!(result.contains("\x1b[35m10\x1b[0m"), "10 should be magenta");
        assert!(result.ends_with('.'), "trailing dot should be plain");
    }

    #[test]
    fn highlight_number_decimal_consumed() {
        // "3.14" — dot followed by digit is part of the number
        let result = highlight_sql("3.14", &[], &[]);
        assert!(result.contains("\x1b[35m3.14\x1b[0m"), "3.14 should be one magenta span");
    }

    #[test]
    fn highlight_string_with_escaped_quote() {
        let result = highlight_sql("'O''Brien'", &[], &[]);
        // entire 'O''Brien' should be one yellow span
        assert_eq!(result, "\x1b[33m'O''Brien'\x1b[0m");
    }

    #[test]
    fn highlight_mixed_query() {
        let tables = vec!["users".to_string()];
        let result = highlight_sql("SELECT * FROM users WHERE id = 1", &tables, &[]);
        assert!(result.contains("\x1b[1;36m"), "SELECT should be bold cyan");
        assert!(result.contains("\x1b[1;33m"), "users should be bold yellow");
        assert!(result.contains("\x1b[35m"), "1 should be magenta");
    }
}
