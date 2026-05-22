use nu_ansi_term::{Color, Style};
use reedline::{Completer, Highlighter, Span, StyledText, Suggestion};
use std::collections::HashMap;

use crate::core::services::schema::service::SchemaService;

struct AliasMap {
    map: HashMap<String, Option<String>>,
}

impl AliasMap {
    fn resolve(&self, name: &str) -> Option<&str> {
        self.map.get(name).and_then(|v| v.as_deref())
    }
}

#[derive(Debug)]
enum AliasState {
    Idle,
    ExpectTable,
    ExpectAlias { candidate: String },
    ExpectAliasName { candidate: String },
    PostAlias,
    InSubquery { depth: usize },
    ExpectSubqueryAlias,
    ExpectSubqueryAliasName,
}

fn build_alias_map(line: &str) -> AliasMap {
    // NOTE: schema-qualified table names (e.g. FROM public.users u) are not handled —
    // the dot is parsed as Other('.') which disrupts the alias extraction for that table.
    let mut map: HashMap<String, Option<String>> = HashMap::new();
    let mut state = AliasState::Idle;

    for token in tokenize(line) {
        if let SqlToken::Other(c) = token {
            if c.is_whitespace() {
                continue;
            }
            state = match (state, SqlToken::Other(c)) {
                (AliasState::ExpectTable, SqlToken::Other('(')) => {
                    AliasState::InSubquery { depth: 1 }
                }
                (AliasState::ExpectAlias { .. }, SqlToken::Other(',')) => AliasState::ExpectTable,
                (AliasState::PostAlias, SqlToken::Other(',')) => AliasState::ExpectTable,
                (AliasState::InSubquery { depth }, SqlToken::Other('(')) => {
                    AliasState::InSubquery { depth: depth + 1 }
                }
                (AliasState::InSubquery { depth }, SqlToken::Other(')')) => {
                    if depth == 1 {
                        AliasState::ExpectSubqueryAlias
                    } else {
                        AliasState::InSubquery { depth: depth - 1 }
                    }
                }
                (AliasState::InSubquery { depth }, _) => AliasState::InSubquery { depth },
                (s, _) => s,
            };
            continue;
        }
        state = match (state, token) {
            (AliasState::Idle, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::ExpectTable, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                AliasState::ExpectAlias { candidate: w.to_lowercase() }
            }
            (AliasState::ExpectTable, _) => AliasState::Idle,
            (AliasState::ExpectAlias { candidate }, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                AliasState::ExpectAliasName { candidate }
            }
            (AliasState::ExpectAlias { candidate }, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                map.insert(w.to_lowercase(), Some(candidate));
                AliasState::PostAlias
            }
            (AliasState::ExpectAlias { .. }, _) => AliasState::Idle,
            (AliasState::ExpectAliasName { candidate }, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), Some(candidate));
                AliasState::PostAlias
            }
            (AliasState::ExpectAliasName { .. }, _) => AliasState::Idle,
            (AliasState::PostAlias, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::PostAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                AliasState::ExpectSubqueryAliasName
            }
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                map.insert(w.to_lowercase(), None);
                AliasState::PostAlias
            }
            (AliasState::ExpectSubqueryAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAliasName, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), None);
                AliasState::PostAlias
            }
            (AliasState::ExpectSubqueryAliasName, _) => AliasState::Idle,
            (s, _) => s,
        };
    }

    AliasMap { map }
}

struct JoinContext {
    right_table: String,
    left_tables: Vec<String>,
}

fn extract_join_context(upper_query: &str, alias_map: &AliasMap) -> Option<JoinContext> {
    let tokens: Vec<&str> = upper_query.split_whitespace().collect();

    let last_join_pos = tokens.iter().rposition(|&t| t == "JOIN")?;

    let right_raw = tokens.get(last_join_pos + 1)?.to_lowercase();
    let right_table = alias_map
        .resolve(&right_raw)
        .map(|s| s.to_string())
        .unwrap_or(right_raw);

    let left_tables: Vec<String> = tokens
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| {
            if (w[0] == "FROM" || w[0] == "JOIN" || w[0] == "UPDATE") && i != last_join_pos {
                Some(w[1].to_lowercase())
            } else {
                None
            }
        })
        .map(|raw| {
            alias_map
                .resolve(&raw)
                .map(|s| s.to_string())
                .unwrap_or(raw)
        })
        .filter(|t| t != &right_table)
        .collect();

    Some(JoinContext { right_table, left_tables })
}

