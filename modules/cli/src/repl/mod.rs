mod command_handler;
mod completer;
mod csv;
mod executor;
mod describe;
mod sql_utils;
mod ui;

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use reedline::{Reedline, Signal};

use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi, TxState, next_tx_state, tx_effect};

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

/// Rebuild the reedline editor so completion/highlighting pick up a refreshed
/// schema and the latest access-frequency ordering.
fn rebuild_reedline(
    rl: &mut Reedline,
    analytics: &AnalyticsApi,
    connection_name: &str,
    schema: SchemaApi,
) {
    let (tf, cf) = freq_for_schema(analytics, connection_name, &schema);
    *rl = ui::build_reedline(schema, tf, cf);
}

/// A single line of REPL input, classified. Keeping the (order-sensitive)
/// backslash-command parsing here, separate from execution, makes the dispatch
/// loop a flat match and lets the parser be unit-tested in isolation.
enum ReplCommand<'a> {
    Empty,
    Quit,
    Help,
    ListTables,      // \dt
    ListTablesPlain, // \d
    ListDatabases,   // \l
    ToggleExpanded,  // \x
    ToggleTiming,    // \timing
    Refresh,         // \refresh
    History,         // \history
    Stats(Option<&'a str>),
    Describe { table: &'a str, extended: bool }, // \d <t> / \d+ <t>
    DescribeUsage,                               // \d+ with no table
    Export(Option<&'a str>),                     // None => bare \export
    Sql(&'a str),
}

impl<'a> ReplCommand<'a> {
    fn parse(trimmed: &'a str) -> ReplCommand<'a> {
        match trimmed {
            "" => ReplCommand::Empty,
            "\\q" | "exit" => ReplCommand::Quit,
            "\\help" | "\\?" => ReplCommand::Help,
            "\\dt" => ReplCommand::ListTables,
            "\\d" => ReplCommand::ListTablesPlain,
            "\\l" => ReplCommand::ListDatabases,
            "\\x" => ReplCommand::ToggleExpanded,
            "\\timing" => ReplCommand::ToggleTiming,
            "\\refresh" => ReplCommand::Refresh,
            "\\begin" => ReplCommand::Sql("BEGIN"),
            "\\commit" => ReplCommand::Sql("COMMIT"),
            "\\rollback" => ReplCommand::Sql("ROLLBACK"),
            "\\history" => ReplCommand::History,
            "\\stats" => ReplCommand::Stats(None),
            "\\d+" => ReplCommand::DescribeUsage,
            "\\export" => ReplCommand::Export(None),
            _ => {
                if let Some(t) = trimmed.strip_prefix("\\d+ ") {
                    ReplCommand::Describe { table: t, extended: true }
                } else if let Some(t) = trimmed.strip_prefix("\\d ") {
                    ReplCommand::Describe { table: t, extended: false }
                } else if let Some(t) = trimmed.strip_prefix("\\stats ") {
                    ReplCommand::Stats(Some(t))
                } else if let Some(rest) = trimmed.strip_prefix("\\export ") {
                    ReplCommand::Export(Some(rest))
                } else {
                    ReplCommand::Sql(trimmed)
                }
            }
        }
    }
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

        let tx = Arc::new(Mutex::new(TxState::Idle));

        let prompt = ui::PgrsPrompt {
            db_name: db_name.clone(),
            environment: environment.clone(),
            tx: Arc::clone(&tx),
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
                    match ReplCommand::parse(trimmed) {
                        ReplCommand::Empty => {}
                        ReplCommand::Quit => break,
                        ReplCommand::Help => println!("{}", ui::repl_help_text()),
                        ReplCommand::ListTables => handler.handle_dt(&schema, &mut stdout),
                        ReplCommand::ListTablesPlain => handler.handle_d(&schema, &mut stdout),
                        ReplCommand::ListDatabases => handler.handle_l(&query, &mut stdout),
                        ReplCommand::ToggleExpanded => {
                            expanded = !expanded;
                            println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                        }
                        ReplCommand::ToggleTiming => {
                            timing = !timing;
                            println!("Timing is {}.", if timing { "on" } else { "off" });
                        }
                        ReplCommand::Refresh => handler.handle_refresh(
                            &query,
                            &connection_name,
                            &mut schema,
                            &mut |s| rebuild_reedline(&mut rl, &analytics, &connection_name, s),
                            &mut stdout,
                        ),
                        ReplCommand::History => {
                            handler.handle_history(&connection_name, &analytics, &mut stdout)
                        }
                        ReplCommand::Stats(table) => {
                            handler.handle_stats(&connection_name, table, &analytics, &mut stdout)
                        }
                        ReplCommand::Describe { table, extended } => {
                            if let Err(e) = describe_table(&query, table, extended, &mut stdout) {
                                writeln!(stdout, "error: {}", e).ok();
                            }
                        }
                        ReplCommand::DescribeUsage => println!("Usage: \\d+ <table>"),
                        ReplCommand::Export(None) => {
                            writeln!(stdout, "Usage: \\export <id> <path>").ok();
                        }
                        ReplCommand::Export(Some(rest)) => match csv::parse_export_args(rest) {
                            None => {
                                writeln!(stdout, "Usage: \\export <id> <path>").ok();
                            }
                            Some((id, path)) => csv::handle_export(
                                id, &path, &connection_name, &query, &analytics, &mut stdout,
                            ),
                        },
                        ReplCommand::Sql(sql) => {
                            let ok = handler.handle_sql(
                                &query,
                                sql,
                                &SqlOptions {
                                    expanded,
                                    timing,
                                    connection_name: &connection_name,
                                    analytics: Some(&analytics),
                                },
                                &mut schema,
                                &mut |s| rebuild_reedline(&mut rl, &analytics, &connection_name, s),
                                &mut stdout,
                            );
                            let prev = *tx.lock().unwrap();
                            let next = next_tx_state(prev, tx_effect(sql), ok);
                            *tx.lock().unwrap() = next;
                            if prev == TxState::InTransaction && next == TxState::Failed {
                                writeln!(
                                    stdout,
                                    "Transaction aborted. Run \\rollback (or ROLLBACK) to recover."
                                ).ok();
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

#[cfg(test)]
mod tests {
    use super::ReplCommand;

    #[test]
    fn empty_line_is_empty() {
        assert!(matches!(ReplCommand::parse(""), ReplCommand::Empty));
    }

    #[test]
    fn quit_aliases() {
        assert!(matches!(ReplCommand::parse("\\q"), ReplCommand::Quit));
        assert!(matches!(ReplCommand::parse("exit"), ReplCommand::Quit));
    }

    #[test]
    fn bare_d_lists_tables_but_d_space_describes() {
        assert!(matches!(ReplCommand::parse("\\d"), ReplCommand::ListTablesPlain));
        assert!(matches!(
            ReplCommand::parse("\\d users"),
            ReplCommand::Describe { table: "users", extended: false }
        ));
    }

    #[test]
    fn d_plus_describes_extended_and_bare_is_usage() {
        assert!(matches!(ReplCommand::parse("\\d+"), ReplCommand::DescribeUsage));
        assert!(matches!(
            ReplCommand::parse("\\d+ orders"),
            ReplCommand::Describe { table: "orders", extended: true }
        ));
    }

    #[test]
    fn stats_with_and_without_table() {
        assert!(matches!(ReplCommand::parse("\\stats"), ReplCommand::Stats(None)));
        assert!(matches!(
            ReplCommand::parse("\\stats users"),
            ReplCommand::Stats(Some("users"))
        ));
    }

    #[test]
    fn export_bare_vs_args() {
        assert!(matches!(ReplCommand::parse("\\export"), ReplCommand::Export(None)));
        assert!(matches!(
            ReplCommand::parse("\\export 1 /tmp/out.csv"),
            ReplCommand::Export(Some("1 /tmp/out.csv"))
        ));
    }

    #[test]
    fn toggles_and_simple_commands() {
        assert!(matches!(ReplCommand::parse("\\x"), ReplCommand::ToggleExpanded));
        assert!(matches!(ReplCommand::parse("\\timing"), ReplCommand::ToggleTiming));
        assert!(matches!(ReplCommand::parse("\\dt"), ReplCommand::ListTables));
        assert!(matches!(ReplCommand::parse("\\l"), ReplCommand::ListDatabases));
        assert!(matches!(ReplCommand::parse("\\refresh"), ReplCommand::Refresh));
        assert!(matches!(ReplCommand::parse("\\history"), ReplCommand::History));
        assert!(matches!(ReplCommand::parse("\\help"), ReplCommand::Help));
        assert!(matches!(ReplCommand::parse("\\?"), ReplCommand::Help));
    }

    #[test]
    fn plain_sql_falls_through() {
        assert!(matches!(
            ReplCommand::parse("SELECT * FROM users;"),
            ReplCommand::Sql("SELECT * FROM users;")
        ));
    }

    #[test]
    fn unknown_backslash_is_treated_as_sql() {
        // Not a recognised command -> handed to the SQL executor, which surfaces
        // the error. Parser stays dumb; it does not guess.
        assert!(matches!(ReplCommand::parse("\\nope"), ReplCommand::Sql("\\nope")));
    }

    #[test]
    fn tx_command_aliases_map_to_sql() {
        assert!(matches!(ReplCommand::parse("\\begin"), ReplCommand::Sql("BEGIN")));
        assert!(matches!(ReplCommand::parse("\\commit"), ReplCommand::Sql("COMMIT")));
        assert!(matches!(ReplCommand::parse("\\rollback"), ReplCommand::Sql("ROLLBACK")));
    }
}
