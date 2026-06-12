use std::collections::HashMap;

use nu_ansi_term::{Color, Style};
use reedline::{Completer, Highlighter, Hinter, History, Span, StyledText, Suggestion};

use pgrs_core::{Completion, CompletionKind, CompletionsApi, SchemaApi, SqlToken, tokenize, SQL_KEYWORDS};

/// UI-side presentation of the (external) `CompletionKind` value type.
trait CompletionKindExt {
    fn label(&self) -> &'static str;
    fn style(&self) -> Style;
}

impl CompletionKindExt for CompletionKind {
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

pub(crate) fn common_prefix(candidates: &[(String, CompletionKind)]) -> String {
    if candidates.is_empty() {
        return String::new();
    }
    let first = &candidates[0].0;
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
    api: CompletionsApi,
}

impl SqlCompleter {
    pub fn new(
        schema: SchemaApi,
        table_freq: HashMap<String, u64>,
        column_freq: HashMap<String, u64>,
    ) -> Self {
        Self { api: CompletionsApi::new(&schema, table_freq, column_freq) }
    }

    pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
        self.api
            .completions(line, pos)
            .into_iter()
            .map(|c| (c.value, c.kind))
            .collect()
    }
}

/// Maximum number of suggestions shown in the completion menu at once.
const MAX_COMPLETIONS: usize = 10;

