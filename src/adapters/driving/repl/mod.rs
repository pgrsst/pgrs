mod alias;
mod commands;
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

use crate::core::ports::analytics_port::AnalyticsPort;
use crate::core::ports::repl_port::ReplPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::services::schema::service::SchemaService;

use describe::describe_table;

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
    let mut rl = ui::build_reedline(schema.clone());

    let prompt = ui::PgrsPrompt {
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
                    "\\help" | "\\?" => println!("{}", ui::repl_help_text()),
                    "\\dt" => commands::handle_dt(&schema, &mut stdout),
                    "\\l" => commands::handle_l(conn.as_ref(), expanded, &mut stdout),
                    "\\x" => {
                        expanded = !expanded;
                        println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                    }
                    "\\timing" => {
                        timing = !timing;
                        println!("Timing is {}.", if timing { "on" } else { "off" });
                    }
                    "\\refresh" => commands::handle_refresh(
                        conn.as_ref(),
                        connection_name,
                        &mut schema,
                        &mut |s| { rl = ui::build_reedline(s); },
                        schema_cache.as_deref(),
                        &mut stdout,
                    ),

                    "\\history" => {
                        match analytics.as_deref() {
                            Some(a) => commands::handle_history(connection_name, a, &mut stdout),
                            None => { writeln!(stdout, "Analytics not available.").ok(); }
                        }
                    }
                    "\\stats" => {
                        match analytics.as_deref() {
                            Some(a) => commands::handle_stats(connection_name, None, a, &mut stdout),
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
                            commands::handle_d(&schema, &mut stdout);
                        } else if let Some(tbl) = trimmed.strip_prefix("\\stats ") {
                            match analytics.as_deref() {
                                Some(a) => commands::handle_stats(connection_name, Some(tbl), a, &mut stdout),
                                None => { writeln!(stdout, "Analytics not available.").ok(); }
                            }
                        } else if trimmed == "\\export" {
                            writeln!(stdout, "Usage: \\export <id> <path>").ok();
                        } else if let Some(rest) = trimmed.strip_prefix("\\export ") {
                            match csv::parse_export_args(rest) {
                                None => { writeln!(stdout, "Usage: \\export <id> <path>").ok(); }
                                Some((id, path)) => match analytics.as_deref() {
                                    None => { writeln!(stdout, "Analytics not available.").ok(); }
                                    Some(a) => csv::handle_export(id, &path, connection_name, conn.as_ref(), a, &mut stdout),
                                }
                            }
                        } else {
                            commands::handle_sql(
                                conn.as_ref(),
                                trimmed,
                                &commands::SqlOptions {
                                    expanded,
                                    timing,
                                    connection_name,
                                    analytics: analytics.as_deref(),
                                    schema_cache: schema_cache.as_deref(),
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
