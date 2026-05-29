mod alias;
mod command_handler;
mod completer;
mod csv;
mod executor;
mod tokenizer;
mod describe;
mod sql_utils;
mod ui;

use std::io::{self, Write};
use std::sync::Arc;

use reedline::Signal;

use crate::core::ports::repl_port::ReplPort;
use crate::core::services::analytics::service::AnalyticsSvc;
use crate::core::services::schema::service::SchemaService;
use crate::core::services::schema_cache::service::SchemaCacheSvc;

use command_handler::{CommandHandler, SqlOptions};
use describe::describe_table;

pub struct Repl {
    conn: Box<dyn ReplPort>,
    db_name: String,
    connection_name: String,
    environment: Option<String>,
    analytics: Option<Arc<dyn AnalyticsSvc>>,
    schema_cache: Option<Arc<dyn SchemaCacheSvc>>,
    handler: CommandHandler,
}

impl Repl {
    pub fn new(
        conn: Box<dyn ReplPort>,
        db_name: &str,
        connection_name: &str,
        environment: Option<&str>,
        analytics: Option<Arc<dyn AnalyticsSvc>>,
        schema_cache: Option<Arc<dyn SchemaCacheSvc>>,
    ) -> Self {
        Self {
            conn,
            db_name: db_name.to_string(),
            connection_name: connection_name.to_string(),
            environment: environment.map(|s| s.to_string()),
            analytics,
            schema_cache,
            handler: CommandHandler,
        }
    }

    pub fn run(self) -> Result<(), String> {
        let mut schema = SchemaService::new(self.schema_cache);
        schema.load(self.conn.as_ref(), &self.connection_name)?;
        let mut rl = ui::build_reedline(schema.clone());

        let prompt = ui::PgrsPrompt {
            db_name: self.db_name.clone(),
            environment: self.environment.clone(),
        };

        println!(
            "Connected to '{}'. Type \\help for commands, \\q or Ctrl+D to exit.",
            self.db_name
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
                        "\\help" | "\\?" => println!("{}", ui::repl_help_text()),
                        "\\dt" => self.handler.handle_dt(&schema, &mut stdout),
                        "\\l" => self.handler.handle_l(self.conn.as_ref(), expanded, &mut stdout),
                        "\\x" => {
                            expanded = !expanded;
                            println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                        }
                        "\\timing" => {
                            timing = !timing;
                            println!("Timing is {}.", if timing { "on" } else { "off" });
                        }
                        "\\refresh" => self.handler.handle_refresh(
                            self.conn.as_ref(),
                            &self.connection_name,
                            &mut schema,
                            &mut |s| { rl = ui::build_reedline(s); },
                            &mut stdout,
                        ),
                        "\\history" => {
                            match self.analytics.as_deref() {
                                Some(a) => self.handler.handle_history(&self.connection_name, a, &mut stdout),
                                None => { writeln!(stdout, "Analytics not available.").ok(); }
                            }
                        }
                        "\\stats" => {
                            match self.analytics.as_deref() {
                                Some(a) => self.handler.handle_stats(&self.connection_name, None, a, &mut stdout),
                                None => { writeln!(stdout, "Analytics not available.").ok(); }
                            }
                        }
                        "" => {}
                        _ => {
                            if let Some(name) = trimmed.strip_prefix("\\d+ ") {
                                if let Err(e) = describe_table(self.conn.as_ref(), name, true, &mut stdout) {
                                    eprintln!("error: {}", e);
                                }
                            } else if let Some(name) = trimmed.strip_prefix("\\d ") {
                                if let Err(e) = describe_table(self.conn.as_ref(), name, false, &mut stdout) {
                                    eprintln!("error: {}", e);
                                }
                            } else if trimmed == "\\d+" {
                                println!("Usage: \\d+ <table>");
                            } else if trimmed == "\\d" {
                                self.handler.handle_d(&schema, &mut stdout);
                            } else if let Some(tbl) = trimmed.strip_prefix("\\stats ") {
                                match self.analytics.as_deref() {
                                    Some(a) => self.handler.handle_stats(&self.connection_name, Some(tbl), a, &mut stdout),
                                    None => { writeln!(stdout, "Analytics not available.").ok(); }
                                }
                            } else if trimmed == "\\export" {
                                writeln!(stdout, "Usage: \\export <id> <path>").ok();
                            } else if let Some(rest) = trimmed.strip_prefix("\\export ") {
                                match csv::parse_export_args(rest) {
                                    None => { writeln!(stdout, "Usage: \\export <id> <path>").ok(); }
                                    Some((id, path)) => match self.analytics.as_deref() {
                                        None => { writeln!(stdout, "Analytics not available.").ok(); }
                                        Some(a) => csv::handle_export(id, &path, &self.connection_name, self.conn.as_ref(), a, &mut stdout),
                                    }
                                }
                            } else {
                                self.handler.handle_sql(
                                    self.conn.as_ref(),
                                    trimmed,
                                    &SqlOptions {
                                        expanded,
                                        timing,
                                        connection_name: &self.connection_name,
                                        analytics: self.analytics.as_deref(),
                                    },
                                    &mut schema,
                                    &mut |s| { rl = ui::build_reedline(s); },
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
}