impl Completer for SqlCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let start = word_start(line, pos);
        self.api
            .completions(line, pos)
            .into_iter()
            .take(MAX_COMPLETIONS)
            .map(|Completion { value, kind }| Suggestion {
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
    pub fn new(schema: SchemaApi) -> Self {
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

pub struct SqlHinter {
    completer: SqlCompleter,
    current_hint: String,
    style: Style,
}

impl SqlHinter {
    pub fn new(
        schema: SchemaApi,
        table_freq: HashMap<String, u64>,
        column_freq: HashMap<String, u64>,
    ) -> Self {
        Self {
            completer: SqlCompleter::new(schema, table_freq, column_freq),
            current_hint: String::new(),
            style: Style::new().fg(Color::DarkGray),
        }
    }
}

impl Hinter for SqlHinter {
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        _history: &dyn History,
        use_ansi_coloring: bool,
        _cwd: &str,
    ) -> String {
        let candidates = self.completer.complete_input(line, pos);

        let start = word_start(line, pos);
        let current_word = &line[start..pos];

        let prefix_candidates: Vec<_> = candidates
            .into_iter()
            .filter(|(c, k)| {
                !matches!(k, CompletionKind::Keyword)
                    && c.to_lowercase().starts_with(&current_word.to_lowercase())
            })
            .collect();
        let prefix = common_prefix(&prefix_candidates);

        self.current_hint = if !prefix.is_empty()
            && prefix.chars().count() > current_word.chars().count()
            && prefix.to_lowercase().starts_with(&current_word.to_lowercase())
        {
            prefix.chars().skip(current_word.chars().count()).collect()
        } else {
            String::new()
        };

        if use_ansi_coloring && !self.current_hint.is_empty() {
            self.style.paint(&self.current_hint).to_string()
        } else {
            self.current_hint.clone()
        }
    }

    fn complete_hint(&self) -> String {
        self.current_hint.clone()
    }

    fn next_hint_token(&self) -> String {
        self.current_hint
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use reedline::Highlighter;

    fn render_to_ansi(line: &str, schema: SchemaApi) -> String {
        let h = SqlHighlighter::new(schema);
        h.highlight(line, 0)
            .buffer
            .iter()
            .map(|(style, text)| style.paint(text).to_string())
            .collect()
    }

    fn schema_with(tables: &[&str], columns: &[(&str, &[&str])]) -> SchemaApi {
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
        SchemaApi::for_test(col_map)
    }

    #[test]
    fn suggests_keywords_at_start_of_input() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SEL", 3);
        assert!(
            results.iter().any(|(r, _)| r == "SELECT"),
            "expected SELECT in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn suggests_table_names_after_from() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT * FROM ", 13);
        assert!(results.iter().any(|(r, _)| r == "users"));
        assert!(results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn suggests_table_names_after_join() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT * FROM users JOIN ", 24);
        assert!(results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn suggests_columns_after_select_when_table_known() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(results.iter().any(|(r, _)| r == "id"), "expected id in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>());
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn filters_by_current_word_prefix() {
        let schema = schema_with(&["users", "user_sessions"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT * FROM user", 18);
        assert!(results.iter().any(|(r, _)| r == "users"));
        assert!(results.iter().any(|(r, _)| r == "user_sessions"));
        assert!(!results.iter().any(|(r, _)| r == "orders"));
    }

    #[test]
    fn no_duplicate_suggestions() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT * FROM ", 14);
        let names: Vec<&str> = results.iter().map(|(r, _)| r.as_str()).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "duplicates found: {:?}", names);
    }

    #[test]
    fn dedup_removes_same_name_same_kind_from_multiple_tables() {
        let schema = schema_with(
            &["users", "orders"],
            &[("users", &["id", "email"]), ("orders", &["id", "status"])],
        );
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT  FROM users JOIN orders ON users.id = orders.id", 7);
        let id_count = results.iter().filter(|(r, _)| r == "id").count();
        assert_eq!(id_count, 1, "id should appear once, not once per joined table");
    }

    #[test]
    fn schema_qualified_from_suggests_columns() {
        let schema = schema_with(&["users"], &[("users", &["id", "email"])]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let input = "SELECT  FROM public.users";
        let results = c.complete_input(input, 7);
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id column after schema-qualified FROM, got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn tags_keywords_with_keyword_kind() {
        let schema = schema_with(&[], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SEL", 3);
        assert!(
            results.iter().any(|(r, k)| r == "SELECT" && matches!(k, CompletionKind::Keyword)),
            "expected SELECT [keyword] in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tags_tables_with_table_kind() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT  FROM users", 7);
        assert!(
            results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
            "expected id [column]"
        );
    }

    #[test]
    fn highlight_keyword_bold_cyan() {
        let rendered = render_to_ansi("SELECT", schema_with(&[], &[]));
        let expected = CompletionKind::Keyword.style().paint("SELECT").to_string();
        assert!(rendered.contains(&expected), "keyword not styled correctly, got: {rendered}");
    }

    #[test]
    fn highlight_string_literal_yellow() {
        let rendered = render_to_ansi("'hello'", schema_with(&[], &[]));
        use nu_ansi_term::Color;
        let expected = Color::Yellow.paint("'hello'").to_string();
        assert!(rendered.contains(&expected), "string literal not yellow, got: {rendered}");
    }

    #[test]
    fn highlight_number_magenta() {
        let rendered = render_to_ansi("42", schema_with(&[], &[]));
        use nu_ansi_term::Color;
        let expected = Color::Magenta.paint("42").to_string();
        assert!(rendered.contains(&expected), "number not magenta, got: {rendered}");
    }

    #[test]
    fn highlight_comment_dim() {
        let rendered = render_to_ansi("-- comment", schema_with(&[], &[]));
        use nu_ansi_term::Style;
        let expected = Style::new().dimmed().paint("-- comment").to_string();
        assert!(rendered.contains(&expected), "comment not dim, got: {rendered}");
    }

    #[test]
    fn highlight_table_name_bold_yellow() {
        let schema = schema_with(&["users"], &[]);
        let rendered = render_to_ansi("users", schema);
        let expected = CompletionKind::Table.style().paint("users").to_string();
        assert!(rendered.contains(&expected), "table not styled correctly, got: {rendered}");
    }

    #[test]
    fn highlight_column_name_green() {
        let schema = schema_with(&[], &[("_dummy", &["email"])]);
        let rendered = render_to_ansi("email", schema);
        let expected = CompletionKind::Column.style().paint("email").to_string();
        assert!(rendered.contains(&expected), "column not styled correctly, got: {rendered}");
    }

    #[test]
    fn highlight_plain_word_no_escape() {
        let rendered = render_to_ansi("foo", schema_with(&[], &[]));
        assert!(!rendered.contains('\x1b'), "unknown word should have no ANSI escape, got: {rendered}");
    }

    #[test]
    fn highlight_number_trailing_dot_not_consumed() {
        let rendered = render_to_ansi("10.", schema_with(&[], &[]));
        use nu_ansi_term::Color;
        let expected_num = Color::Magenta.paint("10").to_string();
        assert!(rendered.contains(&expected_num), "10 should be magenta, got: {rendered}");
        assert!(rendered.ends_with('.'), "trailing dot should be plain, got: {rendered}");
    }

    #[test]
    fn highlight_number_decimal_consumed() {
        let rendered = render_to_ansi("3.14", schema_with(&[], &[]));
        use nu_ansi_term::Color;
        let expected = Color::Magenta.paint("3.14").to_string();
        assert!(rendered.contains(&expected), "3.14 should be one magenta span, got: {rendered}");
    }

    #[test]
    fn highlight_string_with_escaped_quote() {
        let rendered = render_to_ansi("'O''Brien'", schema_with(&[], &[]));
        use nu_ansi_term::Color;
        let expected = Color::Yellow.paint("'O''Brien'").to_string();
        assert_eq!(rendered, expected);
    }

    #[test]
    fn highlight_mixed_query() {
        let schema = schema_with(&["users"], &[]);
        let rendered = render_to_ansi("SELECT * FROM users WHERE id = 1", schema);
        let kw_select = CompletionKind::Keyword.style().paint("SELECT").to_string();
        let tbl_users = CompletionKind::Table.style().paint("users").to_string();
        use nu_ansi_term::Color;
        let num_1 = Color::Magenta.paint("1").to_string();
        assert!(rendered.contains(&kw_select), "SELECT should be keyword style, got: {rendered}");
        assert!(rendered.contains(&tbl_users), "users should be table style, got: {rendered}");
        assert!(rendered.contains(&num_1), "1 should be magenta, got: {rendered}");
    }

    #[test]
    fn suggests_columns_after_table_dot() {
        let schema = schema_with(
            &["users"],
            &[("users", &["id", "email", "created_at"])],
        );
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let input = "SELECT public.users.";
        let results = c.complete_input(input, input.len());
        assert!(
            results.iter().any(|(r, _)| r == "id"),
            "expected id from public.users. in {:?}", results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
    }

    #[test]
    fn word_start_returns_position_after_dot() {
        assert_eq!(word_start("SELECT users.", 13), 13);
    }

    #[test]
    fn word_start_returns_position_after_last_dot_in_schema_table() {
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
        let mut c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let suggestions = c.complete("SELECT * FROM ", 13);
        assert!(suggestions.iter().any(|s| s.value == "users"));
        assert!(suggestions.iter().any(|s| s.value == "orders"));
    }

    #[test]
    fn completer_trait_complete_includes_description_and_span() {
        use reedline::Completer;
        let schema = schema_with(&[], &[]);
        let mut c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let input = "SELECT ghost.";
        let results = c.complete_input(input, input.len());
        assert!(results.iter().any(|(r, _)| r == "id"), "expected fallback column id");
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn select_without_from_suggests_tables() {
        let schema = schema_with(&["users", "orders"], &[("users", &["id", "email"])]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT ", 7);
        assert!(
            results.iter().any(|(r, k)| r == "users" && matches!(k, CompletionKind::Table)),
            "expected tables when no FROM clause, got: {:?}", results
        );
        assert!(
            !results.iter().any(|(_, k)| matches!(k, CompletionKind::Column)),
            "columns should not appear without FROM"
        );
    }

    #[test]
    fn alias_simple() {
        let schema = schema_with(&["users"], &[("users", &["id", "email", "created_at"])]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT u. FROM users AS u", 9);
        assert!(results.iter().any(|(r, _)| r == "id"), "expected id via AS alias");
        assert!(results.iter().any(|(r, _)| r == "email"));
    }

    #[test]
    fn alias_prefix_filter() {
        let schema = schema_with(&["users"], &[("users", &["id", "email", "created_at"])]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT o. FROM users u JOIN orders o ON u.id = o.user_id", 9);
        assert!(
            results.iter().any(|(r, _)| r == "user_id"),
            "expected user_id from orders via alias o, got: {:?}",
            results.iter().map(|(r, _)| r).collect::<Vec<_>>()
        );
        assert!(!results.iter().any(|(r, _)| r == "email"), "email from users should not appear");
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
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

    fn empty_history() -> reedline::FileBackedHistory {
        reedline::FileBackedHistory::new(0).expect("in-memory history")
    }

    #[test]
    fn hinter_shows_suffix_for_partial_table_match() {
        let schema = schema_with(&["transaction", "transaction_detail"], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let hint = h.handle("SELECT * FROM tran", 18, &history, false, "");
        assert_eq!(hint, "saction");
    }

    #[test]
    fn hinter_empty_when_no_candidates() {
        let schema = schema_with(&["users"], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let hint = h.handle("SELECT * FROM xyz", 17, &history, false, "");
        assert_eq!(hint, "");
    }

    #[test]
    fn hinter_empty_when_word_already_equals_prefix() {
        let schema = schema_with(&["users"], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let input = "SELECT * FROM users";
        let hint = h.handle(input, input.len(), &history, false, "");
        assert_eq!(hint, "");
    }

    #[test]
    fn hinter_complete_hint_returns_stored_suffix() {
        let schema = schema_with(&["transaction", "transaction_detail"], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        h.handle("SELECT * FROM tran", 18, &history, false, "");
        assert_eq!(h.complete_hint(), "saction");
    }

    #[test]
    fn hinter_complete_hint_empty_before_first_handle() {
        let schema = schema_with(&["users"], &[]);
        let h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        assert_eq!(h.complete_hint(), "");
    }

    #[test]
    fn hinter_shows_column_suffix_via_dot_notation() {
        let schema = schema_with(
            &["users"],
            &[("users", &["email", "email_verified"])],
        );
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let input = "SELECT users.em";
        let hint = h.handle(input, input.len(), &history, false, "");
        assert_eq!(hint, "ail");
    }

    #[test]
    fn hinter_clears_after_word_grows_past_prefix() {
        let schema = schema_with(&["transaction"], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let hint1 = h.handle("FROM transactio", 15, &history, false, "");
        assert_eq!(hint1, "n");
        let hint2 = h.handle("FROM transactions", 17, &history, false, "");
        assert_eq!(hint2, "");
    }

    #[test]
    fn hinter_no_hint_for_keyword_prefix() {
        let schema = schema_with(&[], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let hint = h.handle("sel", 3, &history, false, "");
        assert_eq!(hint, "", "keyword prefix should produce no ghost text");
    }

    #[test]
    fn tables_sorted_by_length_then_alpha() {
        let schema = schema_with(&["users", "user_role", "user_store"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT * FROM use", 17);
        let names: Vec<&str> = results.iter().map(|(r, _)| r.as_str()).collect();
        let pos_users = names.iter().position(|&n| n == "users").unwrap();
        let pos_role  = names.iter().position(|&n| n == "user_role").unwrap();
        let pos_store = names.iter().position(|&n| n == "user_store").unwrap();
        assert!(pos_users < pos_role,  "users should come before user_role");
        assert!(pos_users < pos_store, "users should come before user_store");
    }

    #[test]
    fn columns_sorted_by_length_then_alpha() {
        let schema = schema_with(
            &["orders"],
            &[("orders", &["id", "status", "created_at"])],
        );
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("SELECT  FROM orders", 7);
        let names: Vec<&str> = results.iter().map(|(r, _)| r.as_str()).collect();
        let pos_id         = names.iter().position(|&n| n == "id").unwrap();
        let pos_status     = names.iter().position(|&n| n == "status").unwrap();
        let pos_created_at = names.iter().position(|&n| n == "created_at").unwrap();
        assert!(pos_id < pos_status,     "id should come before status");
        assert!(pos_status < pos_created_at, "status should come before created_at");
    }

    #[test]
    fn hinter_shows_common_prefix_for_ambiguous_tables() {
        let schema = schema_with(&["users", "user_role", "user_store"], &[]);
        let mut h = SqlHinter::new(schema, HashMap::new(), HashMap::new());
        let history = empty_history();
        let hint = h.handle("SELECT * FROM use", 17, &history, false, "");
        assert_eq!(hint, "r", "hint should be the common prefix suffix 'r'");
    }

    #[test]
    fn completes_table_name_after_backslash_d() {
        let schema = schema_with(&["users", "user_roles", "orders"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("\\d use", 6);
        let names: Vec<_> = results.iter().map(|(s, _)| s.as_str()).collect();
        assert!(names.contains(&"users"), "got: {names:?}");
        assert!(names.contains(&"user_roles"), "got: {names:?}");
        assert!(!names.contains(&"orders"), "orders should be filtered out, got: {names:?}");
    }

    #[test]
    fn completes_table_name_after_backslash_d_plus() {
        let schema = schema_with(&["users", "orders"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("\\d+ ord", 7);
        let names: Vec<_> = results.iter().map(|(s, _)| s.as_str()).collect();
        assert!(names.contains(&"orders"), "got: {names:?}");
        assert!(!names.contains(&"users"), "got: {names:?}");
    }

    #[test]
    fn completions_for_backslash_d_are_table_kind() {
        let schema = schema_with(&["users"], &[]);
        let c = SqlCompleter::new(schema, HashMap::new(), HashMap::new());
        let results = c.complete_input("\\d ", 3);
        let kinds: Vec<_> = results.iter().map(|(_, k)| k).collect();
        assert!(kinds.iter().all(|k| matches!(k, CompletionKind::Table)), "got: {kinds:?}");
    }
}
