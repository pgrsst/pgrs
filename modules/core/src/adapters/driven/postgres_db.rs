use std::cell::RefCell;
use std::collections::HashMap;

use crate::domain::connection::Connection;
use crate::domain::error::DomainError;
use crate::domain::query_result::QueryResult;
use crate::enums::tls_mode::TlsMode;
use crate::ports::db_connection::DbConnection;
use crate::ports::db_connector::DbConnector;
use crate::ports::repl_port::ReplPort;
use crate::ports::schema_port::SchemaPort;

/// Driven adapter that opens live PostgreSQL connections. Holds no state; it
/// exists so the composition root can inject connection-opening as a port.
pub struct PostgresConnector;

impl DbConnector for PostgresConnector {
    fn connect(&self, connection: &Connection) -> Result<Box<dyn ReplPort>, DomainError> {
        Ok(Box::new(PostgresDb::new(connection)?))
    }
}

pub struct PostgresDb {
    client: RefCell<postgres::Client>,
}

impl PostgresDb {
    pub fn new(connection: &Connection) -> Result<Self, DomainError> {
        let mut config = postgres::Config::new();
        config
            .host(&connection.host)
            .port(connection.port)
            .user(&connection.username)
            .password(connection.password.as_bytes())
            .dbname(&connection.database);

        let connect_err = |e: postgres::Error| {
            DomainError::QueryError(format!("could not connect to '{}': {}", connection.name, e))
        };
        let tls_err = |e: native_tls::Error| {
            DomainError::QueryError(format!("failed to build TLS connector: {}", e))
        };

        let client = match connection.tls {
            TlsMode::Disable => config.connect(postgres::NoTls).map_err(connect_err)?,
            TlsMode::Require => {
                // Encrypt without verifying the server certificate (matches psql sslmode=require).
                let tls = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true)
                    .build()
                    .map_err(tls_err)?;
                let tls = postgres_native_tls::MakeTlsConnector::new(tls);
                config.connect(tls).map_err(connect_err)?
            }
            TlsMode::VerifyFull => {
                let tls = native_tls::TlsConnector::new().map_err(tls_err)?;
                let tls = postgres_native_tls::MakeTlsConnector::new(tls);
                config.connect(tls).map_err(connect_err)?
            }
        };

        Ok(Self {
            client: RefCell::new(client),
        })
    }
}

impl DbConnection for PostgresDb {
    fn execute(&self, query: &str) -> Result<QueryResult, DomainError> {
        use postgres::SimpleQueryMessage;

        let mut client = self
            .client
            .try_borrow_mut()
            .map_err(|e| DomainError::QueryError(format!("connection busy: {e}")))?;
        let messages = client
            .simple_query(query)
            .map_err(|e| DomainError::QueryError(format_pg_error(query, &e)))?;

        let mut columns: Vec<String> = vec![];
        let mut rows: Vec<Vec<String>> = vec![];
        let mut rows_affected: Option<u64> = None;

        for msg in messages {
            match msg {
                SimpleQueryMessage::Row(row) => {
                    if columns.is_empty() {
                        columns = row.columns().iter().map(|c| c.name().to_string()).collect();
                    }
                    rows.push(
                        (0..row.len())
                            .map(|i| row.get(i).unwrap_or("NULL").to_string())
                            .collect(),
                    );
                }
                SimpleQueryMessage::CommandComplete(n) => {
                    rows_affected = Some(n);
                }
                _ => {}
            }
        }

        // Zero-row SELECT: simple_query sends no Row messages, so column names
        // are lost. Use PREPARE to retrieve them from the server's plan.
        // DML/DDL also land here but return no columns from prepare — fine.
        if columns.is_empty() && rows.is_empty()
            && let Ok(stmt) = client.prepare(query) {
            columns = stmt.columns().iter().map(|c| c.name().to_string()).collect();
        }

        Ok(QueryResult { columns, rows, rows_affected })
    }
}

/// Field projection of a Postgres `DbError`, decoupled from the driver type so the
/// rendering logic can be unit-tested without a live server connection.
struct DbErrorFields<'a> {
    code: &'a str,
    message: &'a str,
    detail: Option<&'a str>,
    hint: Option<&'a str>,
    /// 1-based character position of the error within `query`, as reported by the server.
    position: Option<usize>,
}

/// Render a server-side error the way psql does: the message and SQLSTATE, the
/// offending line with a caret under the failing token, plus DETAIL/HINT when present.
fn render_db_error(query: &str, f: &DbErrorFields) -> String {
    let mut out = format!("ERROR: {} (SQLSTATE {})", f.message, f.code);

    if let Some(pos) = f.position
        && let Some((line_no, line_text, col)) = locate_position(query, pos)
    {
        let prefix = format!("LINE {}: ", line_no);
        let indent = prefix.chars().count() + col;
        out.push('\n');
        out.push_str(&prefix);
        out.push_str(line_text);
        out.push('\n');
        out.push_str(&" ".repeat(indent));
        out.push('^');
    }

    if let Some(detail) = f.detail {
        out.push_str("\nDETAIL: ");
        out.push_str(detail);
    }
    if let Some(hint) = f.hint {
        out.push_str("\nHINT: ");
        out.push_str(hint);
    }
    out
}

