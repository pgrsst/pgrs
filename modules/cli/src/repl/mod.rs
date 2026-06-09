mod command_handler;
mod completer;
mod csv;
mod executor;
mod describe;
mod explain;
mod pager;
mod sql_utils;
mod ui;

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use reedline::{Reedline, Signal};

use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi, TxState, is_dml, next_tx_state, tx_effect};

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

/// True if `sql` is a row-mutating statement (INSERT/UPDATE/DELETE, including
/// CTE-wrapped DML) submitted with no open transaction. Such statements are
/// rejected so the user always retains a ROLLBACK escape hatch.
fn dml_requires_tx(state: TxState, sql: &str) -> bool {
    state == TxState::Idle && is_dml(sql)
}

/// True only for an explicit affirmative confirmation.
fn is_yes(input: &str) -> bool {
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Handle a quit request. Returns `true` if the REPL should exit. With an open
/// transaction, warns and asks for confirmation; on "yes" (or EOF, since we
/// can't keep prompting) it issues `ROLLBACK` and exits, otherwise it cancels.
fn handle_quit_request(
    query: &QueryApi,
    tx: &Arc<Mutex<TxState>>,
    writer: &mut impl Write,
) -> bool {
    if *tx.lock().unwrap() == TxState::Idle {
        return true;
    }
    writeln!(writer, "A transaction is in progress. Roll back and quit? [y/N]").ok();
    writer.flush().ok();

    let mut input = String::new();
    let confirmed = match io::stdin().read_line(&mut input) {
        Ok(0) | Err(_) => true, // EOF or read error: cannot keep asking — roll back and quit.
        Ok(_) => is_yes(&input),
    };

    if confirmed {
        if let Err(e) = query.execute("ROLLBACK") {
            writeln!(writer, "warning: rollback failed: {e}").ok();
        }
        *tx.lock().unwrap() = TxState::Idle;
        true
    } else {
        writeln!(writer, "Quit cancelled.").ok();
        false
    }
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
    Explain { sql: &'a str, analyze: bool },     // \explain <sql> / \explain+ <sql>
    ExplainUsage,                                // bare \explain / \explain+
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
            "\\explain" | "\\explain+" => ReplCommand::ExplainUsage,
            _ => {
                if let Some(sql) = trimmed.strip_prefix("\\explain+ ") {
                    ReplCommand::Explain { sql, analyze: true }
                } else if let Some(sql) = trimmed.strip_prefix("\\explain ") {
                    ReplCommand::Explain { sql, analyze: false }
                } else if let Some(t) = trimmed.strip_prefix("\\d+ ") {
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
                        ReplCommand::Quit => {
                            if handle_quit_request(&query, &tx, &mut stdout) {
                                break;
                            }
                        }
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
                        ReplCommand::ExplainUsage => {
                            writeln!(stdout, "Usage: \\explain <query>  (\\explain+ runs ANALYZE)").ok();
                        }
                        ReplCommand::Explain { sql, analyze } => {
                            if analyze && dml_requires_tx(*tx.lock().unwrap(), sql) {
                                writeln!(
                                    stdout,
                                    "error: \\explain+ runs ANALYZE which executes the statement; INSERT/UPDATE/DELETE requires an explicit transaction. Run BEGIN (or \\begin) first."
                                ).ok();
                                continue;
                            }
                            explain::handle_explain(&query, sql, analyze, &mut stdout);
                        }
                        ReplCommand::Sql(sql) => {
                            if dml_requires_tx(*tx.lock().unwrap(), sql) {
                                writeln!(
                                    stdout,
                                    "error: INSERT/UPDATE/DELETE requires an explicit transaction. Run BEGIN (or \\begin) first."
                                ).ok();
                                continue;
                            }
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
                Ok(Signal::CtrlC) | Ok(Signal::CtrlD) | Ok(Signal::ExternalBreak(_)) => {
                    let mut stdout = io::stdout();
                    if handle_quit_request(&query, &tx, &mut stdout) {
                        break;
                    }
                }
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

    #[test]
    fn quit_confirmation_accepts_yes_only() {
        assert!(super::is_yes("y"));
        assert!(super::is_yes("Y"));
        assert!(super::is_yes("yes"));
        assert!(super::is_yes("  Yes  "));
        assert!(!super::is_yes("n"));
        assert!(!super::is_yes(""));
        assert!(!super::is_yes("nope"));
    }

    #[test]
    fn dml_without_transaction_is_blocked() {
        use pgrs_core::TxState;
        assert!(super::dml_requires_tx(TxState::Idle, "INSERT INTO t VALUES (1)"));
        assert!(super::dml_requires_tx(TxState::Idle, "UPDATE t SET x = 1"));
        assert!(super::dml_requires_tx(TxState::Idle, "DELETE FROM t"));
    }

    #[test]
    fn cte_wrapped_dml_without_transaction_is_blocked() {
        use pgrs_core::TxState;
        assert!(super::dml_requires_tx(
            TxState::Idle,
            "WITH c AS (INSERT INTO t VALUES (1) RETURNING id) SELECT * FROM c"
        ));
    }

    #[test]
    fn dml_inside_transaction_is_allowed() {
        use pgrs_core::TxState;
        assert!(!super::dml_requires_tx(TxState::InTransaction, "INSERT INTO t VALUES (1)"));
        assert!(!super::dml_requires_tx(TxState::Failed, "DELETE FROM t"));
    }

    #[test]
    fn non_dml_is_never_blocked() {
        use pgrs_core::TxState;
        assert!(!super::dml_requires_tx(TxState::Idle, "SELECT * FROM t"));
        assert!(!super::dml_requires_tx(TxState::Idle, "CREATE TABLE t (id int)"));
        assert!(!super::dml_requires_tx(TxState::Idle, "BEGIN"));
    }

    #[test]
    fn explain_variants_parse() {
        assert!(matches!(ReplCommand::parse("\\explain"), ReplCommand::ExplainUsage));
        assert!(matches!(ReplCommand::parse("\\explain+"), ReplCommand::ExplainUsage));
        assert!(matches!(
            ReplCommand::parse("\\explain SELECT 1"),
            ReplCommand::Explain { sql: "SELECT 1", analyze: false }
        ));
        assert!(matches!(
            ReplCommand::parse("\\explain+ SELECT 1"),
            ReplCommand::Explain { sql: "SELECT 1", analyze: true }
        ));
    }
}
