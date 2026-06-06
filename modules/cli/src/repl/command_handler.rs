use std::io::Write;

use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi, is_ddl};
use super::executor::format_result;

/// Print a `name -> count` frequency table, left-aligned to the widest name.
fn print_freq(freq: &[(String, u64)], writer: &mut impl Write) {
    let name_w = freq.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    for (name, count) in freq {
        writeln!(writer, "  {:<name_w$}  {}", name, count).ok();
    }
}

pub(super) struct CommandHandler;

impl CommandHandler {
    pub(super) fn handle_d(&self, schema: &SchemaApi, writer: &mut impl Write) {
        let tables = schema.tables();
        if tables.is_empty() {
            writeln!(writer, "No tables.").ok();
        } else {
            for table in tables {
                writeln!(writer, " {}", table).ok();
            }
        }
    }

    pub(super) fn handle_dt(&self, schema: &SchemaApi, writer: &mut impl Write) {
        let tables = schema.tables();
        if tables.is_empty() {
            writeln!(writer, "No tables.").ok();
        } else {
            let name_w = tables.iter().map(|t| t.len()).max().unwrap_or(0);
            for table in tables {
                let col_count = schema.columns_for(table).len();
                writeln!(writer, " {:<name_w$}  ({} columns)", table, col_count).ok();
            }
        }
    }

    pub(super) fn handle_l(&self, query: &QueryApi, writer: &mut impl Write) {
        match query.list_databases() {
            Ok(databases) => {
                for name in &databases {
                    writeln!(writer, " {}", name).ok();
                }
                let n = databases.len();
                writeln!(writer, "({} {})", n, if n == 1 { "database" } else { "databases" }).ok();
            }
            Err(e) => {
                writeln!(writer, "error: {}", e).ok();
            }
        }
    }

    pub(super) fn handle_history(&self, connection_name: &str, analytics: &AnalyticsApi, writer: &mut impl Write) {
        use chrono::{DateTime, Local, TimeZone};

        let history = analytics.history(connection_name);
        if history.is_empty() {
            writeln!(writer, "No query history.").ok();
            return;
        }
        let id_w = history.iter().map(|e| format!("{}", e.id).len()).max().unwrap_or(1);
        let q_w  = history.iter().map(|e| e.query.len()).max().unwrap_or(5);
        writeln!(writer, "  {:<id_w$}  {:<q_w$}  executed_at", "id", "query").ok();
        writeln!(writer, "  {:-<id_w$}  {:-<q_w$}  {:-<25}", "", "", "").ok();
        for entry in &history {
            let dt: DateTime<Local> = Local.timestamp_opt(entry.executed_at, 0).single()
                .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap());
            writeln!(
                writer,
                "  {:<id_w$}  {:<q_w$}  {}",
                entry.id,
                entry.query,
                dt.format("%Y-%m-%d %H:%M:%S %z"),
            ).ok();
        }
        writeln!(writer, "({} entries)", history.len()).ok();
    }

    pub(super) fn handle_stats(
        &self,
        connection_name: &str,
        table: Option<&str>,
        analytics: &AnalyticsApi,
        writer: &mut impl Write,
    ) {
        match table {
            None => {
                let freq = analytics.frequent_tables(connection_name);
                if freq.is_empty() {
                    writeln!(writer, "No table statistics yet.").ok();
                    return;
                }
                print_freq(&freq, writer);
            }
            Some(tbl) => {
                let freq = analytics.frequent_columns(connection_name, tbl);
                if freq.is_empty() {
                    writeln!(writer, "No column statistics for '{}'.", tbl).ok();
                    return;
                }
                print_freq(&freq, writer);
            }
        }
    }

    pub(super) fn handle_sql(
        &self,
        query_api: &QueryApi,
        query: &str,
        opts: &SqlOptions<'_>,
        schema: &mut SchemaApi,
        rebuild: &mut impl FnMut(SchemaApi),
        writer: &mut impl Write,
    ) {
        let start = std::time::Instant::now();
        match query_api.execute(query) {
            Ok(result) => {
                write!(writer, "{}", format_result(&result, opts.expanded)).ok();
                if opts.timing {
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    if ms >= 1000.0 {
                        writeln!(writer, "Time: {:.3} s", ms / 1000.0).ok();
                    } else {
                        writeln!(writer, "Time: {:.3} ms", ms).ok();
                    }
                }

                if let Some(analytics) = opts.analytics
                    && let Err(e) = analytics.record_query(opts.connection_name, query, schema)
                {
                    writeln!(writer, "pgrs: analytics write failed: {e}").ok();
                }

                if is_ddl(query) {
                    match schema.refresh(query_api, opts.connection_name) {
                        Ok(()) => {
                            rebuild(schema.clone());
                            writeln!(writer, "(schema refreshed)").ok();
                        }
                        Err(e) => { writeln!(writer, "error: could not refresh schema: {e}").ok(); }
                    }
                }
            }
            Err(e) => { writeln!(writer, "error: {}", e).ok(); }
        }
    }

    pub(super) fn handle_refresh(
        &self,
        query_api: &QueryApi,
        connection_name: &str,
        schema: &mut SchemaApi,
        rebuild: &mut impl FnMut(SchemaApi),
        writer: &mut impl Write,
    ) {
        match schema.refresh(query_api, connection_name) {
            Ok(()) => {
                rebuild(schema.clone());
                writeln!(writer, "Schema refreshed.").ok();
            }
            Err(e) => { writeln!(writer, "error: could not refresh schema: {e}").ok(); }
        }
    }
}

