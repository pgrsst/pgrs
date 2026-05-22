pub mod completer;
pub mod executor;

fn is_complete_statement(s: &str) -> bool {
    let s = s.trim_end();
    if !s.ends_with(';') {
        return false;
    }
    // Ensure the trailing ';' is outside any string literal.
    // Handles SQL '' escape for embedded single quotes.
    let mut in_string = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if in_string {
            if chars[i] == '\'' {
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    i += 2; // '' escape — skip both
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

use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;
use rustyline::config::{Builder, CompletionType};

use crate::core::ports::db_connection::DbConnection;
use crate::core::services::schema::service::SchemaService;

use completer::SqlCompleter;
use executor::print_result;

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
    let schema = SchemaService::load(conn.as_ref())?;
    let completer = SqlCompleter::new(schema);

    let config = Builder::new()
        .completion_type(CompletionType::List)
        .build();
    let mut rl: Editor<SqlCompleter, DefaultHistory> =
        Editor::with_config(config).map_err(|e| e.to_string())?;
    rl.set_helper(Some(completer));

    println!(
        "Connected to '{}'. Type \\q or Ctrl+D to exit. \\dt to list tables.",
        db_name
    );

    let mut pending = String::new();

    loop {
        let prompt = if pending.is_empty() {
            "pgrs> "
        } else {
            "   -> "
        };

        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();

                if trimmed == "\\q" || trimmed == "exit" {
                    break;
                }

                if trimmed == "\\dt" {
                    if let Some(helper) = rl.helper() {
                        for table in helper.schema().tables() {
                            println!(" {}", table);
                        }
                    }
                    continue;
                }

                if trimmed.is_empty() {
                    continue;
                }

                pending.push_str(&line);
                pending.push('\n');

                if is_complete_statement(&pending) {
                    let query = pending.trim().to_string();
                    pending.clear();
                    rl.add_history_entry(&query).ok();
                    match conn.execute(&query) {
                        Ok(result) => print_result(&result),
                        Err(e) => eprintln!("ERROR:  {}", e),
                    }
                }
            }
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => break,
            Err(e) => return Err(e.to_string()),
        }
    }

    println!("Bye.");
    Ok(())
}
