use std::io::Write;

use crate::core::ports::analytics_port::AnalyticsPort;
use crate::core::ports::db_connection::DbConnection;
use crate::core::ports::repl_port::ReplPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::ports::schema_port::SchemaPort;
use crate::core::services::schema::service::SchemaService;
use super::alias::extract_referenced_tables;
use super::executor::format_result;
use super::sql_utils::{is_ddl, extract_column_refs};

pub(super) fn handle_d(schema: &SchemaService, writer: &mut impl Write) {
    let tables = schema.tables();
    if tables.is_empty() {
        writeln!(writer, "No tables.").ok();
    } else {
        for table in tables {
            writeln!(writer, " {}", table).ok();
        }
    }
}

pub(super) fn handle_dt(schema: &SchemaService, writer: &mut impl Write) {
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

const LIST_DATABASES_SQL: &str =
    "SELECT datname AS database \
     FROM pg_database \
     WHERE datistemplate = false \
     ORDER BY datname";

pub(super) fn handle_l(conn: &dyn DbConnection, expanded: bool, writer: &mut impl Write) {
    match conn.execute(LIST_DATABASES_SQL) {
        Ok(result) => write!(writer, "{}", format_result(&result, expanded)).ok(),
        Err(e) => { eprintln!("error: {}", e); None }
    };
}

pub(super) fn handle_history(connection_name: &str, analytics: &dyn AnalyticsPort, writer: &mut impl Write) {
    use chrono::{DateTime, Local, TimeZone};

    let history = analytics.get_history(connection_name);
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
    connection_name: &str,
    table: Option<&str>,
    analytics: &dyn AnalyticsPort,
    writer: &mut impl Write,
) {
    match table {
        None => {
            let freq = analytics.get_frequent_tables(connection_name);
            if freq.is_empty() {
                writeln!(writer, "No table statistics yet.").ok();
                return;
            }
            let name_w = freq.iter().map(|e| e.name.len()).max().unwrap_or(0);
            for entry in &freq {
                writeln!(writer, "  {:<name_w$}  {}", entry.name, entry.count).ok();
            }
        }
        Some(tbl) => {
            let freq = analytics.get_frequent_columns(connection_name, tbl);
            if freq.is_empty() {
                writeln!(writer, "No column statistics for '{}'.", tbl).ok();
                return;
            }
            let name_w = freq.iter().map(|e| e.name.len()).max().unwrap_or(0);
            for entry in &freq {
                writeln!(writer, "  {:<name_w$}  {}", entry.name, entry.count).ok();
            }
        }
    }
}

pub(super) struct SqlOptions<'a> {
    pub(super) expanded: bool,
    pub(super) timing: bool,
    pub(super) connection_name: &'a str,
    pub(super) analytics: Option<&'a dyn AnalyticsPort>,
    pub(super) schema_cache: Option<&'a dyn SchemaCachePort>,
}

pub(super) fn handle_sql(
    conn: &dyn ReplPort,
    query: &str,
    opts: &SqlOptions<'_>,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    writer: &mut impl Write,
) {
    let start = std::time::Instant::now();
    match conn.execute(query) {
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

            if let Some(analytics) = opts.analytics {
                let tables = extract_referenced_tables(query);
                let columns = extract_column_refs(query, schema);
                analytics.record_query(opts.connection_name, query, &tables, &columns);
            }

            if is_ddl(query)
                && let Ok(new_schema) = SchemaService::load_with_cache(conn, opts.connection_name, opts.schema_cache)
            {
                *schema = new_schema.clone();
                rebuild(new_schema);
                writeln!(writer, "(schema refreshed)").ok();
            }
        }
        Err(e) => eprintln!("error: {}", e),
    }
}

