mod alias;
mod completer;
mod executor;
mod tokenizer;
mod describe;

use std::borrow::Cow;
use std::io::{self, Write};

use reedline::{
    ColumnarMenu, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    ValidationResult, Validator, default_emacs_keybindings,
};

use crate::core::ports::db_connection::DbConnection;
use crate::core::ports::repl_port::ReplPort;
use crate::core::ports::schema_port::SchemaPort;
use crate::core::services::schema::service::SchemaService;

use completer::{SqlCompleter, SqlHighlighter, SqlHinter};
use describe::describe_table;
use executor::format_result;

fn is_complete_statement(s: &str) -> bool {
    let s = s.trim_end();
    if !s.ends_with(';') {
        return false;
    }
    let mut in_single = false; // inside '...'
    let mut in_double = false; // inside "..." (quoted identifier)
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                if in_single && chars.peek() == Some(&'\'') {
                    chars.next(); // '' escape inside string
                } else {
                    in_single = !in_single;
                }
            }
            '"' if !in_single => {
                if in_double && chars.peek() == Some(&'"') {
                    chars.next(); // "" escape inside quoted identifier
                } else {
                    in_double = !in_double;
                }
            }
            _ => {}
        }
    }
    !in_single && !in_double
}

struct PgrsPrompt {
    db_name: String,
}

impl Prompt for PgrsPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Owned(format!("pgrs ({})", self.db_name))
    }
    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }
    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("   -> ")
    }
    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

struct SqlValidator;

impl Validator for SqlValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('\\') || is_complete_statement(line) {
            ValidationResult::Complete
        } else {
            ValidationResult::Incomplete
        }
    }
}

// To add a new REPL command: append one (&str, &str) entry here.
const REPL_COMMANDS: &[(&str, &str)] = &[
    ("\\dt",       "list tables with column count"),
    ("\\d <table>",  "describe table (columns, indexes, constraints)"),
    ("\\d+ <table>", "describe table (extended: + storage, triggers, comments)"),
    ("\\l",        "list databases"),
    ("\\x",        "toggle expanded display"),
    ("\\timing",   "toggle query execution time"),
    ("\\refresh",  "reload schema (after CREATE/DROP/ALTER TABLE)"),
    ("\\help, \\?","show this help"),
    ("\\q, exit",  "quit (or Ctrl+D)"),
];

