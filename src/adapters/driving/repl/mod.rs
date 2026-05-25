mod alias;
mod completer;
mod executor;
mod tokenizer;
mod describe;

use std::borrow::Cow;
use std::io::{self, Write};
use std::sync::Arc;

use reedline::{
    ColumnarMenu, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    ValidationResult, Validator, default_emacs_keybindings,
};

use crate::core::ports::analytics_port::AnalyticsPort;
use crate::core::ports::db_connection::{DbConnection, QueryResult};
use crate::core::ports::repl_port::ReplPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::ports::schema_port::SchemaPort;
use crate::core::services::schema::service::SchemaService;

use alias::extract_referenced_tables;
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
    environment: Option<String>,
}

impl Prompt for PgrsPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        match &self.environment {
            Some(env) => Cow::Owned(format!("pgrs({}:{})", self.db_name, env)),
            None => Cow::Owned(format!("pgrs({})", self.db_name)),
        }
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
    ("\\d",              "list all tables"),
    ("\\dt",             "list all tables with extended information (column count)"),
    ("\\d <table>",      "describe table (columns, indexes, constraints)"),
    ("\\d+ <table>",     "describe table (extended: + storage, triggers, comments)"),
    ("\\l",              "list databases"),
    ("\\x",              "toggle expanded display"),
    ("\\timing",         "toggle query execution time"),
    ("\\refresh",        "reload schema (after CREATE/DROP/ALTER TABLE)"),
    ("\\history",        "show recent query history"),
    ("\\stats",          "show most frequently queried tables"),
    ("\\stats <table>",  "show most frequently queried columns for table"),
    ("\\help, \\?",      "show this help"),
    ("\\q, exit",        "quit (or Ctrl+D)"),
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

fn is_dml(query: &str) -> bool {
    matches!(
        query
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_uppercase()
            .as_str(),
        "INSERT" | "UPDATE" | "DELETE"
    )
}

fn csv_quote(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

fn write_csv(result: &QueryResult, file: &mut impl Write) -> io::Result<()> {
    let header: Vec<String> = result.columns.iter().map(|c| csv_quote(c)).collect();
    writeln!(file, "{}", header.join(","))?;
    for row in &result.rows {
        let cells: Vec<String> = row.iter().map(|v| csv_quote(v)).collect();
        writeln!(file, "{}", cells.join(","))?;
    }
    Ok(())
}

fn handle_d(schema: &SchemaService, writer: &mut impl Write) {
    let tables = schema.tables();
    if tables.is_empty() {
        writeln!(writer, "No tables.").ok();
    } else {
        for table in tables {
            writeln!(writer, " {}", table).ok();
        }
    }
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

fn handle_history(connection_name: &str, analytics: &dyn AnalyticsPort, writer: &mut impl Write) {
    use chrono::{DateTime, Local, TimeZone};

    let history = analytics.get_history(connection_name);
    if history.is_empty() {
        writeln!(writer, "No query history.").ok();
        return;
    }
    let id_w = history.iter().map(|e| format!("{}", e.id).len()).max().unwrap_or(1);
    let q_w  = history.iter().map(|e| e.query.len()).max().unwrap_or(5);
    writeln!(writer, "  {:<id_w$}  {:<q_w$}  executed_at", "id", "query").ok();
    writeln!(writer, "  {:-<id_w$}  {:-<q_w$}  {:-<25}", "", "", "").ok();
    for entry in &history {
        let dt: DateTime<Local> = Local.timestamp_opt(entry.executed_at, 0).single()
            .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap());
        writeln!(
            writer,
            "  {:<id_w$}  {:<q_w$}  {}",
            entry.id,
            entry.query,
            dt.format("%Y-%m-%d %H:%M:%S %z"),
        ).ok();
    }
    writeln!(writer, "({} entries)", history.len()).ok();
}

fn handle_stats(
    connection_name: &str,
    table: Option<&str>,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
) {
    match table {
        None => {
            let freq = analytics.get_frequent_tables(connection_name);
            if freq.is_empty() {
                writeln!(writer, "No table statistics yet.").ok();
                return;
            }
            let name_w = freq.iter().map(|e| e.name.len()).max().unwrap_or(0);
            for entry in &freq {
                writeln!(writer, "  {:<name_w$}  {}", entry.name, entry.count).ok();
            }
        }
        Some(tbl) => {
            let freq = analytics.get_frequent_columns(connection_name, tbl);
            if freq.is_empty() {
                writeln!(writer, "No column statistics for '{}'.", tbl).ok();
                return;
            }
            let name_w = freq.iter().map(|e| e.name.len()).max().unwrap_or(0);
            for entry in &freq {
                writeln!(writer, "  {:<name_w$}  {}", entry.name, entry.count).ok();
            }
        }
    }
}

fn handle_export(
    id: i64,
    path: &str,
    connection_name: &str,
    conn: &dyn ReplPort,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
) {
    if std::path::Path::new(path).exists() {
        writeln!(writer, "error: file already exists: {}", path).ok();
        return;
    }
    let history = analytics.get_history(connection_name);
    let entry = match history.iter().find(|e| e.id == id) {
        Some(e) => e,
        None => {
            writeln!(writer, "error: no history entry with id {}", id).ok();
            return;
        }
    };
    if is_dml(&entry.query) || is_ddl(&entry.query) {
        writeln!(writer, "error: cannot export non-SELECT query").ok();
        return;
    }
    let result = match conn.execute(&entry.query) {
        Ok(r) => r,
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
            return;
        }
    };
    let mut file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            writeln!(writer, "error: could not write file: {}", e).ok();
            return;
        }
    };
    if let Err(e) = write_csv(&result, &mut file) {
        drop(file);
        std::fs::remove_file(path).ok();
        writeln!(writer, "error: could not write file: {}", e).ok();
        return;
    }
    writeln!(writer, "Exported {} rows to {}", result.rows.len(), path).ok();
}