const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "ON", "AND", "OR", "NOT", "IN", "IS", "NULL", "AS", "DISTINCT",
    "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "INSERT", "INTO",
    "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "DROP", "ALTER",
    "BEGIN", "COMMIT", "ROLLBACK",
];

const TABLE_TRIGGERS: &[&str] = &["FROM", "JOIN", "INTO", "UPDATE"];
const COLUMN_TRIGGERS: &[&str] = &["SELECT", "WHERE", "ON", "SET", "BY"];

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

#[derive(Debug)]
enum SqlToken {
    Comment(String),
    StringLiteral(String),
    Number(String),
    Word(String),
    Other(char),
}

fn tokenize(input: &str) -> Vec<SqlToken> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < len {
        if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
            let start = i;
            while i < len && chars[i] != '\n' { i += 1; }
            tokens.push(SqlToken::Comment(chars[start..i].iter().collect()));
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
            tokens.push(SqlToken::StringLiteral(chars[start..i].iter().collect()));
        } else if chars[i].is_ascii_digit() {
            let start = i;
            let mut has_dot = false;
            while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                if chars[i] == '.' { has_dot = true; }
                i += 1;
            }
            tokens.push(SqlToken::Number(chars[start..i].iter().collect()));
        } else if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            tokens.push(SqlToken::Word(chars[start..i].iter().collect()));
        } else {
            tokens.push(SqlToken::Other(chars[i]));
            i += 1;
        }
    }

    tokens
}

fn classify_word(word: &str, tables: &[String], columns: &[String]) -> Option<CompletionKind> {
    let upper = word.to_uppercase();
    if SQL_KEYWORDS.contains(&upper.as_str()) {
        Some(CompletionKind::Keyword)
    } else if tables.iter().any(|t| t.eq_ignore_ascii_case(word)) {
        Some(CompletionKind::Table)
    } else if columns.iter().any(|c| c.eq_ignore_ascii_case(word)) {
        Some(CompletionKind::Column)
    } else {
        None
    }
}

#[cfg(test)]
fn highlight_sql(line: &str, tables: &[String], columns: &[String]) -> String {
    let mut out = String::with_capacity(line.len() * 2);
    for token in tokenize(line) {
        match token {
            SqlToken::Comment(s)       => out.push_str(&format!("\x1b[2m{s}\x1b[0m")),
            SqlToken::StringLiteral(s) => out.push_str(&format!("\x1b[33m{s}\x1b[0m")),
            SqlToken::Number(s)        => out.push_str(&format!("\x1b[35m{s}\x1b[0m")),
            SqlToken::Word(s) => match classify_word(&s, tables, columns) {
                Some(CompletionKind::Keyword) => out.push_str(&format!("\x1b[1;36m{s}\x1b[0m")),
                Some(CompletionKind::Table)   => out.push_str(&format!("\x1b[1;33m{s}\x1b[0m")),
                Some(CompletionKind::Column)  => out.push_str(&format!("\x1b[32m{s}\x1b[0m")),
                None                          => out.push_str(&s),
            },
            SqlToken::Other(c) => out.push(c),
        }
    }
    out
}

fn fuzzy_match(candidate: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut chars = candidate.chars();
    // `chars` is a shared mutable iterator: each call to `chars.any(...)` advances
    // its position, so the next outer `.all()` iteration resumes from where the
    // previous one left off.  This enforces that query characters are matched as a
    // subsequence (in order) rather than as an unordered set.
    query
        .chars()
        .all(|q| chars.any(|c| c.eq_ignore_ascii_case(&q)))
}