fn repl_help_text() -> String {
    let cmd_w = REPL_COMMANDS.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
    let commands: String = REPL_COMMANDS
        .iter()
        .map(|(cmd, desc)| format!("  {cmd:<cmd_w$}  {desc}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "  Type any SQL and end it with ';' to run it (Enter alone continues a\n\
           multi-line statement until the ';').\n\n\
         {commands}"
    )
}

fn build_reedline(schema: SchemaService) -> Reedline {
    let highlighter = SqlHighlighter::new(schema.clone());
    let hinter = SqlHinter::new(schema.clone());
    let completer = SqlCompleter::new(schema);

    let menu = ColumnarMenu::default().with_name("completion_menu");

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            // Accept ghost text first (table/column prefix completion).
            // Keywords never produce ghost text, so this is Inapplicable for them
            // and falls through to the menu which does uppercase replacement.
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    Reedline::create()
        .with_completer(Box::new(completer))
        .with_hinter(Box::new(hinter))
        .with_highlighter(Box::new(highlighter))
        .with_validator(Box::new(SqlValidator))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_quick_completions(true)
        .with_partial_completions(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
}

fn is_ddl(query: &str) -> bool {
    matches!(
        query
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_uppercase()
            .as_str(),
        "CREATE" | "DROP" | "ALTER" | "TRUNCATE"
    )
}

fn handle_dt(schema: &SchemaService, writer: &mut impl Write) {
    let tables = schema.tables();
    if tables.is_empty() {
        writeln!(writer, "No tables.").ok();
    } else {
        let name_w = tables.iter().map(|t| t.len()).max().unwrap_or(0);
        for table in tables {
            let col_count = schema.columns_for(table).len();
            writeln!(writer, " {:<name_w$}  ({} columns)", table, col_count).ok();
        }
    }
}

const LIST_DATABASES_SQL: &str =
    "SELECT datname AS database \
     FROM pg_database \
     WHERE datistemplate = false \
     ORDER BY datname";

fn handle_l(conn: &dyn DbConnection, expanded: bool, writer: &mut impl Write) {
    match conn.execute(LIST_DATABASES_SQL) {
        Ok(result) => write!(writer, "{}", format_result(&result, expanded)).ok(),
        Err(e) => { eprintln!("error: {}", e); None }
    };
}

fn handle_refresh(
    conn: &dyn SchemaPort,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    writer: &mut impl Write,
) {
    match SchemaService::load(conn) {
        Ok(new_schema) => {
            *schema = new_schema.clone();
            rebuild(new_schema);
            writeln!(writer, "Schema refreshed.").ok();
        }
        Err(e) => eprintln!("error: could not refresh schema: {}", e),
    }
}

fn handle_sql(
    conn: &dyn ReplPort,
    query: &str,
    expanded: bool,
    timing: bool,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    writer: &mut impl Write,
) {
    let start = std::time::Instant::now();
    match conn.execute(query) {
        Ok(result) => {
            write!(writer, "{}", format_result(&result, expanded)).ok();
            if timing {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                if ms >= 1000.0 {
                    writeln!(writer, "Time: {:.3} s", ms / 1000.0).ok();
                } else {
                    writeln!(writer, "Time: {:.3} ms", ms).ok();
                }
            }
            if is_ddl(query)
                && let Ok(new_schema) = SchemaService::load(conn)
            {
                *schema = new_schema.clone();
                rebuild(new_schema);
                writeln!(writer, "(schema refreshed)").ok();
            }
        }
        Err(e) => eprintln!("error: {}", e),
    }
}

pub fn run(conn: Box<dyn ReplPort>, db_name: &str) -> Result<(), String> {
    let mut schema = SchemaService::load(conn.as_ref())?;
    let mut rl = build_reedline(schema.clone());

    let prompt = PgrsPrompt {
        db_name: db_name.to_string(),
    };

    println!(
        "Connected to '{}'. Type \\help for commands, \\q or Ctrl+D to exit.",
        db_name
    );

    let mut expanded = false;
    let mut timing = false;

    loop {
        match rl.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();
                let mut stdout = io::stdout();
                match trimmed {
                    "\\q" | "exit" => break,
                    "\\help" | "\\?" => println!("{}", repl_help_text()),
                    "\\dt" => handle_dt(&schema, &mut stdout),
                    "\\l" => handle_l(conn.as_ref(), expanded, &mut stdout),
                    "\\x" => {
                        expanded = !expanded;
                        println!(
                            "Expanded display is {}.",
                            if expanded { "on" } else { "off" }
                        );
                    }
                    "\\timing" => {
                        timing = !timing;
                        println!("Timing is {}.", if timing { "on" } else { "off" });
                    }
                    "\\refresh" => handle_refresh(conn.as_ref(), &mut schema, &mut |s| { rl = build_reedline(s); }, &mut stdout),
                    "" => {}
                    _ => {
                        if let Some(name) = trimmed.strip_prefix("\\d+ ") {
                            if let Err(e) = describe_table(conn.as_ref(), name, true, &mut stdout) {
                                eprintln!("error: {}", e);
                            }
                        } else if let Some(name) = trimmed.strip_prefix("\\d ") {
                            if let Err(e) = describe_table(conn.as_ref(), name, false, &mut stdout) {
                                eprintln!("error: {}", e);
                            }
                        } else if trimmed == "\\d+" {
                            println!("Usage: \\d+ <table>");
                        } else if trimmed == "\\d" {
                            println!("Usage: \\d <table>");
                        } else {
                            handle_sql(conn.as_ref(), trimmed, expanded, timing, &mut schema, &mut |s| { rl = build_reedline(s); }, &mut stdout)
                        }
                    }
                }
            }
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) | Ok(Signal::ExternalBreak(_)) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
    }

    println!("Bye.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::core::ports::db_connection::QueryResult;

    struct StubDb {
        columns: HashMap<String, Vec<String>>,
        result: Result<QueryResult, String>,
    }

    impl StubDb {
        fn ok(rows: Vec<Vec<String>>, cols: Vec<String>) -> Self {
            Self {
                columns: HashMap::new(),
                result: Ok(QueryResult { columns: cols, rows, rows_affected: None }),
            }
        }
        fn err(msg: &str) -> Self {
            Self { columns: HashMap::new(), result: Err(msg.to_string()) }
        }
        fn with_schema(tables: &[(&str, &[&str])]) -> Self {
            let mut columns = HashMap::new();
            for (table, cols) in tables {
                columns.insert(table.to_string(), cols.iter().map(|c| c.to_string()).collect());
            }
            Self { columns, result: Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: Some(0) }) }
        }
    }

    impl crate::core::ports::db_connection::DbConnection for StubDb {
        fn execute(&self, _query: &str) -> Result<QueryResult, String> {
            self.result.clone()
        }
    }

    impl crate::core::ports::schema_port::SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            Ok(self.columns.clone())
        }
    }

    fn schema_from(tables: &[(&str, &[&str])]) -> SchemaService {
        let stub = StubDb::with_schema(tables);
        SchemaService::load(&stub).unwrap()
    }

    #[test]
    fn handle_dt_prints_nothing_for_empty_schema() {
        let schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No tables."));
    }

    #[test]
    fn handle_dt_lists_table_names() {
        let schema = schema_from(&[("users", &["id", "email"]), ("orders", &["id"])]);
        let mut out = Vec::new();
        handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected 'users' in output, got: {text}");
        assert!(text.contains("orders"), "expected 'orders' in output, got: {text}");
    }

    #[test]
    fn handle_dt_shows_column_count() {
        let schema = schema_from(&[("users", &["id", "email"])]);
        let mut out = Vec::new();
        handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("2 columns"), "expected column count, got: {text}");
    }

    #[test]
    fn handle_l_output_includes_database_name() {
        let stub = StubDb::ok(
            vec![vec!["mydb".to_string()]],
            vec!["database".to_string()],
        );
        let mut out = Vec::new();
        handle_l(&stub, false, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("mydb"), "expected db name in output, got: {text}");
    }

    #[test]
    fn handle_l_handles_db_error_gracefully() {
        let stub = StubDb::err("connection lost");
        let mut out = Vec::new();
        handle_l(&stub, false, &mut out);
        // error goes to stderr, stdout output is empty — should not panic
    }

    #[test]
    fn handle_sql_executes_query_without_panic() {
        let stub = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 1", false, false, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt, "no DDL — schema should not be rebuilt");
    }

    #[test]
    fn handle_sql_output_includes_query_result() {
        let stub = StubDb::ok(vec![vec!["42".to_string()]], vec!["id".to_string()]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 42", false, false, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("42"), "expected result value in output, got: {text}");
    }

    #[test]
    fn handle_sql_rebuilds_schema_after_ddl() {
        let stub = StubDb::with_schema(&[("users", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "CREATE TABLE users (id int)", false, false, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(rebuilt, "DDL should trigger schema rebuild");
    }

    #[test]
    fn handle_sql_shows_schema_refreshed_after_ddl() {
        let stub = StubDb::with_schema(&[("users", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "CREATE TABLE users (id int)", false, false, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("schema refreshed"), "expected refresh notice, got: {text}");
    }

    #[test]
    fn handle_sql_does_not_rebuild_on_select() {
        let stub = StubDb::ok(vec![], vec![]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 1", false, false, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt);
    }

    #[test]
    fn handle_sql_handles_error_gracefully() {
        let stub = StubDb::err("syntax error");
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "SELEKT *", false, false, &mut schema, &mut |_| {}, &mut out);
    }

    #[test]
    fn handle_refresh_updates_schema() {
        let stub = StubDb::with_schema(&[("products", &["id", "name"])]);
        let mut schema = schema_from(&[]);
        assert!(schema.tables().is_empty());
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_refresh(&stub, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(rebuilt);
        assert!(schema.tables().contains(&"products".to_string()));
    }

    #[test]
    fn handle_refresh_prints_confirmation() {
        let stub = StubDb::with_schema(&[("t", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_refresh(&stub, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("refreshed"), "expected refresh confirmation, got: {text}");
    }

    #[test]
    fn handle_refresh_handles_error_gracefully() {
        struct FailingDb;
        impl crate::core::ports::schema_port::SchemaPort for FailingDb {
            fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
                Err("connection lost".to_string())
            }
        }
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_refresh(&FailingDb, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt, "failed refresh must not trigger rebuild");
    }

    #[test]
    fn prompt_left_includes_database_name() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
        };
        let left = prompt.render_prompt_left();
        assert!(
            left.contains("mydb"),
            "prompt should include db name, got: {left}"
        );
    }

    #[test]
    fn prompt_left_format_is_pgrs_parens_name() {
        let prompt = PgrsPrompt {
            db_name: "production".to_string(),
        };
        let left = prompt.render_prompt_left();
        assert_eq!(left.as_ref(), "pgrs (production)");
    }

    #[test]
    fn help_text_mentions_quit_command() {
        let text = repl_help_text();
        assert!(text.contains("\\q"), "help should mention \\q, got: {text}");
    }

    #[test]
    fn help_text_mentions_dt_command() {
        let text = repl_help_text();
        assert!(
            text.contains("\\dt"),
            "help should mention \\dt, got: {text}"
        );
    }

    #[test]
    fn help_text_mentions_help_command() {
        let text = repl_help_text();
        assert!(
            text.contains("\\help"),
            "help should mention \\help itself, got: {text}"
        );
    }

    #[test]
    fn help_text_mentions_exit_alias() {
        let text = repl_help_text();
        assert!(
            text.contains("exit"),
            "help should mention exit alias, got: {text}"
        );
    }

    #[test]
    fn help_text_mentions_x_command() {
        let text = repl_help_text();
        assert!(text.contains("\\x"), "help should mention \\x, got: {text}");
    }

    #[test]
    fn help_text_mentions_refresh_command() {
        let text = repl_help_text();
        assert!(
            text.contains("\\refresh"),
            "help should mention \\refresh, got: {text}"
        );
    }

    #[test]
    fn help_text_mentions_timing_command() {
        let text = repl_help_text();
        assert!(
            text.contains("\\timing"),
            "help should mention \\timing, got: {text}"
        );
    }

    #[test]
    fn is_ddl_detects_create() {
        assert!(is_ddl("CREATE TABLE foo (id int);"));
        assert!(is_ddl("create table foo (id int);"));
    }

    #[test]
    fn is_ddl_detects_drop() {
        assert!(is_ddl("DROP TABLE foo;"));
    }

    #[test]
    fn is_ddl_detects_alter() {
        assert!(is_ddl("ALTER TABLE foo ADD COLUMN bar text;"));
    }

    #[test]
    fn is_ddl_detects_truncate() {
        assert!(is_ddl("TRUNCATE TABLE foo;"));
    }

    #[test]
    fn is_ddl_returns_false_for_select() {
        assert!(!is_ddl("SELECT * FROM foo;"));
        assert!(!is_ddl("INSERT INTO foo VALUES (1);"));
        assert!(!is_ddl("UPDATE foo SET x = 1;"));
        assert!(!is_ddl("DELETE FROM foo;"));
    }

    // is_complete_statement — double-quoted identifier handling
    #[test]
    fn complete_unclosed_double_quoted_identifier_ending_with_semicolon_not_complete() {
        assert!(!is_complete_statement(r#"SELECT "col;"#));
    }

    #[test]
    fn complete_closed_double_quoted_identifier_then_semicolon_is_complete() {
        assert!(is_complete_statement(r#"SELECT "col;name" FROM t;"#));
    }

    #[test]
    fn complete_double_quoted_identifier_with_escaped_double_quote() {
        assert!(is_complete_statement(r#"SELECT "O""Brien" FROM t;"#));
    }

    #[test]
    fn complete_double_quote_inside_single_quote_does_not_open_identifier() {
        assert!(is_complete_statement(r#"SELECT '"quoted"' FROM t;"#));
    }

    #[test]
    fn complete_single_quote_inside_double_quote_does_not_open_string() {
        assert!(is_complete_statement(r#"SELECT "it's" FROM t;"#));
    }

    #[test]
    fn complete_no_semicolon_not_complete() {
        assert!(!is_complete_statement(r#"SELECT "col" FROM t"#));
    }

    #[test]
    fn help_text_mentions_l_command() {
        let text = repl_help_text();
        assert!(text.contains("\\l"), "help should mention \\l, got: {text}");
    }

    #[test]
    fn help_text_mentions_backslash_d() {
        let text = repl_help_text();
        assert!(text.contains("\\d"), "help should mention \\d, got: {text}");
    }
}
