use std::borrow::Cow;
use std::collections::HashMap;

use reedline::{
    ColumnarMenu, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu,
    ValidationResult, Validator, default_emacs_keybindings,
};

use crate::core::services::schema::service::SchemaService;
use super::completer::{SqlCompleter, SqlHighlighter, SqlHinter};
use super::sql_utils::is_complete_statement;

pub(super) struct PgrsPrompt {
    pub(super) db_name: String,
    pub(super) environment: Option<String>,
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
pub(super) const REPL_COMMANDS: &[(&str, &str)] = &[
    ("\\d",                  "list all tables"),
    ("\\dt",                 "list all tables with extended information (column count)"),
    ("\\d <table>",          "describe table (columns, indexes, constraints)"),
    ("\\d+ <table>",         "describe table (extended: + storage, triggers, comments)"),
    ("\\l",                  "list databases"),
    ("\\x",                  "toggle expanded display"),
    ("\\timing",             "toggle query execution time"),
    ("\\refresh",            "reload schema (after CREATE/DROP/ALTER TABLE)"),
    ("\\history",            "show recent query history"),
    ("\\export <id> <path>", "export query result from history to CSV file"),
    ("\\stats",              "show most frequently queried tables"),
    ("\\stats <table>",      "show most frequently queried columns for table"),
    ("\\help, \\?",          "show this help"),
    ("\\q, exit",            "quit (or Ctrl+D)"),
];

pub(super) fn repl_help_text() -> String {
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

pub(super) fn build_reedline(
    schema: SchemaService,
    table_freq: HashMap<String, u64>,
    column_freq: HashMap<String, u64>,
) -> Reedline {
    let highlighter = SqlHighlighter::new(schema.clone());
    let hinter = SqlHinter::new(schema.clone(), table_freq.clone(), column_freq.clone());
    let completer = SqlCompleter::new(schema, table_freq, column_freq);

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn help_text_mentions_l_command() {
        let text = repl_help_text();
        assert!(text.contains("\\l"), "help should mention \\l, got: {text}");
    }

    #[test]
    fn help_text_mentions_backslash_d() {
        let text = repl_help_text();
        assert!(text.contains("\\d"), "help should mention \\d, got: {text}");
    }

    #[test]
    fn help_text_mentions_export_command() {
        let text = repl_help_text();
        assert!(text.contains("\\export"), "help should mention \\export, got: {text}");
    }
}