/// Map a 1-based character position into `(line_number, line_text, column)`,
/// where `column` is the 0-based char offset within that line. Returns `None`
/// if the position falls outside the query text.
fn locate_position(query: &str, pos_1based: usize) -> Option<(usize, &str, usize)> {
    let target = pos_1based.checked_sub(1)?;
    let mut char_idx = 0;
    for (line_no, line) in query.lines().enumerate() {
        let line_len = line.chars().count();
        if target <= char_idx + line_len {
            return Some((line_no + 1, line, target - char_idx));
        }
        char_idx += line_len + 1; // +1 for the stripped '\n'
    }
    None
}

/// Translate a `postgres::Error` into an informative message. Server-side errors
/// (bad SQL, constraint violations) carry a structured `DbError` we expand fully;
/// transport/protocol errors fall back to the driver's own text.
fn format_pg_error(query: &str, err: &postgres::Error) -> String {
    match err.as_db_error() {
        Some(db) => {
            let position = match db.position() {
                Some(postgres::error::ErrorPosition::Original(p)) => Some(*p as usize),
                _ => None,
            };
            render_db_error(
                query,
                &DbErrorFields {
                    code: db.code().code(),
                    message: db.message(),
                    detail: db.detail(),
                    hint: db.hint(),
                    position,
                },
            )
        }
        None => err.to_string(),
    }
}

impl SchemaPort for PostgresDb {
    fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
        let mut client = self
            .client
            .try_borrow_mut()
            .map_err(|e| DomainError::QueryError(format!("connection busy: {e}")))?;
        let rows = client
            .query(
                "SELECT table_name, column_name FROM information_schema.columns \
                 WHERE table_schema = 'public' \
                 ORDER BY table_name, ordinal_position",
                &[],
            )
            .map_err(|e| DomainError::QueryError(format_pg_error("", &e)))?;

        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for row in &rows {
            let table: String = row.get(0);
            let column: String = row.get(1);
            map.entry(table).or_default().push(column);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields<'a>(code: &'a str, message: &'a str) -> DbErrorFields<'a> {
        DbErrorFields { code, message, detail: None, hint: None, position: None }
    }

    #[test]
    fn render_db_error_includes_message_and_sqlstate() {
        let out = render_db_error(
            "SELECT 1",
            &fields("42P01", "relation \"userz\" does not exist"),
        );
        assert!(out.contains("relation \"userz\" does not exist"), "got: {out}");
        assert!(out.contains("42P01"), "expected SQLSTATE, got: {out}");
    }

    #[test]
    fn render_db_error_includes_detail_and_hint() {
        let out = render_db_error(
            "INSERT INTO t VALUES (1)",
            &DbErrorFields {
                code: "23505",
                message: "duplicate key value violates unique constraint \"t_pkey\"",
                detail: Some("Key (id)=(1) already exists."),
                hint: Some("Use a different id."),
                position: None,
            },
        );
        assert!(out.contains("DETAIL: Key (id)=(1) already exists."), "got: {out}");
        assert!(out.contains("HINT: Use a different id."), "got: {out}");
    }

    #[test]
    fn render_db_error_shows_line_and_caret_for_position() {
        // position 15 = the 'u' in "userz" (1-based char index)
        let out = render_db_error(
            "SELECT * FROM userz",
            &DbErrorFields {
                code: "42P01",
                message: "relation \"userz\" does not exist",
                detail: None,
                hint: None,
                position: Some(15),
            },
        );
        assert!(out.contains("LINE 1: SELECT * FROM userz"), "got: {out}");
        let caret_line = out.lines().find(|l| l.trim() == "^").expect("caret line");
        assert_eq!(
            caret_line.find('^').unwrap(),
            "LINE 1: ".len() + "SELECT * FROM ".len(),
            "caret should sit under 'u'; got: {out}"
        );
    }

    #[test]
    fn render_db_error_picks_correct_line_in_multiline_query() {
        // "SELECT *\nFROM userz" — 'u' is the 15th char overall, on line 2
        let out = render_db_error(
            "SELECT *\nFROM userz",
            &DbErrorFields {
                code: "42P01",
                message: "relation does not exist",
                detail: None,
                hint: None,
                position: Some(15),
            },
        );
        assert!(out.contains("LINE 2: FROM userz"), "got: {out}");
        let caret_line = out.lines().find(|l| l.trim() == "^").expect("caret line");
        assert_eq!(
            caret_line.find('^').unwrap(),
            "LINE 2: ".len() + "FROM ".len(),
            "caret should sit under 'u' on line 2; got: {out}"
        );
    }

    #[test]
    fn render_db_error_omits_caret_when_no_position() {
        let out = render_db_error("SELECT 1", &fields("42601", "syntax error"));
        assert!(!out.contains('^'), "no position → no caret line; got: {out}");
        assert!(!out.contains("LINE"), "no position → no LINE prefix; got: {out}");
    }

    #[test]
    fn locate_position_out_of_range_returns_none() {
        assert!(locate_position("SELECT 1", 999).is_none());
        assert!(locate_position("SELECT 1", 0).is_none());
    }
}
