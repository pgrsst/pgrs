use std::io::{self, Write};

use pgrs_core::{AnalyticsApi, QueryApi, QueryResult};

use super::sql_utils::{is_ddl, is_dml};

fn csv_quote(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

fn write_csv(result: &QueryResult, file: &mut impl Write) -> io::Result<()> {
    let header: Vec<String> = result.columns.iter().map(|c| csv_quote(c)).collect();
    writeln!(file, "{}", header.join(","))?;
    for row in &result.rows {
        let cells: Vec<String> = row.iter().map(|v| csv_quote(v)).collect();
        writeln!(file, "{}", cells.join(","))?;
    }
    Ok(())
}

/// Parses `\export <id> <path>` rest string (everything after `\export `).
/// Path may be unquoted, single-quoted, or double-quoted (to support spaces).
/// Returns `None` if the rest string cannot be parsed into (id, path).
pub(super) fn parse_export_args(rest: &str) -> Option<(i64, String)> {
    let rest = rest.trim();
    let (id_str, after_id) = rest.split_once(' ')?;
    let id: i64 = id_str.parse().ok()?;
    let path_raw = after_id.trim();
    if path_raw.is_empty() {
        return None;
    }
    let path = if (path_raw.starts_with('"') && path_raw.ends_with('"'))
        || (path_raw.starts_with('\'') && path_raw.ends_with('\''))
    {
        path_raw[1..path_raw.len() - 1].to_string()
    } else {
        path_raw.to_string()
    };
    let path = if let Some(without_tilde) = path.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{}{}", home, without_tilde)
    } else {
        path
    };
    Some((id, path))
}

