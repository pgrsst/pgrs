mod completer;
mod executor;

use std::borrow::Cow;

use reedline::{
    ColumnarMenu, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu,
    Signal, ValidationResult, Validator, default_emacs_keybindings,
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
        if line.trim().is_empty() || is_complete_statement(line) {
            ValidationResult::Complete
        } else {
            ValidationResult::Incomplete
        }
    }
}

fn repl_help_text() -> &'static str {
    "  \\dt        list tables\n  \\help      show this help\n  \\q, exit   quit"
}

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
    let schema = SchemaService::load(conn.as_ref())?;
    let tables_for_dt: Vec<String> = schema.tables().to_vec();

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

    let mut rl = Reedline::create()
        .with_completer(Box::new(completer))
        .with_hinter(Box::new(hinter))
        .with_highlighter(Box::new(highlighter))
        .with_validator(Box::new(SqlValidator))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_quick_completions(true)
        .with_partial_completions(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    let prompt = PgrsPrompt { db_name: db_name.to_string() };

    println!(
        "Connected to '{}'. Type \\help for commands, \\q or Ctrl+D to exit.",
        db_name
    );

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
                    for table in &tables_for_dt {
                        println!(" {}", table);
                    }
                    continue;
                }
                if trimmed.is_empty() {
                    continue;
                }
                match conn.execute(trimmed) {
                    Ok(result) => print_result(&result),
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
        let prompt = PgrsPrompt { db_name: "mydb".to_string() };
        let left = prompt.render_prompt_left();
        assert!(left.contains("mydb"), "prompt should include db name, got: {left}");
    }

    #[test]
    fn prompt_left_format_is_pgrs_parens_name() {
        let prompt = PgrsPrompt { db_name: "production".to_string() };
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
        assert!(text.contains("\\dt"), "help should mention \\dt, got: {text}");
    }

    #[test]
    fn help_text_mentions_help_command() {
        let text = repl_help_text();
        assert!(text.contains("\\help"), "help should mention \\help itself, got: {text}");
    }

    #[test]
    fn help_text_mentions_exit_alias() {
        let text = repl_help_text();
        assert!(text.contains("exit"), "help should mention exit alias, got: {text}");
    }
}
