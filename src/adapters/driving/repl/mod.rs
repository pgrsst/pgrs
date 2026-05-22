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

use completer::{SqlCompleter, SqlHighlighter};
use executor::print_result;

fn is_complete_statement(s: &str) -> bool {
    let s = s.trim_end();
    if !s.ends_with(';') {
        return false;
    }
    let mut in_string = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if in_string {
            if chars[i] == '\'' {
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    i += 2;
                } else {
                    in_string = false;
                    i += 1;
                }
            } else {
                i += 1;
            }
        } else {
            if chars[i] == '\'' {
                in_string = true;
            }
            i += 1;
        }
    }
    !in_string
}

struct PgrsPrompt;

impl Prompt for PgrsPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("pgrs")
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

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
    let schema = SchemaService::load(conn.as_ref())?;
    let tables_for_dt: Vec<String> = schema.tables().to_vec();

    let highlighter = SqlHighlighter::new(schema.clone());
    let completer = SqlCompleter::new(schema);

    let menu = ColumnarMenu::default().with_name("completion_menu");

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let mut rl = Reedline::create()
        .with_completer(Box::new(completer))
        .with_highlighter(Box::new(highlighter))
        .with_validator(Box::new(SqlValidator))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    let prompt = PgrsPrompt;

    println!(
        "Connected to '{}'. Type \\q or Ctrl+D to exit. \\dt to list tables.",
        db_name
    );

    loop {
        match rl.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();
                if trimmed == "\\q" || trimmed == "exit" {
                    break;
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
                    Err(e) => eprintln!("ERROR:  {}", e),
                }
            }
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
    }

    println!("Bye.");
    Ok(())
}
