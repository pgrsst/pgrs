mod command_handler;
mod completer;
mod csv;
mod executor;
mod describe;
mod sql_utils;
mod ui;

use std::collections::HashMap;
use std::io::{self, Write};

use reedline::Signal;

use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi};

use command_handler::{CommandHandler, SqlOptions};
use describe::describe_table;

fn freq_for_schema(
    analytics: &AnalyticsApi,
    conn_name: &str,
    schema: &SchemaApi,
) -> (HashMap<String, u64>, HashMap<String, u64>) {
    let table_freq = analytics.frequent_tables(conn_name).into_iter().collect();
    let column_freq = schema
        .tables()
        .iter()
        .flat_map(|t| analytics.frequent_columns(conn_name, t))
        .fold(HashMap::new(), |mut m, (name, count)| {
            let c = m.entry(name).or_insert(0u64);
            *c = (*c).max(count);
            m
        });
    (table_freq, column_freq)
}

pub struct Repl {
    query: QueryApi,
    db_name: String,
    connection_name: String,
    environment: Option<String>,
    analytics: AnalyticsApi,
    schema: SchemaApi,
    handler: CommandHandler,
}

impl Repl {
    pub fn new(
        query: QueryApi,
        db_name: &str,
        connection_name: &str,
        environment: Option<&str>,
        analytics: AnalyticsApi,
        schema: SchemaApi,
    ) -> Self {
        Self {
            query,
            db_name: db_name.to_string(),
            connection_name: connection_name.to_string(),
            environment: environment.map(|s| s.to_string()),
            analytics,
            schema,
            handler: CommandHandler,
        }
    }

    pub fn run(self) -> Result<(), String> {
        let Repl {
            query,
            db_name,
            connection_name,
            environment,
            analytics,
            mut schema,
            handler,
        } = self;

        schema.load(&query, &connection_name)?;
        let (table_freq, column_freq) = freq_for_schema(&analytics, &connection_name, &schema);
        let mut rl = ui::build_reedline(schema.clone(), table_freq, column_freq);

        let prompt = ui::PgrsPrompt {
            db_name: db_name.clone(),
            environment: environment.clone(),
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
                        "\\help" | "\\?" => println!("{}", ui::repl_help_text()),
                        "\\dt" => handler.handle_dt(&schema, &mut stdout),
                        "\\l" => handler.handle_l(&query, expanded, &mut stdout),
                        "\\x" => {
                            expanded = !expanded;
                            println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                        }
                        "\\timing" => {
                            timing = !timing;
                            println!("Timing is {}.", if timing { "on" } else { "off" });
                        }
                        "\\refresh" => handler.handle_refresh(
                            &query,
                            &connection_name,
                            &mut schema,
                            &mut |s: SchemaApi| {
                                let (tf, cf) = freq_for_schema(&analytics, &connection_name, &s);
                                rl = ui::build_reedline(s, tf, cf);
                            },
                            &mut stdout,
                        ),
                        "\\history" => handler.handle_history(&connection_name, &analytics, &mut stdout),
                        "\\stats" => handler.handle_stats(&connection_name, None, &analytics, &mut stdout),
                        "" => {}
                        _ => {
                            if let Some(name) = trimmed.strip_prefix("\\d+ ") {
                                if let Err(e) = describe_table(&query, name, true, &mut stdout) {
                                    eprintln!("error: {}", e);
                                }
                            } else if let Some(name) = trimmed.strip_prefix("\\d ") {
                                if let Err(e) = describe_table(&query, name, false, &mut stdout) {
                                    eprintln!("error: {}", e);
                                }
                            } else if trimmed == "\\d+" {
                                println!("Usage: \\d+ <table>");
                            } else if trimmed == "\\d" {
                                handler.handle_d(&schema, &mut stdout);
                            } else if let Some(tbl) = trimmed.strip_prefix("\\stats ") {
                                handler.handle_stats(&connection_name, Some(tbl), &analytics, &mut stdout);
                            } else if trimmed == "\\export" {
                                writeln!(stdout, "Usage: \\export <id> <path>").ok();
                            } else if let Some(rest) = trimmed.strip_prefix("\\export ") {
                                match csv::parse_export_args(rest) {
                                    None => { writeln!(stdout, "Usage: \\export <id> <path>").ok(); }
                                    Some((id, path)) => csv::handle_export(
                                        id, &path, &connection_name, &query, &analytics, &mut stdout,
                                    ),
                                }
                            } else {
                                handler.handle_sql(
                                    &query,
                                    trimmed,
                                    &SqlOptions {
                                        expanded,
                                        timing,
                                        connection_name: &connection_name,
                                        analytics: Some(&analytics),
                                    },
                                    &mut schema,
                                    &mut |s: SchemaApi| {
                                        let (tf, cf) = freq_for_schema(&analytics, &connection_name, &s);
                                        rl = ui::build_reedline(s, tf, cf);
                                    },
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
