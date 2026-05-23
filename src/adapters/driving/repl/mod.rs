mod completer;
mod executor;

use std::borrow::Cow;

use reedline::{
    ColumnarMenu, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    ValidationResult, Validator, default_emacs_keybindings,
};

use crate::core::ports::db_connection::DbConnection;
use crate::core::services::schema::service::SchemaService;

use completer::{SqlCompleter, SqlHighlighter, SqlHinter};
use executor::print_result;

fn is_complete_statement(s: &str) -> bool {
    let s = s.trim_end();
    if !s.ends_with(';') {
        return false;
    }
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    in_string = false;
                }
            }
        } else if c == '\'' {
            in_string = true;
        }
    }
    !in_string
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

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
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
                if trimmed == "\\q" || trimmed == "exit" {
                    break;
                }
                if trimmed == "\\help" || trimmed == "\\?" {
                    println!("{}", repl_help_text());
                    continue;
                }
                if trimmed == "\\dt" {
                    let tables = schema.tables();
                    if tables.is_empty() {
                        println!("No tables.");
                    } else {
                        let name_w = tables.iter().map(|t| t.len()).max().unwrap_or(0);
                        for table in tables {
                            let col_count = schema.columns_for(table).len();
                            println!(" {:<name_w$}  ({} columns)", table, col_count);
                        }
                    }
                    continue;
                }
                if trimmed == "\\x" {
                    expanded = !expanded;
                    println!(
                        "Expanded display is {}.",
                        if expanded { "on" } else { "off" }
                    );
                    continue;
                }
                if trimmed == "\\timing" {
                    timing = !timing;
                    println!("Timing is {}.", if timing { "on" } else { "off" });
                    continue;
                }
                if trimmed == "\\refresh" {
                    match SchemaService::load(conn.as_ref()) {
                        Ok(new_schema) => {
                            schema = new_schema;
                            rl = build_reedline(schema.clone());
                            println!("Schema refreshed.");
                        }
                        Err(e) => eprintln!("error: could not refresh schema: {}", e),
                    }
                    continue;
                }
                if trimmed.is_empty() {
                    continue;
                }
                let start = std::time::Instant::now();
                match conn.execute(trimmed) {
                    Ok(result) => {
                        print_result(&result, expanded);
                        if timing {
                            let ms = start.elapsed().as_secs_f64() * 1000.0;
                            if ms >= 1000.0 {
                                println!("Time: {:.3} s", ms / 1000.0);
                            } else {
                                println!("Time: {:.3} ms", ms);
                            }
                        }
                        if is_ddl(trimmed)
                            && let Ok(new_schema) = SchemaService::load(conn.as_ref())
                        {
                            schema = new_schema;
                            rl = build_reedline(schema.clone());
                            println!("(schema refreshed)");
                        }
                    }
                    Err(e) => eprintln!("error: {}", e),
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
}
