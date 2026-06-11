use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use reedline::{
    ColumnarMenu, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu,
    ValidationResult, Validator, default_emacs_keybindings,
};

use pgrs_core::{SchemaApi, TxState};
use super::completer::{SqlCompleter, SqlHighlighter, SqlHinter};
use super::sql_utils::is_complete_statement;

pub(super) struct PgrsPrompt {
    pub(super) db_name: String,
    pub(super) environment: Option<String>,
    /// Shared with the REPL loop, which updates it after each statement so the
    /// prompt reflects the current transaction status.
    pub(super) tx: Arc<Mutex<TxState>>,
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
        match *self.tx.lock().unwrap() {
            TxState::Idle => Cow::Borrowed("> "),
            TxState::InTransaction => Cow::Borrowed("*> "),
            TxState::Failed => Cow::Borrowed("!> "),
        }
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

/// Validator for the `\edit` multiline editor: always Incomplete so Enter
/// inserts a newline instead of submitting. Submission is driven by an explicit
/// `Alt+Enter -> Submit` keybinding (see `build_editor_reedline`).
struct AlwaysIncomplete;

impl Validator for AlwaysIncomplete {
    fn validate(&self, _line: &str) -> ValidationResult {
        ValidationResult::Incomplete
    }
}

/// Prompt for the `\edit` editor — visually distinct from the main prompt so the
/// user knows Enter inserts a newline and Alt+Enter submits.
pub(super) struct EditorPrompt;

impl Prompt for EditorPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("edit> ")
    }
    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("   -> ")
    }
    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
}

/// Build a reedline configured as a multiline SQL editor for `\edit`: Enter
/// inserts a newline (always-Incomplete validator), `Alt+Enter` submits, `Esc`
/// cancels. Reuses the same completion/highlighting/hinting as the main prompt.
pub(super) fn build_editor_reedline(
    schema: SchemaApi,
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
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
    // Alt+Enter submits the whole buffer (bypasses the always-Incomplete validator).
    keybindings.add_binding(KeyModifiers::ALT, KeyCode::Enter, ReedlineEvent::Submit);
    // Esc cancels the edit (same outcome as Ctrl+C: a CtrlC signal). Trade-off:
    // this shadows menu dismissal, so Esc with the completion menu open cancels
    // the whole edit rather than just closing the menu.
    keybindings.add_binding(KeyModifiers::NONE, KeyCode::Esc, ReedlineEvent::CtrlC);

    Reedline::create()
        .with_completer(Box::new(completer))
        .with_hinter(Box::new(hinter))
        .with_highlighter(Box::new(highlighter))
        .with_validator(Box::new(AlwaysIncomplete))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_quick_completions(true)
        .with_partial_completions(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
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
    ("\\explain <query>",    "show query plan as a tree (\\explain+ runs ANALYZE)"),
    ("\\pager",              "toggle paging long output through $PAGER (default on)"),
    ("\\edit, \\e",          "open a multiline editor (Alt+Enter runs, Esc cancels)"),
    ("\\refresh",            "reload schema (after CREATE/DROP/ALTER TABLE)"),
    ("\\history",            "show recent query history"),
    ("\\export <id> <path>", "export query result from history to CSV file"),
    ("\\save <name> <id>",   "save a query from history (by id) under a name"),
    ("\\saved",              "list saved queries for this connection"),
    ("\\run <name>",         "run a saved query"),
    ("\\unsave <name>",      "delete a saved query"),
    ("\\stats",              "show most frequently queried tables"),
    ("\\stats <table>",      "show most frequently queried columns for table"),
    ("\\begin",              "begin a transaction (BEGIN)"),
    ("\\commit",             "commit the current transaction (COMMIT)"),
    ("\\rollback",           "roll back the current transaction (ROLLBACK)"),
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
           multi-line statement until the ';').\n\
           INSERT/UPDATE/DELETE require an open transaction — run BEGIN (\\begin) first.\n\n\
         {commands}"
    )
}

pub(super) fn build_reedline(
    schema: SchemaApi,
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
    use std::sync::{Arc, Mutex};
    use pgrs_core::TxState;

    fn prompt_with_tx(state: TxState) -> PgrsPrompt {
        PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: None,
            tx: Arc::new(Mutex::new(state)),
        }
    }

    #[test]
    fn indicator_is_plain_when_idle() {
        let p = prompt_with_tx(TxState::Idle);
        assert_eq!(p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(), "> ");
    }

    #[test]
    fn indicator_marks_open_transaction() {
        let p = prompt_with_tx(TxState::InTransaction);
        assert_eq!(p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(), "*> ");
    }

    #[test]
    fn indicator_marks_failed_transaction() {
        let p = prompt_with_tx(TxState::Failed);
        assert_eq!(p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(), "!> ");
    }

    #[test]
    fn prompt_left_with_environment_shows_env() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: Some("production".to_string()),
            tx: Arc::new(Mutex::new(TxState::Idle)),
        };
        let left = prompt.render_prompt_left();
        assert_eq!(left.as_ref(), "pgrs(mydb:production)");
    }

    #[test]
    fn prompt_left_without_environment_omits_env() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: None,
            tx: Arc::new(Mutex::new(TxState::Idle)),
        };
        let left = prompt.render_prompt_left();
        assert_eq!(left.as_ref(), "pgrs(mydb)");
    }

    #[test]
    fn prompt_left_includes_database_name() {
        let prompt = PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: None,
            tx: Arc::new(Mutex::new(TxState::Idle)),
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
            tx: Arc::new(Mutex::new(TxState::Idle)),
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

    #[test]
    fn help_text_mentions_transaction_commands() {
        let text = repl_help_text();
        assert!(text.contains("\\begin"), "help should mention \\begin, got: {text}");
        assert!(text.contains("\\commit"), "help should mention \\commit, got: {text}");
        assert!(text.contains("\\rollback"), "help should mention \\rollback, got: {text}");
    }

    #[test]
    fn help_text_mentions_explain_command() {
        let text = repl_help_text();
        assert!(text.contains("\\explain"), "help should mention \\explain, got: {text}");
    }

    #[test]
    fn help_text_mentions_saved_query_commands() {
        let text = repl_help_text();
        assert!(text.contains("\\save"), "help should mention \\save, got: {text}");
        assert!(text.contains("\\saved"), "help should mention \\saved, got: {text}");
        assert!(text.contains("\\run"), "help should mention \\run, got: {text}");
        assert!(text.contains("\\unsave"), "help should mention \\unsave, got: {text}");
    }

    #[test]
    fn help_text_mentions_pager_command() {
        let text = repl_help_text();
        assert!(text.contains("\\pager"), "help should mention \\pager, got: {text}");
    }

    #[test]
    fn help_text_mentions_edit_command() {
        let text = repl_help_text();
        assert!(text.contains("\\edit"), "help should mention \\edit, got: {text}");
    }

    #[test]
    fn help_mentions_transaction_requirement_for_dml() {
        let text = repl_help_text();
        assert!(
            text.contains("INSERT/UPDATE/DELETE"),
            "help should explain DML needs a transaction, got: {text}"
        );
    }

    #[test]
    fn always_incomplete_validator_never_completes() {
        let v = AlwaysIncomplete;
        assert!(matches!(v.validate("SELECT 1;"), ValidationResult::Incomplete));
        assert!(matches!(v.validate(""), ValidationResult::Incomplete));
    }

    #[test]
    fn editor_prompt_indicator_is_edit() {
        let p = EditorPrompt;
        assert_eq!(
            p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(),
            "edit> "
        );
    }
}