fn extract_column_refs(query: &str, schema: &SchemaService) -> Vec<(String, String)> {
    use tokenizer::{SqlToken, tokenize};
    use alias::SQL_KEYWORDS;

    let mut in_select = false;
    let mut candidates: Vec<String> = Vec::new();

    for token in tokenize(query) {
        if let SqlToken::Word(w) = token {
            let upper = w.to_uppercase();
            if upper == "SELECT" { in_select = true; continue; }
            if upper == "FROM" { break; }
            if in_select && !SQL_KEYWORDS.contains(&upper.as_str()) && w != "*" {
                candidates.push(w.to_lowercase());
            }
        }
    }

    let mut refs = Vec::new();
    for col in candidates {
        for table in schema.tables() {
            if schema.columns_for(table).iter().any(|c| c == &col) {
                refs.push((table.to_string(), col.clone()));
                break;
            }
        }
    }
    refs
}

struct SqlOptions<'a> {
    expanded: bool,
    timing: bool,
    connection_name: &'a str,
    analytics: Option<&'a dyn AnalyticsPort>,
    schema_cache: Option<&'a dyn SchemaCachePort>,
}

fn handle_sql(
    conn: &dyn ReplPort,
    query: &str,
    opts: &SqlOptions<'_>,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    writer: &mut impl Write,
) {
    let start = std::time::Instant::now();
    match conn.execute(query) {
        Ok(result) => {
            write!(writer, "{}", format_result(&result, opts.expanded)).ok();
            if opts.timing {
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                if ms >= 1000.0 {
                    writeln!(writer, "Time: {:.3} s", ms / 1000.0).ok();
                } else {
                    writeln!(writer, "Time: {:.3} ms", ms).ok();
                }
            }

            if let Some(analytics) = opts.analytics {
                let tables = extract_referenced_tables(query);
                let columns = extract_column_refs(query, schema);
                analytics.record_query(opts.connection_name, query, &tables, &columns);
            }

            if is_ddl(query)
                && let Ok(new_schema) = SchemaService::load_with_cache(conn, opts.connection_name, opts.schema_cache)
            {
                *schema = new_schema.clone();
                rebuild(new_schema);
                writeln!(writer, "(schema refreshed)").ok();
            }
        }
        Err(e) => eprintln!("error: {}", e),
    }
}

fn handle_refresh(
    conn: &dyn SchemaPort,
    connection_name: &str,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    schema_cache: Option<&dyn SchemaCachePort>,
    writer: &mut impl Write,
) {
    if let Some(cache) = schema_cache {
        cache.invalidate(connection_name);
    }
    match SchemaService::load_with_cache(conn, connection_name, schema_cache) {
        Ok(new_schema) => {
            *schema = new_schema.clone();
            rebuild(new_schema);
            writeln!(writer, "Schema refreshed.").ok();
        }
        Err(e) => eprintln!("error: could not refresh schema: {}", e),
    }
}