pub(super) fn handle_export(
    id: i64,
    path: &str,
    connection_name: &str,
    query: &QueryApi,
    analytics: &AnalyticsApi,
    writer: &mut impl Write,
) {
    if std::path::Path::new(path).exists() {
        writeln!(writer, "error: file already exists: {}", path).ok();
        return;
    }
    let history = analytics.history(connection_name);
    let entry = match history.iter().find(|e| e.id == id) {
        Some(e) => e,
        None => {
            writeln!(writer, "error: no history entry with id {}", id).ok();
            return;
        }
    };
    if is_dml(&entry.query) || is_ddl(&entry.query) {
        writeln!(writer, "error: cannot export non-SELECT query").ok();
        return;
    }
    let result = match query.execute(&entry.query) {
        Ok(r) => r,
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
            return;
        }
    };
    let mut file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            writeln!(writer, "error: could not write file: {}", e).ok();
            return;
        }
    };
    if let Err(e) = write_csv(&result, &mut file) {
        drop(file);
        std::fs::remove_file(path).ok();
        writeln!(writer, "error: could not write file: {}", e).ok();
        return;
    }
    writeln!(writer, "Exported {} rows to {}", result.rows.len(), path).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use pgrs_core::{Core, DbConnection, DomainError, QueryApi, QueryResult, SchemaApi, SchemaPort};
    use std::collections::HashMap;

    struct StubDb {
        result: Result<QueryResult, DomainError>,
    }

    impl StubDb {
        fn ok(rows: Vec<Vec<String>>, cols: Vec<String>) -> Self {
            Self {
                result: Ok(QueryResult { columns: cols, rows, rows_affected: None }),
            }
        }
    }

    impl DbConnection for StubDb {
        fn execute(&self, _query: &str) -> Result<QueryResult, DomainError> {
            self.result.clone()
        }
    }

    impl SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
            Ok(HashMap::new())
        }
    }

    /// Seed an in-memory store with a connection and the given queries (as
    /// history), returning the Core plus the assigned history id for each query.
    fn seed(connection_name: &str, queries: &[&str]) -> (Core, Vec<i64>) {
        let core = Core::in_memory();
        core.connection
            .add(pgrs_core::AddConnectionInput {
                name: connection_name.to_string(),
                host: "localhost".to_string(),
                port: pgrs_core::DEFAULT_PORT,
                username: "u".to_string(),
                password: "p".to_string(),
                database: "db".to_string(),
                tls: pgrs_core::TlsMode::Disable,
                environment: None,
            })
            .unwrap();
        let analytics = core.analytics_api();
        let schema = SchemaApi::for_test(HashMap::new());
        for q in queries {
            analytics.record_query(connection_name, q, &schema).unwrap();
        }
        let history = analytics.history(connection_name);
        let ids = queries
            .iter()
            .map(|q| history.iter().find(|e| &e.query == q).map(|e| e.id).unwrap())
            .collect();
        (core, ids)
    }

    #[test]
    fn csv_quote_plain_value_unchanged() {
        assert_eq!(csv_quote("hello"), "hello");
    }

    #[test]
    fn csv_quote_value_with_comma_is_quoted() {
        assert_eq!(csv_quote("a,b"), "\"a,b\"");
    }

    #[test]
    fn csv_quote_value_with_double_quote_is_escaped() {
        assert_eq!(csv_quote("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_quote_value_with_newline_is_quoted() {
        assert_eq!(csv_quote("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn csv_quote_value_with_carriage_return_is_quoted() {
        assert_eq!(csv_quote("line1\rline2"), "\"line1\rline2\"");
    }

    #[test]
    fn write_csv_produces_header_and_rows() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec!["1".to_string(), "alice".to_string()],
                vec!["2".to_string(), "bob".to_string()],
            ],
            rows_affected: None,
        };
        let mut out = Vec::new();
        write_csv(&result, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "id,name\n1,alice\n2,bob\n");
    }

    #[test]
    fn write_csv_quotes_values_with_comma() {
        let result = QueryResult {
            columns: vec!["note".to_string()],
            rows: vec![vec!["a,b".to_string()]],
            rows_affected: None,
        };
        let mut out = Vec::new();
        write_csv(&result, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "note\n\"a,b\"\n");
    }

    #[test]
    fn write_csv_empty_result_writes_only_header() {
        let result = QueryResult {
            columns: vec!["id".to_string()],
            rows: vec![],
            rows_affected: None,
        };
        let mut out = Vec::new();
        write_csv(&result, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "id\n");
    }

    fn export_tmp_path(tag: &str) -> String {
        let dir = std::env::temp_dir();
        dir.join(format!("pgrs_export_{}_{}.csv", std::process::id(), tag))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn handle_export_writes_csv_for_valid_id() {
        let path = export_tmp_path("happy");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(
            vec![vec!["1".to_string(), "alice".to_string()]],
            vec!["id".to_string(), "name".to_string()],
        );
        let query = QueryApi::from_repl(Box::new(stub));
        let (core, ids) = seed("mydb", &["SELECT id, name FROM users;"]);
        let analytics = core.analytics_api();
        let mut out = Vec::new();
        handle_export(ids[0], &path, "mydb", &query, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("Exported 1 rows to"), "expected confirmation, got: {msg}");

        let csv = std::fs::read_to_string(&path).unwrap();
        assert_eq!(csv, "id,name\n1,alice\n");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn handle_export_errors_on_existing_file() {
        let path = export_tmp_path("exists");
        std::fs::write(&path, "existing").unwrap();

        let stub = StubDb::ok(vec![], vec![]);
        let query = QueryApi::from_repl(Box::new(stub));
        let (core, ids) = seed("mydb", &["SELECT 1;"]);
        let analytics = core.analytics_api();
        let mut out = Vec::new();
        handle_export(ids[0], &path, "mydb", &query, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("file already exists"), "expected file-exists error, got: {msg}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing", "file must not be overwritten");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn handle_export_errors_on_unknown_id() {
        let path = export_tmp_path("unknown");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(vec![], vec![]);
        let query = QueryApi::from_repl(Box::new(stub));
        let (core, _ids) = seed("mydb", &["SELECT 1;"]);
        let analytics = core.analytics_api();
        let mut out = Vec::new();
        handle_export(999, &path, "mydb", &query, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("no history entry with id 999"), "expected id-not-found error, got: {msg}");
        assert!(!std::path::Path::new(&path).exists(), "file must not be created");
    }

    #[test]
    fn handle_export_errors_on_dml_query() {
        let path = export_tmp_path("dml");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(vec![], vec![]);
        let query = QueryApi::from_repl(Box::new(stub));
        let (core, ids) = seed("mydb", &["INSERT INTO foo VALUES (1);"]);
        let analytics = core.analytics_api();
        let mut out = Vec::new();
        handle_export(ids[0], &path, "mydb", &query, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("cannot export non-SELECT query"), "expected non-SELECT error, got: {msg}");
        assert!(!std::path::Path::new(&path).exists(), "file must not be created");
    }

    #[test]
    fn handle_export_errors_on_ddl_query() {
        let path = export_tmp_path("ddl");
        let _ = std::fs::remove_file(&path);

        let stub = StubDb::ok(vec![], vec![]);
        let query = QueryApi::from_repl(Box::new(stub));
        let (core, ids) = seed("mydb", &["DROP TABLE foo;"]);
        let analytics = core.analytics_api();
        let mut out = Vec::new();
        handle_export(ids[0], &path, "mydb", &query, &analytics, &mut out);

        let msg = String::from_utf8(out).unwrap();
        assert!(msg.contains("cannot export non-SELECT query"), "expected non-SELECT error, got: {msg}");
        assert!(!std::path::Path::new(&path).exists(), "file must not be created");
    }

    #[test]
    fn parse_export_args_unquoted() {
        let result = parse_export_args("42 /tmp/output.csv");
        assert_eq!(result, Some((42, "/tmp/output.csv".to_string())));
    }

    #[test]
    fn parse_export_args_double_quoted_path_with_space() {
        let result = parse_export_args("1 \"/home/user/my documents/export.csv\"");
        assert_eq!(result, Some((1, "/home/user/my documents/export.csv".to_string())));
    }

    #[test]
    fn parse_export_args_single_quoted_path_with_space() {
        let result = parse_export_args("5 '/home/user/my docs/out.csv'");
        assert_eq!(result, Some((5, "/home/user/my docs/out.csv".to_string())));
    }

    #[test]
    fn parse_export_args_tilde_expansion() {
        let result = parse_export_args("7 ~/Documents/export.csv");
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(result, Some((7, format!("{}/Documents/export.csv", home))));
    }

    #[test]
    fn parse_export_args_tilde_in_quotes() {
        let result = parse_export_args("2 \"~/My Docs/export.csv\"");
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(result, Some((2, format!("{}/My Docs/export.csv", home))));
    }

    #[test]
    fn parse_export_args_invalid_id() {
        assert!(parse_export_args("abc /tmp/out.csv").is_none());
    }

    #[test]
    fn parse_export_args_missing_path() {
        assert!(parse_export_args("1").is_none());
        assert!(parse_export_args("1 ").is_none());
    }
}