pub(super) fn handle_refresh(
    conn: &dyn SchemaPort,
    connection_name: &str,
    schema: &mut SchemaService,
    rebuild: &mut impl FnMut(SchemaService),
    schema_cache: Option<&dyn SchemaCachePort>,
    writer: &mut impl Write,
) {
    if let Some(cache) = schema_cache {
        cache.invalidate(connection_name);
    }
    match SchemaService::load_with_cache(conn, connection_name, schema_cache) {
        Ok(new_schema) => {
            *schema = new_schema.clone();
            rebuild(new_schema);
            writeln!(writer, "Schema refreshed.").ok();
        }
        Err(e) => eprintln!("error: could not refresh schema: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use crate::core::domain::analytics::{FreqEntry, HistoryEntry};
    use crate::core::ports::db_connection::QueryResult;

    struct StubDb {
        columns: HashMap<String, Vec<String>>,
        result: Result<QueryResult, String>,
    }

    impl StubDb {
        fn ok(rows: Vec<Vec<String>>, cols: Vec<String>) -> Self {
            Self {
                columns: HashMap::new(),
                result: Ok(QueryResult { columns: cols, rows, rows_affected: None }),
            }
        }
        fn err(msg: &str) -> Self {
            Self { columns: HashMap::new(), result: Err(msg.to_string()) }
        }
        fn with_schema(tables: &[(&str, &[&str])]) -> Self {
            let mut columns = HashMap::new();
            for (table, cols) in tables {
                columns.insert(table.to_string(), cols.iter().map(|c| c.to_string()).collect());
            }
            Self { columns, result: Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: Some(0) }) }
        }
    }

    impl crate::core::ports::db_connection::DbConnection for StubDb {
        fn execute(&self, _query: &str) -> Result<QueryResult, String> {
            self.result.clone()
        }
    }

    impl crate::core::ports::schema_port::SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
            Ok(self.columns.clone())
        }
    }

    fn schema_from(tables: &[(&str, &[&str])]) -> SchemaService {
        let stub = StubDb::with_schema(tables);
        SchemaService::load(&stub).unwrap()
    }

    #[test]
    fn handle_dt_prints_nothing_for_empty_schema() {
        let schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No tables."));
    }

    #[test]
    fn handle_dt_lists_table_names() {
        let schema = schema_from(&[("users", &["id", "email"]), ("orders", &["id"])]);
        let mut out = Vec::new();
        handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected 'users' in output, got: {text}");
        assert!(text.contains("orders"), "expected 'orders' in output, got: {text}");
    }

    #[test]
    fn handle_dt_shows_column_count() {
        let schema = schema_from(&[("users", &["id", "email"])]);
        let mut out = Vec::new();
        handle_dt(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("2 columns"), "expected column count, got: {text}");
    }

    #[test]
    fn handle_l_output_includes_database_name() {
        let stub = StubDb::ok(
            vec![vec!["mydb".to_string()]],
            vec!["database".to_string()],
        );
        let mut out = Vec::new();
        handle_l(&stub, false, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("mydb"), "expected db name in output, got: {text}");
    }

    #[test]
    fn handle_l_handles_db_error_gracefully() {
        let stub = StubDb::err("connection lost");
        let mut out = Vec::new();
        handle_l(&stub, false, &mut out);
        // error goes to stderr, stdout output is empty — should not panic
    }

    #[test]
    fn handle_sql_executes_query_without_panic() {
        let stub = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt, "no DDL — schema should not be rebuilt");
    }

    #[test]
    fn handle_sql_output_includes_query_result() {
        let stub = StubDb::ok(vec![vec!["42".to_string()]], vec!["id".to_string()]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 42", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("42"), "expected result value in output, got: {text}");
    }

    #[test]
    fn handle_sql_rebuilds_schema_after_ddl() {
        let stub = StubDb::with_schema(&[("users", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "CREATE TABLE users (id int)", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(rebuilt, "DDL should trigger schema rebuild");
    }

    #[test]
    fn handle_sql_shows_schema_refreshed_after_ddl() {
        let stub = StubDb::with_schema(&[("users", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "CREATE TABLE users (id int)", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| {}, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("schema refreshed"), "expected refresh notice, got: {text}");
    }

    #[test]
    fn handle_sql_does_not_rebuild_on_select() {
        let stub = StubDb::ok(vec![], vec![]);
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_sql(&stub, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| { rebuilt = true; }, &mut out);
        assert!(!rebuilt);
    }

    #[test]
    fn handle_sql_handles_error_gracefully() {
        let stub = StubDb::err("syntax error");
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_sql(&stub, "SELEKT *", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None, schema_cache: None }, &mut schema, &mut |_| {}, &mut out);
    }

    #[test]
    fn handle_refresh_updates_schema() {
        let stub = StubDb::with_schema(&[("products", &["id", "name"])]);
        let mut schema = schema_from(&[]);
        assert!(schema.tables().is_empty());
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_refresh(&stub, "my-conn", &mut schema, &mut |_| { rebuilt = true; }, None, &mut out);
        assert!(rebuilt);
        assert!(schema.tables().contains(&"products".to_string()));
    }

    #[test]
    fn handle_refresh_prints_confirmation() {
        let stub = StubDb::with_schema(&[("t", &["id"])]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_refresh(&stub, "my-conn", &mut schema, &mut |_| {}, None, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("refreshed"), "expected refresh confirmation, got: {text}");
    }

    #[test]
    fn handle_refresh_handles_error_gracefully() {
        struct FailingDb;
        impl crate::core::ports::schema_port::SchemaPort for FailingDb {
            fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, String> {
                Err("connection lost".to_string())
            }
        }
        let mut schema = schema_from(&[]);
        let mut rebuilt = false;
        let mut out = Vec::new();
        handle_refresh(&FailingDb, "my-conn", &mut schema, &mut |_| { rebuilt = true; }, None, &mut out);
        assert!(!rebuilt, "failed refresh must not trigger rebuild");
    }

    #[test]
    fn handle_d_prints_nothing_for_empty_schema() {
        let schema = schema_from(&[]);
        let mut out = Vec::new();
        handle_d(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("No tables."));
    }

    #[test]
    fn handle_d_lists_table_names_without_column_count() {
        let schema = schema_from(&[("users", &["id", "email"]), ("orders", &["id"])]);
        let mut out = Vec::new();
        handle_d(&schema, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected 'users' in output, got: {text}");
        assert!(text.contains("orders"), "expected 'orders' in output, got: {text}");
        assert!(!text.contains("columns"), "handle_d should not show column count, got: {text}");
    }

    struct RecordingAnalytics {
        recorded: RwLock<Vec<(String, String)>>,
    }
    impl RecordingAnalytics {
        fn new() -> Self { Self { recorded: RwLock::new(vec![]) } }
    }
    impl AnalyticsPort for RecordingAnalytics {
        fn record_query(&self, connection_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
            self.recorded.write().unwrap().push((connection_name.to_string(), query.to_string()));
        }
        fn get_history(&self, _: &str) -> Vec<HistoryEntry> {
            vec![
                HistoryEntry { id: 2, query: "SELECT 1".to_string(), executed_at: 1000 },
                HistoryEntry { id: 1, query: "SELECT 2".to_string(), executed_at: 999 },
            ]
        }
        fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> {
            vec![FreqEntry { name: "users".to_string(), count: 5 }]
        }
        fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> {
            vec![FreqEntry { name: "email".to_string(), count: 3 }]
        }
    }

    #[test]
    fn handle_sql_records_analytics_with_connection_name() {
        struct CapturingAnalytics {
            recorded: RwLock<Vec<(String, String)>>,
        }
        impl CapturingAnalytics {
            fn new() -> Self { Self { recorded: RwLock::new(vec![]) } }
        }
        impl AnalyticsPort for CapturingAnalytics {
            fn record_query(&self, connection_name: &str, query: &str, _: &[String], _: &[(String, String)]) {
                self.recorded.write().unwrap().push((connection_name.to_string(), query.to_string()));
            }
            fn get_history(&self, _: &str) -> Vec<HistoryEntry> { vec![] }
            fn get_frequent_tables(&self, _: &str) -> Vec<FreqEntry> { vec![] }
            fn get_frequent_columns(&self, _: &str, _: &str) -> Vec<FreqEntry> { vec![] }
        }

        let stub = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]);
        let analytics = CapturingAnalytics::new();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();

        handle_sql(
            &stub,
            "SELECT 1",
            &SqlOptions {
                expanded: false,
                timing: false,
                connection_name: "my-conn",
                analytics: Some(&analytics),
                schema_cache: None,
            },
            &mut schema,
            &mut |_| {},
            &mut out,
        );

        let recorded = analytics.recorded.read().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "my-conn", "connection_name must reach analytics, not db_name");
        assert_eq!(recorded[0].1, "SELECT 1");
    }

    #[test]
    fn handle_history_shows_queries() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_history("mydb", &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("SELECT 1"), "expected query in history, got: {text}");
        assert!(text.contains("SELECT 2"), "expected query in history, got: {text}");
    }

    #[test]
    fn handle_stats_no_table_shows_tables() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_stats("mydb", None, &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected table name, got: {text}");
        assert!(text.contains("5"), "expected count, got: {text}");
    }

    #[test]
    fn handle_stats_with_table_shows_columns() {
        let analytics = RecordingAnalytics::new();
        let mut out = Vec::new();
        handle_stats("mydb", Some("users"), &analytics, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("email"), "expected column name, got: {text}");
        assert!(text.contains("3"), "expected count, got: {text}");
    }
}
