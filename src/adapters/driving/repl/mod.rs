pub mod completer;
pub mod executor;

use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;

use crate::core::ports::db_connection::DbConnection;
use crate::core::services::schema::service::SchemaService;

use completer::SqlCompleter;
use executor::print_result;

pub fn run(conn: Box<dyn DbConnection>, db_name: &str) -> Result<(), String> {
    let schema = SchemaService::load(conn.as_ref())?;
    let completer = SqlCompleter::new(schema);

    let mut rl: Editor<SqlCompleter, DefaultHistory> =
        Editor::new().map_err(|e| e.to_string())?;
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

                rl.add_history_entry(&line).ok();
                pending.push_str(&line);
                pending.push('\n');

                if pending.trim_end().ends_with(';') {
                    let query = pending.trim().to_string();
                    pending.clear();
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
