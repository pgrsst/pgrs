use std::io::Write;

use crate::core::ports::db_connection::DbConnection;
use crate::core::ports::repl_port::ReplPort;
use crate::core::ports::schema_cache_port::SchemaCachePort;
use crate::core::ports::schema_port::SchemaPort;
use crate::core::services::analytics::service::AnalyticsService;
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

pub(super) fn handle_history(connection_name: &str, analytics: &AnalyticsService, writer: &mut impl Write) {
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
    analytics: &AnalyticsService,
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
    pub(super) analytics: Option<&'a AnalyticsService>,
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
    use std::sync::Mutex;
    use crate::core::domain::analytics::FreqEntry;
    use crate::core::domain::column_access::ColumnAccess;
    use crate::core::domain::connection::Connection;
    use crate::core::domain::error::DomainError;
    use crate::core::domain::query_history::QueryHistory;
    use crate::core::domain::table_access::TableAccess;
    use crate::core::ports::column_access_repository::ColumnAccessRepository;
    use crate::core::ports::connection_repository::ConnectionRepository;
    use crate::core::ports::db_connection::QueryResult;
    use crate::core::ports::query_history_repository::QueryHistoryRepository;
    use crate::core::ports::table_access_repository::TableAccessRepository;
    use crate::core::services::analytics::service::AnalyticsService;
    use crate::core::services::column_access::service::ColumnAccessService;
    use crate::core::services::query_history::service::QueryHistoryService;
    use crate::core::services::table_access::service::TableAccessService;

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

    struct StubConnRepo;
    impl ConnectionRepository for StubConnRepo {
        fn add(&self, _: Connection) -> Result<(), DomainError> { Ok(()) }
        fn list(&self) -> Result<Vec<Connection>, DomainError> { Ok(vec![]) }
        fn delete(&self, _: &str) -> Result<(), DomainError> { Ok(()) }
        fn get_connection(&self, n: &str) -> Result<Connection, DomainError> { Err(DomainError::NotFound(n.to_string())) }
        fn find_row_id(&self, _: &str) -> Result<i64, DomainError> { Ok(1) }
        fn rename(&self, _: &str, _: &str) -> Result<(), DomainError> { Ok(()) }
        fn update(&self, _: Connection) -> Result<(), DomainError> { Ok(()) }
    }

    struct StubAnalytics {
        history: Vec<QueryHistory>,
        tables: Vec<FreqEntry>,
        columns: Vec<FreqEntry>,
        recorded: Mutex<Vec<(String, String)>>,
    }

    impl StubAnalytics {
        fn new(history: Vec<QueryHistory>, tables: Vec<FreqEntry>, columns: Vec<FreqEntry>) -> Self {
            Self { history, tables, columns, recorded: Mutex::new(vec![]) }
        }
    }

    impl QueryHistoryRepository for StubAnalytics {
        fn save(&self, entity: &QueryHistory) -> Result<i64, DomainError> {
            self.recorded.lock().unwrap().push(("".to_string(), entity.query.clone()));
            Ok(1)
        }
        fn list_recent(&self, _: &str, _: usize) -> Vec<QueryHistory> { self.history.clone() }
    }

    impl TableAccessRepository for StubAnalytics {
        fn save(&self, _: &TableAccess) -> Result<(), DomainError> { Ok(()) }
        fn list_frequent(&self, _: &str, _: usize) -> Vec<FreqEntry> { self.tables.clone() }
    }

    impl ColumnAccessRepository for StubAnalytics {
        fn save(&self, _: &ColumnAccess) -> Result<(), DomainError> { Ok(()) }
        fn list_frequent_by_table(&self, _: &str, _: &str, _: usize) -> Vec<FreqEntry> { self.columns.clone() }
    }

    fn make_svc(
        history: Vec<QueryHistory>,
        tables: Vec<FreqEntry>,
        columns: Vec<FreqEntry>,
    ) -> (std::sync::Arc<StubAnalytics>, AnalyticsService) {
        let conn_repo = std::sync::Arc::new(StubConnRepo);
        let stub = std::sync::Arc::new(StubAnalytics::new(history, tables, columns));
        let history_svc = std::sync::Arc::new(QueryHistoryService::new(
            std::sync::Arc::clone(&conn_repo) as std::sync::Arc<dyn ConnectionRepository>,
            std::sync::Arc::clone(&stub) as std::sync::Arc<dyn QueryHistoryRepository>,
        ));
        let table_svc = std::sync::Arc::new(TableAccessService::new(
            std::sync::Arc::clone(&conn_repo) as std::sync::Arc<dyn ConnectionRepository>,
            std::sync::Arc::clone(&stub) as std::sync::Arc<dyn TableAccessRepository>,
        ));
        let col_svc = std::sync::Arc::new(ColumnAccessService::new(
            std::sync::Arc::clone(&conn_repo) as std::sync::Arc<dyn ConnectionRepository>,
            std::sync::Arc::clone(&stub) as std::sync::Arc<dyn ColumnAccessRepository>,
        ));
        let svc = AnalyticsService::new(history_svc, table_svc, col_svc);
        (stub, svc)
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

    #[test]
    fn handle_sql_records_analytics_with_connection_name() {
        let stub = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]);
        let (recording, svc) = make_svc(vec![], vec![], vec![]);
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();

        handle_sql(
            &stub,
            "SELECT 1",
            &SqlOptions {
                expanded: false,
                timing: false,
                connection_name: "my-conn",
                analytics: Some(&svc),
                schema_cache: None,
            },
            &mut schema,
            &mut |_| {},
            &mut out,
        );

        let recorded = recording.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "my-conn", "connection_name must reach analytics, not db_name");
        assert_eq!(recorded[0].1, "SELECT 1");
    }

    #[test]
    fn handle_history_shows_queries() {
        let history = vec![
            QueryHistory { id: 2, connection_id: 1, query: "SELECT 1".to_string(), executed_at: 1000 },
            QueryHistory { id: 1, connection_id: 1, query: "SELECT 2".to_string(), executed_at: 999 },
        ];
        let (_, svc) = make_svc(history, vec![], vec![]);
        let mut out = Vec::new();
        handle_history("mydb", &svc, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("SELECT 1"), "expected query in history, got: {text}");
        assert!(text.contains("SELECT 2"), "expected query in history, got: {text}");
    }

    #[test]
    fn handle_stats_no_table_shows_tables() {
        let tables = vec![FreqEntry { name: "users".to_string(), count: 5 }];
        let (_, svc) = make_svc(vec![], tables, vec![]);
        let mut out = Vec::new();
        handle_stats("mydb", None, &svc, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("users"), "expected table name, got: {text}");
        assert!(text.contains("5"), "expected count, got: {text}");
    }

    #[test]
    fn handle_stats_with_table_shows_columns() {
        let columns = vec![FreqEntry { name: "email".to_string(), count: 3 }];
        let (_, svc) = make_svc(vec![], vec![], columns);
        let mut out = Vec::new();
        handle_stats("mydb", Some("users"), &svc, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("email"), "expected column name, got: {text}");
        assert!(text.contains("3"), "expected count, got: {text}");
    }
}