pub(crate) fn common_prefix(candidates: &[(String, CompletionKind)]) -> String {
    if candidates.is_empty() {
        return String::new();
    }
    let first = &candidates[0].0;
    // Count how many leading chars of `first` are a case-insensitive prefix of every other candidate.
    let prefix_len = candidates[1..].iter().fold(first.chars().count(), |acc, (c, _)| {
        first
            .chars()
            .zip(c.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count()
            .min(acc)
    });
    first.chars().take(prefix_len).collect()
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
        let alias_map = build_alias_map(line);
        let input = &line[..pos];

        // Qualified name: "table.col_prefix" or "schema.table.col_prefix"
        let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
        let token = &input[last_ws..];
        if let Some(dot_pos) = token.rfind('.') {
            let table_name = token[..dot_pos]
                .split('.')
                .next_back()
                .unwrap_or(&token[..dot_pos])
                .to_lowercase();
            let col_prefix = token[dot_pos + 1..].to_uppercase();
            return self.complete_qualified(&table_name, &col_prefix, &alias_map);
        }

        let upper = input.to_uppercase();
        let tokens: Vec<&str> = upper.split_whitespace().collect();

        let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
            ""
        } else {
            tokens.last().copied().unwrap_or("")
        };

        let effective_trigger = if TABLE_TRIGGERS.contains(&current_word) || COLUMN_TRIGGERS.contains(&current_word) {
            current_word
        } else if input.ends_with(char::is_whitespace) {
            tokens.last().copied().unwrap_or("")
        } else if tokens.len() >= 2 {
            tokens[tokens.len() - 2]
        } else {
            ""
        };

        let full_upper = line.to_uppercase();
        let candidates = self.candidates_for_trigger(effective_trigger, &full_upper, &alias_map);

        let is_trigger = TABLE_TRIGGERS.contains(&current_word) || COLUMN_TRIGGERS.contains(&current_word);
        let prefix_upper = if is_trigger { String::new() } else { current_word.to_uppercase() };

        // For JOIN ON completions the candidates are already ordered intentionally
        // (shared columns first).  Sorting alphabetically would destroy that priority,
        // so we skip the sort and use a seen-set for deduplication instead.
        if effective_trigger == "ON" {
            let mut seen = std::collections::HashSet::new();
            return candidates
                .into_iter()
                .filter(|(c, _)| fuzzy_match(c, &prefix_upper))
                .filter(|(c, _)| seen.insert(c.clone()))
                .collect();
        }

        let mut results: Vec<(String, CompletionKind)> = candidates
            .into_iter()
            .filter(|(c, _)| fuzzy_match(c, &prefix_upper))
            .collect();

        results.sort_by(|a, b| a.0.cmp(&b.0));
        results.dedup_by(|a, b| a.0 == b.0);
        results
    }

    fn complete_qualified(&self, table_name: &str, col_prefix: &str, alias_map: &AliasMap) -> Vec<(String, CompletionKind)> {
        let resolved = alias_map.resolve(table_name).unwrap_or(table_name);
        let cols = self.schema.columns_for(resolved);
        if !cols.is_empty() {
            cols.iter()
                .filter(|c| fuzzy_match(c, col_prefix))
                .map(|c| (c.to_string(), CompletionKind::Column))
                .collect()
        } else {
            // Table not found: fallback to all columns
            self.schema
                .tables()
                .iter()
                .flat_map(|t| self.schema.columns_for(t).iter().cloned())
                .filter(|c| fuzzy_match(c, col_prefix))
                .map(|c| (c, CompletionKind::Column))
                .collect()
        }
    }

    fn candidates_for_trigger(&self, trigger: &str, upper_query: &str, alias_map: &AliasMap) -> Vec<(String, CompletionKind)> {
        match trigger {
            "FROM" | "JOIN" | "INTO" | "UPDATE" => self
                .schema
                .tables()
                .iter()
                .map(|t| (t.to_string(), CompletionKind::Table))
                .collect(),
            "ON" => {
                if let Some(ctx) = extract_join_context(upper_query, alias_map) {
                    let right_cols: Vec<String> = self.schema.columns_for(&ctx.right_table).to_vec();
                    let left_cols: Vec<String> = ctx
                        .left_tables
                        .iter()
                        .flat_map(|t| self.schema.columns_for(t).iter().cloned())
                        .collect();

                    // Build a lowercase set for O(1) shared-column lookup
                    let left_lower: std::collections::HashSet<String> =
                        left_cols.iter().map(|c| c.to_lowercase()).collect();

                    // Shared columns (likely FK keys) first
                    let mut result: Vec<(String, CompletionKind)> = right_cols
                        .iter()
                        .filter(|c| left_lower.contains(&c.to_lowercase()))
                        .map(|c| (c.clone(), CompletionKind::Column))
                        .collect();

                    // Remaining right-table-only columns
                    result.extend(
                        right_cols
                            .iter()
                            .filter(|c| !left_lower.contains(&c.to_lowercase()))
                            .map(|c| (c.clone(), CompletionKind::Column)),
                    );

                    // Left table columns
                    result.extend(left_cols.iter().map(|c| (c.clone(), CompletionKind::Column)));

                    result
                } else {
                    let table_refs = self.extract_table_refs(upper_query, alias_map);
                    if table_refs.is_empty() {
                        SQL_KEYWORDS
                            .iter()
                            .map(|k| (k.to_string(), CompletionKind::Keyword))
                            .collect()
                    } else {
                        table_refs
                            .iter()
                            .flat_map(|t| {
                                self.schema
                                    .columns_for(t)
                                    .iter()
                                    .map(|c| (c.to_string(), CompletionKind::Column))
                            })
                            .collect()
                    }
                }
            }
            "SELECT" | "WHERE" | "SET" | "BY" => {
                let table_refs = self.extract_table_refs(upper_query, alias_map);
                if table_refs.is_empty() {
                    SQL_KEYWORDS
                        .iter()
                        .map(|k| (k.to_string(), CompletionKind::Keyword))
                        .collect()
                } else {
                    table_refs
                        .iter()
                        .flat_map(|t| {
                            self.schema
                                .columns_for(t)
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
        }
    }

    fn extract_table_refs(&self, upper_query: &str, alias_map: &AliasMap) -> Vec<String> {
        let tokens: Vec<&str> = upper_query.split_whitespace().collect();
        let trigger = ["FROM", "JOIN", "UPDATE"];
        let mut refs: Vec<String> = tokens
            .windows(2)
            .filter_map(|w| trigger.contains(&w[0]).then_some(w[1].to_lowercase()))
            .collect();
        for real_table in alias_map.map.values().filter_map(|v| v.as_deref()) {
            if !refs.iter().any(|r| r == real_table) {
                refs.push(real_table.to_string());
            }
        }
        refs
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
        for token in tokenize(line) {
            match token {
                SqlToken::Comment(s)       => styled.push((Style::new().dimmed(), s)),
                SqlToken::StringLiteral(s) => styled.push((Style::new().fg(Color::Yellow), s)),
                SqlToken::Number(s)        => styled.push((Style::new().fg(Color::Magenta), s)),
                SqlToken::Word(s) => {
                    let style = match classify_word(&s, &self.tables, &self.columns) {
                        Some(kind) => kind.style(),
                        None       => Style::new(),
                    };
                    styled.push((style, s));
                }
                SqlToken::Other(c) => styled.push((Style::new(), c.to_string())),
            }
        }
        styled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::db_connection::QueryResult;
    use std::collections::HashMap;

    struct TestDb {
        tables: Vec<String>,
        columns: HashMap<String, Vec<String>>,
    }

    impl crate::core::ports::db_connection::DbConnection for TestDb {
        fn execute(&self, _: &str) -> Result<QueryResult, String> {
            Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: None })
        }
        fn list_tables(&self) -> Result<Vec<String>, String> {
            Ok(self.tables.clone())
        }
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            Ok(self.columns.clone())
        }
    }

    fn schema_with(tables: &[&str], columns: &[(&str, &[&str])]) -> SchemaService {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table, cols) in columns {
            col_map.insert(
                table.to_string(),
                cols.iter().map(|c| c.to_string()).collect(),
            );
        }
        SchemaService::load(&TestDb {
            tables: tables.iter().map(|t| t.to_string()).collect(),
            columns: col_map,
        })
        .unwrap()
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

    #[test]
    fn suggests_columns_after_table_dot() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT users.";
        let results = c.complete_input(input, input.len());
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id [column] in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
        assert!(results.iter().any(|(r, _)| r == "email"));
        assert!(results.iter().any(|(r, _)| r == "created_at"));
    }

    #[test]
    fn filters_columns_after_table_dot_with_prefix() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT users.em";
        let results = c.complete_input(input, input.len());
        assert!(results.iter().any(|(r, _)| r == "email"), "expected email");
        assert!(!results.iter().any(|(r, _)| r == "id"), "id should not appear");
    }

    #[test]
    fn suggests_columns_after_schema_table_dot() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email"])],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT public.users.";
        let results = c.complete_input(input, input.len());
        assert!(
            results.iter().any(|(r, _)| r == "id"),
            "expected id from public.users. in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn word_start_returns_position_after_dot() {
        // "SELECT users." — word_start at pos=13 should be 13 (after the dot)
        assert_eq!(word_start("SELECT users.", 13), 13);
    }

    #[test]
    fn word_start_returns_position_after_last_dot_in_schema_table() {
        // "SELECT public.users." — word_start at pos=20 should be 20
        assert_eq!(word_start("SELECT public.users.", 20), 20);
    }

    #[test]
    fn completion_kind_label_keyword() {
        assert_eq!(CompletionKind::Keyword.label(), "[keyword]");
    }

    #[test]
    fn completion_kind_label_table() {
        assert_eq!(CompletionKind::Table.label(), "[table]");
    }

    #[test]
    fn completion_kind_label_column() {
        assert_eq!(CompletionKind::Column.label(), "[column]");
    }

    #[test]
    fn completion_kind_style_returns_distinct_styles() {
        let kw = CompletionKind::Keyword.style();
        let tbl = CompletionKind::Table.style();
        let col = CompletionKind::Column.style();
        assert_ne!(format!("{kw:?}"), format!("{tbl:?}"));
        assert_ne!(format!("{tbl:?}"), format!("{col:?}"));
    }

    #[test]
    fn completer_trait_complete_returns_suggestions() {
        use reedline::Completer;
        let schema = schema_with(&["users", "orders"], &[]);
        let mut c = SqlCompleter::new(schema);
        let suggestions = c.complete("SELECT * FROM ", 13);
        assert!(suggestions.iter().any(|s| s.value == "users"));
        assert!(suggestions.iter().any(|s| s.value == "orders"));
    }

    #[test]
    fn completer_trait_complete_includes_description_and_span() {
        use reedline::Completer;
        let schema = schema_with(&[], &[]);
        let mut c = SqlCompleter::new(schema);
        let suggestions = c.complete("SEL", 3);
        let sel = suggestions.iter().find(|s| s.value == "SELECT").unwrap();
        assert_eq!(sel.description.as_deref(), Some("[keyword]"));
        assert_eq!(sel.span.start, 0);
        assert_eq!(sel.span.end, 3);
    }

    #[test]
    fn highlighter_new_collects_all_columns() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email"])],
        );
        let h = SqlHighlighter::new(schema);
        assert!(h.tables.contains(&"users".to_string()));
        assert!(h.columns.contains(&"id".to_string()));
        assert!(h.columns.contains(&"email".to_string()));
    }

    #[test]
    fn highlighter_highlight_returns_styled_text() {
        use reedline::Highlighter;
        let schema = schema_with(&["users"], &[("users", &["id"])]);
        let h = SqlHighlighter::new(schema);
        let styled = h.highlight("SELECT id FROM users", 0);
        let combined: String = styled.buffer.iter().map(|(_, s)| s.as_str()).collect();
        assert!(combined.contains("SELECT"));
        assert!(combined.contains("users"));
    }

    #[test]
    fn highlighter_highlight_covers_comment_string_number_and_plain_word() {
        use reedline::Highlighter;
        let schema = schema_with(&[], &[]);
        let h = SqlHighlighter::new(schema);
        // exercises Comment, StringLiteral, Number, and plain-word (None) branches
        let styled = h.highlight("-- note\n'hello' 42 foo", 0);
        let combined: String = styled.buffer.iter().map(|(_, s)| s.as_str()).collect();
        assert!(combined.contains("note"));
        assert!(combined.contains("hello"));
        assert!(combined.contains("42"));
        assert!(combined.contains("foo"));
    }

    #[test]
    fn qualified_dot_with_unknown_table_falls_back_to_all_columns() {
        let schema = schema_with(&["users"], &[("users", &["id", "email"])]);
        let c = SqlCompleter::new(schema);
        // "ghost" is not a known table — should fall back to all columns
        let input = "SELECT ghost.";
        let results = c.complete_input(input, input.len());
        assert!(results.iter().any(|(r, _)| r == "id"), "expected fallback column id");
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn select_without_from_suggests_keywords() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema);
        // SELECT followed by space, no FROM yet — table_refs empty → keywords
        let results = c.complete_input("SELECT ", 7);
        assert!(
            results.iter().any(|(r, k)| r == "FROM" && matches!(k, CompletionKind::Keyword)),
            "expected FROM keyword when no table referenced yet"
        );
    }

    #[test]
    fn alias_map_resolve_known_alias() {
        let mut m = AliasMap { map: std::collections::HashMap::new() };
        m.map.insert("u".to_string(), Some("users".to_string()));
        assert_eq!(m.resolve("u"), Some("users"));
    }

    #[test]
    fn alias_map_resolve_unknown_returns_none() {
        let m = AliasMap { map: std::collections::HashMap::new() };
        assert_eq!(m.resolve("x"), None);
    }

    #[test]
    fn alias_map_resolve_subquery_alias_returns_none() {
        let mut m = AliasMap { map: std::collections::HashMap::new() };
        m.map.insert("s".to_string(), None);
        assert_eq!(m.resolve("s"), None);
    }

    #[test]
    fn build_alias_map_from_without_as() {
        let m = build_alias_map("SELECT * FROM users u");
        assert_eq!(m.resolve("u"), Some("users"));
    }

    #[test]
    fn build_alias_map_from_with_as() {
        let m = build_alias_map("SELECT * FROM users AS u");
        assert_eq!(m.resolve("u"), Some("users"));
    }

    #[test]
    fn build_alias_map_join_alias() {
        let m = build_alias_map("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        assert_eq!(m.resolve("u"), Some("users"));
        assert_eq!(m.resolve("o"), Some("orders"));
    }

    #[test]
    fn build_alias_map_table_without_alias_not_in_map() {
        let m = build_alias_map("SELECT * FROM users");
        assert_eq!(m.resolve("users"), None);
    }

    #[test]
    fn build_alias_map_comma_separated() {
        let m = build_alias_map("SELECT * FROM users u, orders o");
        assert_eq!(m.resolve("u"), Some("users"));
        assert_eq!(m.resolve("o"), Some("orders"));
    }

    #[test]
    fn build_alias_map_subquery_with_as() {
        let m = build_alias_map("SELECT * FROM (SELECT id FROM users) AS s");
        assert_eq!(m.resolve("s"), None);
    }

    #[test]
    fn build_alias_map_subquery_without_as() {
        let m = build_alias_map("SELECT * FROM (SELECT id FROM users) s");
        assert_eq!(m.resolve("s"), None);
    }

    #[test]
    fn alias_simple() {
        let schema = schema_with(&["users"], &[("users", &["id", "email", "created_at"])]);
        let c = SqlCompleter::new(schema);
        // cursor at pos 9 — "SELECT u." — alias defined later in full line
        let results = c.complete_input("SELECT u. FROM users u", 9);
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id [column] via alias u, got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn alias_with_as() {
        let schema = schema_with(&["users"], &[("users", &["id", "email"])]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT u. FROM users AS u", 9);
        assert!(results.iter().any(|(r, _)| r == "id"), "expected id via AS alias");
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn alias_prefix_filter() {
        let schema = schema_with(&["users"], &[("users", &["id", "email", "created_at"])]);
        let c = SqlCompleter::new(schema);
        // "SELECT u.em" — pos=11
        let results = c.complete_input("SELECT u.em FROM users u", 11);
        assert!(results.iter().any(|(r, _)| r == "email"), "expected email");
        assert!(!results.iter().any(|(r, _)| r == "id"), "id should not appear");
        assert!(!results.iter().any(|(r, _)| r == "created_at"), "created_at should not appear");
    }

    #[test]
    fn alias_in_where_trigger() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email"])],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT u.id FROM users u WHERE ";
        let results = c.complete_input(input, input.len());
        assert!(
            results.iter().any(|(r, k)| r == "email" && matches!(k, CompletionKind::Column)),
            "expected email via WHERE trigger, got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id via WHERE trigger"
        );
    }

    #[test]
    fn multi_alias() {
        let schema = schema_with(
            &["users", "orders"],
            &[("users", &["id", "email"]), ("orders", &["id", "user_id"])],
        );
        let c = SqlCompleter::new(schema);
        // "SELECT o." — pos=9 — alias o resolves to orders
        let results = c.complete_input("SELECT o. FROM users u JOIN orders o ON u.id = o.user_id", 9);
        assert!(
            results.iter().any(|(r, _)| r == "user_id"),
            "expected user_id from orders via alias o, got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
        assert!(!results.iter().any(|(r, _)| r == "email"), "email from users should not appear");
    }

    #[test]
    fn fuzzy_match_empty_query_matches_everything() {
        assert!(fuzzy_match("users", ""));
        assert!(fuzzy_match("orders", ""));
    }

    #[test]
    fn fuzzy_match_prefix_still_works() {
        assert!(fuzzy_match("users", "use"));
    }

    #[test]
    fn fuzzy_match_subsequence_usr_users() {
        assert!(fuzzy_match("users", "usr"));
    }

    #[test]
    fn fuzzy_match_subsequence_crat_created_at() {
        assert!(fuzzy_match("created_at", "crat"));
    }

    #[test]
    fn fuzzy_match_no_match() {
        assert!(!fuzzy_match("users", "xyz"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("Users", "usr"));
        assert!(fuzzy_match("users", "USR"));
    }

    #[test]
    fn complete_input_fuzzy_matches_table_by_subsequence() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT * FROM usr", 17);
        assert!(
            results.iter().any(|(r, _)| r == "users"),
            "expected 'users' via fuzzy 'usr', got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn complete_input_fuzzy_matches_column_by_subsequence() {
        let schema = schema_with(
            &["users"],
            &[("users", &["created_at", "email"])],
        );
        let c = SqlCompleter::new(schema);
        let results = c.complete_input("SELECT crat FROM users", 11);
        assert!(
            results.iter().any(|(r, _)| r == "created_at"),
            "expected 'created_at' via fuzzy 'crat', got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn complete_qualified_fuzzy_matches_column_by_subsequence() {
        let schema = schema_with(
            &["users"],
            &[("users", &["created_at", "email"])],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT users.crat";
        let results = c.complete_input(input, input.len());
        assert!(
            results.iter().any(|(r, _)| r == "created_at"),
            "expected 'created_at' via fuzzy 'crat' in dot-completion, got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn extract_join_context_finds_right_and_left_tables() {
        let alias_map = build_alias_map("SELECT * FROM users JOIN orders ON");
        let ctx = extract_join_context("SELECT * FROM USERS JOIN ORDERS ON", &alias_map)
            .expect("should find join context");
        assert_eq!(ctx.right_table, "orders");
        assert!(ctx.left_tables.contains(&"users".to_string()), "left_tables: {:?}", ctx.left_tables);
    }

    #[test]
    fn extract_join_context_resolves_aliases() {
        // alias map: u -> users, o -> orders
        let alias_map = build_alias_map("SELECT * FROM users u JOIN orders o ON");
        // upper query passes alias tokens (u, o) not real table names
        let ctx = extract_join_context("SELECT * FROM U JOIN O ON", &alias_map)
            .expect("should find join context with aliases");
        assert_eq!(ctx.right_table, "orders", "right alias 'o' should resolve to 'orders'");
        assert!(
            ctx.left_tables.contains(&"users".to_string()),
            "left alias 'u' should resolve to 'users', left_tables: {:?}",
            ctx.left_tables
        );
    }

    #[test]
    fn extract_join_context_no_join_returns_none() {
        let alias_map = build_alias_map("SELECT * FROM users");
        let ctx = extract_join_context("SELECT * FROM USERS ON", &alias_map);
        assert!(ctx.is_none(), "expected None when no JOIN present");
    }

    #[test]
    fn extract_join_context_multi_join_uses_last() {
        let alias_map = build_alias_map("SELECT * FROM a JOIN b ON b.x = a.y JOIN c ON");
        let ctx = extract_join_context("SELECT * FROM A JOIN B ON B.X = A.Y JOIN C ON", &alias_map)
            .expect("should find context for last JOIN");
        assert_eq!(ctx.right_table, "c");
        assert!(ctx.left_tables.contains(&"a".to_string()), "left_tables: {:?}", ctx.left_tables);
        assert!(ctx.left_tables.contains(&"b".to_string()), "left_tables: {:?}", ctx.left_tables);
    }

    #[test]
    fn join_on_shared_column_appears_first() {
        let schema = schema_with(
            &["users", "orders"],
            &[
                ("users", &["id", "email"]),
                ("orders", &["id", "user_id"]),
            ],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT * FROM users JOIN orders ON ";
        let results = c.complete_input(input, input.len());
        let id_pos = results.iter().position(|(r, _)| r == "id");
        let email_pos = results.iter().position(|(r, _)| r == "email");
        assert!(id_pos.is_some(), "expected 'id' in results");
        assert!(email_pos.is_some(), "expected 'email' in results");
        assert!(
            id_pos.unwrap() < email_pos.unwrap(),
            "shared column 'id' should appear before non-shared 'email'"
        );
    }

    #[test]
    fn join_on_with_aliases_includes_both_tables_columns() {
        let schema = schema_with(
            &["users", "orders"],
            &[
                ("users", &["id", "email"]),
                ("orders", &["id", "user_id"]),
            ],
        );
        let c = SqlCompleter::new(schema);
        let input = "SELECT * FROM users u JOIN orders o ON ";
        let results = c.complete_input(input, input.len());
        assert!(results.iter().any(|(r, _)| r == "user_id"), "expected user_id from orders");
        assert!(results.iter().any(|(r, _)| r == "email"), "expected email from users");
    }

    #[test]
    fn on_without_prior_join_falls_back_to_all_table_cols() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email"])],
        );
        let c = SqlCompleter::new(schema);
        // ON without a preceding JOIN — unusual but must not panic, fall back to table columns
        let input = "SELECT id FROM users ON ";
        let results = c.complete_input(input, input.len());
        assert!(results.iter().any(|(r, _)| r == "id" || r == "email"),
            "fallback should return columns from known tables");
    }

    #[test]
    fn common_prefix_multiple_shared() {
        let cands = vec![
            ("transaction".to_string(), CompletionKind::Table),
            ("transaction_detail".to_string(), CompletionKind::Table),
            ("transaction_shipment".to_string(), CompletionKind::Table),
        ];
        assert_eq!(common_prefix(&cands), "transaction");
    }

    #[test]
    fn common_prefix_single_candidate() {
        let cands = vec![("users".to_string(), CompletionKind::Table)];
        assert_eq!(common_prefix(&cands), "users");
    }

    #[test]
    fn common_prefix_empty_candidates() {
        assert_eq!(common_prefix(&[]), "");
    }

    #[test]
    fn common_prefix_no_shared_chars() {
        let cands = vec![
            ("users".to_string(), CompletionKind::Table),
            ("orders".to_string(), CompletionKind::Table),
        ];
        assert_eq!(common_prefix(&cands), "");
    }

    #[test]
    fn common_prefix_partial_overlap() {
        let cands = vec![
            ("users".to_string(), CompletionKind::Table),
            ("user_sessions".to_string(), CompletionKind::Table),
            ("user_profiles".to_string(), CompletionKind::Table),
        ];
        assert_eq!(common_prefix(&cands), "user");
    }

    #[test]
    fn common_prefix_case_insensitive_preserves_first_case() {
        let cands = vec![
            ("Users".to_string(), CompletionKind::Table),
            ("users_sessions".to_string(), CompletionKind::Table),
        ];
        assert_eq!(common_prefix(&cands), "Users");
    }
}