pub(super) struct SqlOptions<'a> {
    pub(super) expanded: bool,
    pub(super) timing: bool,
    pub(super) connection_name: &'a str,
    pub(super) analytics: Option<&'a AnalyticsApi>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use pgrs_core::{
        AddConnectionInput, Core, DbConnection, DomainError, QueryApi, QueryResult, SchemaApi,
        SchemaPort, TlsMode, DEFAULT_PORT,
    };

    struct StubDb {
        columns: HashMap<String, Vec<String>>,
        result: Result<QueryResult, DomainError>,
    }

    impl StubDb {
        fn ok(rows: Vec<Vec<String>>, cols: Vec<String>) -> Self {
            Self {
                columns: HashMap::new(),
                result: Ok(QueryResult { columns: cols, rows, rows_affected: None }),
            }
        }
        fn err(msg: &str) -> Self {
            Self { columns: HashMap::new(), result: Err(DomainError::QueryError(msg.to_string())) }
        }
        fn with_schema(tables: &[(&str, &[&str])]) -> Self {
            let mut columns = HashMap::new();
            for (table, cols) in tables {
                columns.insert(table.to_string(), cols.iter().map(|c| c.to_string()).collect());
            }
            Self { columns, result: Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: Some(0) }) }
        }
        fn into_query(self) -> QueryApi {
            QueryApi::from_repl(Box::new(self))
        }
    }

    impl DbConnection for StubDb {
        fn execute(&self, _query: &str) -> Result<QueryResult, DomainError> {
            self.result.clone()
        }
    }

    impl SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
            Ok(self.columns.clone())
        }
    }

    fn schema_from(tables: &[(&str, &[&str])]) -> SchemaApi {
        let mut col_map: HashMap<String, Vec<String>> = HashMap::new();
        for (table, cols) in tables {
            col_map.insert(table.to_string(), cols.iter().map(|c| c.to_string()).collect());
        }
        SchemaApi::for_test(col_map)
    }

    /// In-memory `Core` seeded with a connection, so analytics records persist.
    fn core_with_connection(name: &str) -> Core {
        let core = Core::in_memory();
        core.connection
            .add(AddConnectionInput {
                name: name.to_string(),
                host: "localhost".to_string(),
                port: DEFAULT_PORT,
                username: "u".to_string(),
                password: "p".to_string(),
                database: "db".to_string(),
                tls: TlsMode::Disable,
                environment: None,
            })
            .unwrap();
        core
    }

    fn handler() -> CommandHandler { CommandHandler }

    #[test]
    fn handle_dt_prints_nothing_for_empty_schema() {
        let schema = schema_from(&[]);
        let mut out = Vec::new();
        handler().handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No tables."));
    }

    #[test]
    fn handle_dt_lists_table_names() {
        let schema = schema_from(&[("users", &["id", "email"]), ("orders", &["id"])]);
        let mut out = Vec::new();
        handler().handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected 'users' in output, got: {text}");
        assert!(text.contains("orders"), "expected 'orders' in output, got: {text}");
    }

    #[test]
    fn handle_dt_shows_column_count() {
        let schema = schema_from(&[("users", &["id", "email"])]);
        let mut out = Vec::new();
        handler().handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("2 columns"), "expected column count, got: {text}");
    }

    #[test]
    fn handle_l_output_includes_database_name() {
        let query = StubDb::ok(
            vec![vec!["mydb".to_string()]],
            vec!["database".to_string()],
        ).into_query();
        let mut out = Vec::new();
        handler().handle_l(&query, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("mydb"), "expected db name in output, got: {text}");
    }

    #[test]
    fn handle_l_handles_db_error_gracefully() {
        let query = StubDb::err("connection lost").into_query();
        let mut out = Vec::new();
        handler().handle_l(&query, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("error"), "expected error written to output, got: {text}");
    }

    #[test]
    fn handle_sql_executes_query_without_panic() {
        let query = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]).into_query();
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handler().handle_sql(&query, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt, "no DDL — schema should not be rebuilt");
    }

    #[test]
    fn handle_sql_output_includes_query_result() {
        let query = StubDb::ok(vec![vec!["42".to_string()]], vec!["id".to_string()]).into_query();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handler().handle_sql(&query, "SELECT 42", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("42"), "expected result value in output, got: {text}");
    }

    #[test]
    fn handle_sql_rebuilds_schema_after_ddl() {
        let query = StubDb::with_schema(&[("users", &["id"])]).into_query();
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handler().handle_sql(&query, "CREATE TABLE users (id int)", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(rebuilt, "DDL should trigger schema rebuild");
    }

    #[test]
    fn handle_sql_shows_schema_refreshed_after_ddl() {
        let query = StubDb::with_schema(&[("users", &["id"])]).into_query();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handler().handle_sql(&query, "CREATE TABLE users (id int)", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("schema refreshed"), "expected refresh notice, got: {text}");
    }

    #[test]
    fn handle_sql_does_not_rebuild_on_select() {
        let query = StubDb::ok(vec![], vec![]).into_query();
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handler().handle_sql(&query, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt);
    }

    #[test]
    fn handle_sql_handles_error_gracefully() {
        let query = StubDb::err("syntax error").into_query();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handler().handle_sql(&query, "SELEKT *", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| {}, &mut out);
    }

    #[test]
    fn handle_refresh_updates_schema() {
        let query = StubDb::with_schema(&[("products", &["id", "name"])]).into_query();
        let mut schema = schema_from(&[]);
        assert!(schema.tables().is_empty());
        let mut rebuilt = false;
        let mut out = Vec::new();
        handler().handle_refresh(&query, "my-conn", &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(rebuilt);
        assert!(schema.tables().contains(&"products".to_string()));
    }

    #[test]
    fn handle_refresh_prints_confirmation() {
        let query = StubDb::with_schema(&[("t", &["id"])]).into_query();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handler().handle_refresh(&query, "my-conn", &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("refreshed"), "expected refresh confirmation, got: {text}");
    }

    #[test]
    fn handle_refresh_handles_error_gracefully() {
        // A connection whose schema fetch (list_columns) fails.
        struct FailingDb;
        impl DbConnection for FailingDb {
            fn execute(&self, _q: &str) -> Result<QueryResult, DomainError> {
                Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: None })
            }
        }
        impl SchemaPort for FailingDb {
            fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
                Err(DomainError::QueryError("connection lost".to_string()))
            }
        }
        let query = QueryApi::from_repl(Box::new(FailingDb));
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handler().handle_refresh(&query, "my-conn", &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt, "failed refresh must not trigger rebuild");
    }

    #[test]
    fn handle_d_prints_nothing_for_empty_schema() {
        let schema = schema_from(&[]);
        let mut out = Vec::new();
        handler().handle_d(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No tables."));
    }

    #[test]
    fn handle_d_lists_table_names_without_column_count() {
        let schema = schema_from(&[("users", &["id", "email"]), ("orders", &["id"])]);
        let mut out = Vec::new();
        handler().handle_d(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected 'users' in output, got: {text}");
        assert!(text.contains("orders"), "expected 'orders' in output, got: {text}");
        assert!(!text.contains("columns"), "handle_d should not show column count, got: {text}");
    }

    #[test]
    fn handle_sql_records_analytics_with_connection_name() {
        let query = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]).into_query();
        let core = core_with_connection("my-conn");
        let analytics = core.analytics_api();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();

        handler().handle_sql(
            &query,
            "SELECT 1",
            &SqlOptions {
                expanded: false,
                timing: false,
                connection_name: "my-conn",
                analytics: Some(&analytics),
            },
            &mut schema,
            &mut |_| {},
            &mut out,
        );

        let history = analytics.history("my-conn");
        assert_eq!(history.len(), 1, "query should be recorded for the connection");
        assert_eq!(history[0].query, "SELECT 1");
    }

    #[test]
    fn handle_history_shows_queries() {
        let core = core_with_connection("mydb");
        let analytics = core.analytics_api();
        let schema = schema_from(&[]);
        analytics.record_query("mydb", "SELECT 1", &schema).unwrap();
        analytics.record_query("mydb", "SELECT 2", &schema).unwrap();
        let mut out = Vec::new();
        handler().handle_history("mydb", &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("SELECT 1"), "expected query in history, got: {text}");
        assert!(text.contains("SELECT 2"), "expected query in history, got: {text}");
    }

    #[test]
    fn handle_stats_no_table_shows_tables() {
        let core = core_with_connection("mydb");
        let analytics = core.analytics_api();
        let schema = schema_from(&[("users", &["id", "email"])]);
        analytics.record_query("mydb", "SELECT id FROM users", &schema).unwrap();
        let mut out = Vec::new();
        handler().handle_stats("mydb", None, &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected table name, got: {text}");
    }

    #[test]
    fn handle_stats_with_table_shows_columns() {
        let core = core_with_connection("mydb");
        let analytics = core.analytics_api();
        let schema = schema_from(&[("users", &["id", "email"])]);
        analytics.record_query("mydb", "SELECT email FROM users", &schema).unwrap();
        let mut out = Vec::new();
        handler().handle_stats("mydb", Some("users"), &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("email"), "expected column name, got: {text}");
    }
}