pub fn run(
    conn: Box<dyn ReplPort>,
    db_name: &str,
    connection_name: &str,
    environment: Option<&str>,
    analytics: Option<Arc<dyn AnalyticsPort>>,
    schema_cache: Option<Arc<dyn SchemaCachePort>>,
) -> Result<(), String> {
    let mut schema = SchemaService::load_with_cache(
        conn.as_ref(),
        connection_name,
        schema_cache.as_deref(),
    )?;
    let mut rl = build_reedline(schema.clone());

    let prompt = PgrsPrompt {
        db_name: db_name.to_string(),
        environment: environment.map(|s| s.to_string()),
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
                        println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                    }
                    "\\timing" => {
                        timing = !timing;
                        println!("Timing is {}.", if timing { "on" } else { "off" });
                    }
                    "\\refresh" => handle_refresh(
                        conn.as_ref(),
                        connection_name,
                        &mut schema,
                        &mut |s| { rl = build_reedline(s); },
                        schema_cache.as_deref(),
                        &mut stdout,
                    ),
                    "\\history" => {
                        match analytics.as_deref() {
                            Some(a) => handle_history(connection_name, a, &mut stdout),
                            None => { writeln!(stdout, "Analytics not available.").ok(); }
                        }
                    }
                    "\\stats" => {
                        match analytics.as_deref() {
                            Some(a) => handle_stats(connection_name, None, a, &mut stdout),
                            None => { writeln!(stdout, "Analytics not available.").ok(); }
                        }
                    }
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
                            handle_d(&schema, &mut stdout);
                        } else if let Some(tbl) = trimmed.strip_prefix("\\stats ") {
                            match analytics.as_deref() {
                                Some(a) => handle_stats(connection_name, Some(tbl), a, &mut stdout),
                                None => { writeln!(stdout, "Analytics not available.").ok(); }
                            }
                        } else {
                            handle_sql(
                                conn.as_ref(),
                                trimmed,
                                &SqlOptions {
                                    expanded,
                                    timing,
                                    connection_name,
                                    analytics: analytics.as_deref(),
                                    schema_cache: schema_cache.as_deref(),
                                },
                                &mut schema,
                                &mut |s| { rl = build_reedline(s); },
                                &mut stdout,
                            );
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
        handle_sql(&stub, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt, "no DDL — schema should not be rebuilt");
    }

    #[test]
    fn handle_sql_output_includes_query_result() {
        let stub = StubDb::ok(vec![vec!["42".to_string()]], vec!["id".to_string()]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 42", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("42"), "expected result value in output, got: {text}");
    }

    #[test]
    fn handle_sql_rebuilds_schema_after_ddl() {
        let stub = StubDb::with_schema(&[("users", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "CREATE TABLE users (id int)", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(rebuilt, "DDL should trigger schema rebuild");
    }

    #[test]
    fn handle_sql_shows_schema_refreshed_after_ddl() {
        let stub = StubDb::with_schema(&[("users", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "CREATE TABLE users (id int)", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("schema refreshed"), "expected refresh notice, got: {text}");
    }

    #[test]
    fn handle_sql_does_not_rebuild_on_select() {
        let stub = StubDb::ok(vec![], vec![]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt);
    }

    #[test]
    fn handle_sql_handles_error_gracefully() {
        let stub = StubDb::err("syntax error");
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "SELEKT *", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| {}, &mut out);
    }

    #[test]
    fn handle_refresh_updates_schema() {
        let stub = StubDb::with_schema(&[("products", &["id", "name"])]);
        let mut schema = schema_from(&[]);
        assert!(schema.tables().is_empty());
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_refresh(&stub, "my-conn", &mut schema, &mut |_| { rebuilt = true; }, None, &mut out);
        assert!(rebuilt);
        assert!(schema.tables().contains(&"products".to_string()));
    }

    #[test]
    fn handle_refresh_prints_confirmation() {
        let stub = StubDb::with_schema(&[("t", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_refresh(&stub, "my-conn", &mut schema, &mut |_| {}, None, &mut out);
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
        handle_refresh(&FailingDb, "my-conn", &mut schema, &mut |_| { rebuilt = true; }, None, &mut out);
        assert!(!rebuilt, "failed refresh must not trigger rebuild");
    }

    #[test]
    fn prompt_left_with_environment_shows_env() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: Some("production".to_string()),
        };
        let left = prompt.render_prompt_left();
        assert_eq!(left.as_ref(), "pgrs(mydb:production)");
    }

    #[test]
    fn prompt_left_without_environment_omits_env() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: None,
        };
        let left = prompt.render_prompt_left();
        assert_eq!(left.as_ref(), "pgrs(mydb)");
    }

    #[test]
    fn prompt_left_includes_database_name() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: None,
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
            environment: None,
        };
        let left = prompt.render_prompt_left();
        assert_eq!(left.as_ref(), "pgrs(production)");
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
    fn handle_d_prints_nothing_for_empty_schema() {
        let schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_d(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No tables."));
    }

    #[test]
    fn handle_d_lists_table_names_without_column_count() {
        let schema = schema_from(&[("users", &["id", "email"]), ("orders", &["id"])]);
        let mut out = Vec::new();
        handle_d(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected 'users' in output, got: {text}");
        assert!(text.contains("orders"), "expected 'orders' in output, got: {text}");
        assert!(!text.contains("columns"), "handle_d should not show column count, got: {text}");
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

    use std::sync::RwLock;
    use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
    use crate::core::ports::analytics_port::AnalyticsPort;

    struct RecordingAnalytics {
        recorded: RwLock<Vec<(String, String)>>,
    }
    impl RecordingAnalytics {
        fn new() -> Self { Self { recorded: RwLock::new(vec![]) } }
    }
    impl AnalyticsPort for RecordingAnalytics {
        fn record_query(&self, connection_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
            self.recorded.write().unwrap().push((connection_name.to_string(), query.to_string()));
        }
        fn get_history(&self, _: &str) -> Vec<HistoryEntry> {
            vec![
                HistoryEntry { id: 2, query: "SELECT 1".to_string(), executed_at: 1000 },
                HistoryEntry { id: 1, query: "SELECT 2".to_string(), executed_at: 999 },
            ]
        }
        fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> {
            vec![FreqEntry { name: "users".to_string(), count: 5 }]
        }
        fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> {
            vec![FreqEntry { name: "email".to_string(), count: 3 }]
        }
    }

    struct FixedHistoryAnalytics {
        entries: Vec<HistoryEntry>,
    }
    impl FixedHistoryAnalytics {
        fn new(entries: Vec<HistoryEntry>) -> Self { Self { entries } }
    }
    impl AnalyticsPort for FixedHistoryAnalytics {
        fn record_query(&self, _: &str, _: &str, _: &[String], _: &[(String, String)]) {}
        fn get_history(&self, _: &str) -> Vec<HistoryEntry> { self.entries.clone() }
        fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> { vec![] }
        fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> { vec![] }
    }

    #[test]
    fn handle_sql_records_analytics_with_connection_name() {
        struct CapturingAnalytics {
            recorded: RwLock<Vec<(String, String)>>,
        }
        impl CapturingAnalytics {
            fn new() -> Self { Self { recorded: RwLock::new(vec![]) } }
        }
        impl AnalyticsPort for CapturingAnalytics {
            fn record_query(&self, connection_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
                self.recorded.write().unwrap().push((connection_name.to_string(), query.to_string()));
            }
            fn get_history(&self, _: &str) -> Vec<HistoryEntry> { vec![] }
            fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> { vec![] }
            fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> { vec![] }
        }

        let stub = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]);
        let analytics = CapturingAnalytics::new();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();

        handle_sql(
            &stub,
            "SELECT 1",
            &SqlOptions {
                expanded: false,
                timing: false,
                connection_name: "my-conn",
                analytics: Some(&analytics),
                schema_cache: None,
            },
            &mut schema,
            &mut |_| {},
            &mut out,
        );

        let recorded = analytics.recorded.read().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "my-conn", "connection_name must reach analytics, not db_name");
        assert_eq!(recorded[0].1, "SELECT 1");
    }

    #[test]
    fn handle_history_shows_queries() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_history("mydb", &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("SELECT 1"), "expected query in history, got: {text}");
        assert!(text.contains("SELECT 2"), "expected query in history, got: {text}");
    }

    #[test]
    fn handle_stats_no_table_shows_tables() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_stats("mydb", None, &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected table name, got: {text}");
        assert!(text.contains("5"), "expected count, got: {text}");
    }

    #[test]
    fn handle_stats_with_table_shows_columns() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_stats("mydb", Some("users"), &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("email"), "expected column name, got: {text}");
        assert!(text.contains("3"), "expected count, got: {text}");
    }

    #[test]
    fn csv_quote_plain_value_unchanged() {
        assert_eq!(csv_quote("hello"), "hello");
    }

    #[test]
    fn csv_quote_value_with_comma_is_quoted() {
        assert_eq!(csv_quote("a,b"), "\"a,b\"");
    }

    #[test]
    fn csv_quote_value_with_double_quote_is_escaped() {
        assert_eq!(csv_quote("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_quote_value_with_newline_is_quoted() {
        assert_eq!(csv_quote("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn write_csv_produces_header_and_rows() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec!["1".to_string(), "alice".to_string()],
                vec!["2".to_string(), "bob".to_string()],
            ],
            rows_affected: None,
        };
        let mut out = Vec::new();
        write_csv(&result, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "id,name\n1,alice\n2,bob\n");
    }

    #[test]
    fn write_csv_quotes_values_with_comma() {
        let result = QueryResult {
            columns: vec!["note".to_string()],
            rows: vec![vec!["a,b".to_string()]],
            rows_affected: None,
        };
        let mut out = Vec::new();
        write_csv(&result, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "note\n\"a,b\"\n");
    }

    #[test]
    fn write_csv_empty_result_writes_only_header() {
        let result = QueryResult {
            columns: vec!["id".to_string()],
            rows: vec![],
            rows_affected: None,
        };
        let mut out = Vec::new();
        write_csv(&result, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "id\n");
    }

    #[test]
    fn is_dml_detects_insert() {
        assert!(is_dml("INSERT INTO foo VALUES (1);"));
        assert!(is_dml("insert into foo values (1);"));
    }

    #[test]
    fn is_dml_detects_update() {
        assert!(is_dml("UPDATE foo SET x = 1;"));
    }

    #[test]
    fn is_dml_detects_delete() {
        assert!(is_dml("DELETE FROM foo;"));
    }

    #[test]
    fn is_dml_returns_false_for_select() {
        assert!(!is_dml("SELECT * FROM foo;"));
        assert!(!is_dml("WITH cte AS (SELECT 1) SELECT * FROM cte;"));
    }

    fn export_tmp_path(tag: &str) -> String {
        format!("/tmp/pgrs_export_{}_{}.csv", std::process::id(), tag)
    }

    #[test]
    fn handle_export_writes_csv_for_valid_id() {
        let path = export_tmp_path("happy");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(
            vec![vec!["1".to_string(), "alice".to_string()]],
            vec!["id".to_string(), "name".to_string()],
        );
        let analytics = FixedHistoryAnalytics::new(vec![
            HistoryEntry { id: 3, query: "SELECT id, name FROM users;".to_string(), executed_at: 1000 },
        ]);
        let mut out = Vec::new();
        handle_export(3, &path, "mydb", &stub, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("Exported 1 rows to"), "expected confirmation, got: {msg}");

        let csv = std::fs::read_to_string(&path).unwrap();
        assert_eq!(csv, "id,name\n1,alice\n");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn handle_export_errors_on_existing_file() {
        let path = export_tmp_path("exists");
        std::fs::write(&path, "existing").unwrap();

        let stub = StubDb::ok(vec![], vec![]);
        let analytics = FixedHistoryAnalytics::new(vec![
            HistoryEntry { id: 1, query: "SELECT 1;".to_string(), executed_at: 1000 },
        ]);
        let mut out = Vec::new();
        handle_export(1, &path, "mydb", &stub, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("file already exists"), "expected file-exists error, got: {msg}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing", "file must not be overwritten");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn handle_export_errors_on_unknown_id() {
        let path = export_tmp_path("unknown");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(vec![], vec![]);
        let analytics = FixedHistoryAnalytics::new(vec![
            HistoryEntry { id: 1, query: "SELECT 1;".to_string(), executed_at: 1000 },
        ]);
        let mut out = Vec::new();
        handle_export(999, &path, "mydb", &stub, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("no history entry with id 999"), "expected id-not-found error, got: {msg}");
        assert!(!std::path::Path::new(&path).exists(), "file must not be created");
    }

    #[test]
    fn handle_export_errors_on_dml_query() {
        let path = export_tmp_path("dml");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(vec![], vec![]);
        let analytics = FixedHistoryAnalytics::new(vec![
            HistoryEntry { id: 5, query: "INSERT INTO foo VALUES (1);".to_string(), executed_at: 1000 },
        ]);
        let mut out = Vec::new();
        handle_export(5, &path, "mydb", &stub, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("cannot export non-SELECT query"), "expected non-SELECT error, got: {msg}");
        assert!(!std::path::Path::new(&path).exists(), "file must not be created");
    }

    #[test]
    fn handle_export_errors_on_ddl_query() {
        let path = export_tmp_path("ddl");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(vec![], vec![]);
        let analytics = FixedHistoryAnalytics::new(vec![
            HistoryEntry { id: 6, query: "DROP TABLE foo;".to_string(), executed_at: 1000 },
        ]);
        let mut out = Vec::new();
        handle_export(6, &path, "mydb", &stub, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("cannot export non-SELECT query"), "expected non-SELECT error, got: {msg}");
        assert!(!std::path::Path::new(&path).exists(), "file must not be created");
    }
}
