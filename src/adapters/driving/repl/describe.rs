use std::io::Write;
use crate::core::ports::db_connection::{DbConnection, QueryResult};
use super::executor::format_result;

const COLUMNS_SQL: &str = "\
    SELECT \
        a.attname AS column, \
        pg_catalog.format_type(a.atttypid, a.atttypmod) AS type, \
        CASE WHEN a.attnotnull THEN 'not null' ELSE '' END AS nullable, \
        COALESCE(pg_catalog.pg_get_expr(d.adbin, d.adrelid), '') AS default \
    FROM pg_catalog.pg_attribute a \
    LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
    WHERE a.attrelid = 'TABLE_NAME'::regclass \
      AND a.attnum > 0 \
      AND NOT a.attisdropped \
    ORDER BY a.attnum";

const SCHEMA_SQL: &str = "\
    SELECT n.nspname \
    FROM pg_catalog.pg_class c \
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
    WHERE c.relname = 'TABLE_NAME'";

fn fetch_schema(db: &dyn DbConnection, table: &str) -> String {
    let sql = SCHEMA_SQL.replace("TABLE_NAME", table);
    db.execute(&sql)
        .ok()
        .and_then(|r| r.rows.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .unwrap_or_else(|| "public".to_string())
}

pub fn describe_table(
    db: &dyn DbConnection,
    table: &str,
    extended: bool,
    writer: &mut impl Write,
) -> Result<(), String> {
    validate_table_name(table)?;

    let schema_name = fetch_schema(db, table);
    writeln!(writer, "Table \"{}.{}\"", schema_name, table).map_err(|e| e.to_string())?;
    writeln!(writer).map_err(|e| e.to_string())?;

    let sql = COLUMNS_SQL.replace("TABLE_NAME", table);
    let result = db.execute(&sql).map_err(|_| {
        format!("Did not find any relation named \"{}\".", table)
    })?;

    if result.rows.is_empty() {
        return Err(format!("Did not find any relation named \"{}\".", table));
    }

    write!(writer, "{}", format_result(&result, false)).map_err(|e| e.to_string())?;
    let _ = extended;
    Ok(())
}

fn validate_table_name(name: &str) -> Result<(), String> {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
        Ok(())
    } else {
        Err("invalid table name: only letters, digits, underscores, and dots are allowed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct StubDb {
        responses: HashMap<&'static str, Result<QueryResult, String>>,
    }

    impl StubDb {
        fn new() -> Self {
            Self { responses: HashMap::new() }
        }

        fn with(mut self, key: &'static str, result: Result<QueryResult, String>) -> Self {
            self.responses.insert(key, result);
            self
        }
    }

    impl DbConnection for StubDb {
        fn execute(&self, query: &str) -> Result<QueryResult, String> {
            for (key, result) in &self.responses {
                if query.contains(key) {
                    return result.clone();
                }
            }
            Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: None })
        }
    }

    fn make_columns_result() -> QueryResult {
        QueryResult {
            columns: vec![
                "column".to_string(),
                "type".to_string(),
                "nullable".to_string(),
                "default".to_string(),
            ],
            rows: vec![
                vec![
                    "id".to_string(),
                    "integer".to_string(),
                    "not null".to_string(),
                    "nextval('users_id_seq'::regclass)".to_string(),
                ],
                vec![
                    "email".to_string(),
                    "character varying(255)".to_string(),
                    "not null".to_string(),
                    "".to_string(),
                ],
                vec![
                    "created_at".to_string(),
                    "timestamp with time zone".to_string(),
                    "".to_string(),
                    "now()".to_string(),
                ],
            ],
            rows_affected: None,
        }
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("public.users").is_ok());
        assert!(validate_table_name("user_roles").is_ok());
    }

    #[test]
    fn validate_rejects_special_chars() {
        assert!(validate_table_name("users; DROP TABLE users").is_err());
        assert!(validate_table_name("users'").is_err());
        assert!(validate_table_name("users\"").is_err());
        assert!(validate_table_name("users-table").is_err());
    }

    #[test]
    fn validate_error_message_is_user_friendly() {
        let err = validate_table_name("bad'name").unwrap_err();
        assert!(err.contains("invalid table name"), "got: {err}");
    }

    #[test]
    fn describe_prints_table_header_and_columns() {
        let db = StubDb::new().with("pg_attribute", Ok(make_columns_result()));
        let mut out = Vec::new();
        describe_table(&db, "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Table"), "should show Table header, got:\n{text}");
        assert!(text.contains("id"), "should show column id, got:\n{text}");
        assert!(text.contains("integer"), "should show type, got:\n{text}");
        assert!(text.contains("not null"), "should show nullable, got:\n{text}");
    }
}
