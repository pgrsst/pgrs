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

const INDEXES_SQL: &str = "\
    SELECT indexname, indexdef \
    FROM pg_indexes \
    WHERE tablename = 'TABLE_NAME' \
    ORDER BY indexname";

const FK_SQL: &str = "\
    SELECT conname, pg_catalog.pg_get_constraintdef(oid, true) \
    FROM pg_catalog.pg_constraint \
    WHERE conrelid = 'TABLE_NAME'::regclass AND contype = 'f' \
    ORDER BY conname";

const CHECK_SQL: &str = "\
    SELECT conname, pg_catalog.pg_get_constraintdef(oid, true) \
    FROM pg_catalog.pg_constraint \
    WHERE conrelid = 'TABLE_NAME'::regclass AND contype = 'c' \
    ORDER BY conname";

const TRIGGERS_SQL: &str = "\
    SELECT tgname, pg_catalog.pg_get_triggerdef(oid, true) \
    FROM pg_catalog.pg_trigger \
    WHERE tgrelid = 'TABLE_NAME'::regclass AND NOT tgisinternal \
    ORDER BY tgname";

const COLUMNS_EXTENDED_SQL: &str = "\
    SELECT \
        a.attname AS column, \
        pg_catalog.format_type(a.atttypid, a.atttypmod) AS type, \
        CASE WHEN a.attnotnull THEN 'not null' ELSE '' END AS nullable, \
        COALESCE(pg_catalog.pg_get_expr(d.adbin, d.adrelid), '') AS default, \
        CASE a.attstorage \
            WHEN 'p' THEN 'plain' \
            WHEN 'e' THEN 'external' \
            WHEN 'm' THEN 'main' \
            WHEN 'x' THEN 'extended' \
            ELSE '' \
        END AS storage, \
        CASE WHEN a.attstattarget = -1 THEN '-' ELSE a.attstattarget::text END AS stats_target, \
        COALESCE(pg_catalog.col_description(a.attrelid, a.attnum), '') AS description \
    FROM pg_catalog.pg_attribute a \
    LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
    WHERE a.attrelid = 'TABLE_NAME'::regclass \
      AND a.attnum > 0 \
      AND NOT a.attisdropped \
    ORDER BY a.attnum";

fn fetch_schema(db: &dyn DbConnection, table: &str) -> String {
    let sql = SCHEMA_SQL.replace("TABLE_NAME", table);
    db.execute(&sql)
        .ok()
        .and_then(|r| r.rows.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .unwrap_or_else(|| "public".to_string())
}

fn print_named_list(
    db: &dyn DbConnection,
    sql_template: &str,
    table: &str,
    header: &str,
    writer: &mut impl Write,
) {
    let sql = sql_template.replace("TABLE_NAME", table);
    if let Ok(result) = db.execute(&sql) {
        if !result.rows.is_empty() {
            writeln!(writer, "\n{}:", header).ok();
            for row in &result.rows {
                let name = row.get(0).map(String::as_str).unwrap_or("");
                let def = row.get(1).map(String::as_str).unwrap_or("");
                writeln!(writer, "    \"{}\" {}", name, def).ok();
            }
        }
    }
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

    let col_sql = if extended {
        COLUMNS_EXTENDED_SQL.replace("TABLE_NAME", table)
    } else {
        COLUMNS_SQL.replace("TABLE_NAME", table)
    };

    let result = db.execute(&col_sql).map_err(|_| {
        format!("Did not find any relation named \"{}\".", table)
    })?;

    if result.rows.is_empty() {
        return Err(format!("Did not find any relation named \"{}\".", table));
    }

    write!(writer, "{}", format_result(&result, false)).map_err(|e| e.to_string())?;

    print_named_list(db, INDEXES_SQL, table, "Indexes", writer);
    print_named_list(db, FK_SQL, table, "Foreign-key constraints", writer);
    print_named_list(db, CHECK_SQL, table, "Check constraints", writer);

    if extended {
        print_named_list(db, TRIGGERS_SQL, table, "Triggers", writer);
    }

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

    #[test]
    fn describe_prints_indexes_section() {
        let indexes = QueryResult {
            columns: vec!["indexname".to_string(), "indexdef".to_string()],
            rows: vec![
                vec!["users_pkey".to_string(), "CREATE UNIQUE INDEX users_pkey ON public.users USING btree (id)".to_string()],
            ],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("pg_indexes", Ok(indexes));
        let mut out = Vec::new();
        describe_table(&db, "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Indexes:"), "got:\n{text}");
        assert!(text.contains("users_pkey"), "got:\n{text}");
    }

    #[test]
    fn describe_prints_fk_section() {
        let fk = QueryResult {
            columns: vec!["conname".to_string(), "condef".to_string()],
            rows: vec![
                vec!["users_role_id_fkey".to_string(), "FOREIGN KEY (role_id) REFERENCES roles(id)".to_string()],
            ],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("contype = 'f'", Ok(fk));
        let mut out = Vec::new();
        describe_table(&db, "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Foreign-key constraints:"), "got:\n{text}");
        assert!(text.contains("users_role_id_fkey"), "got:\n{text}");
    }

    #[test]
    fn describe_prints_check_constraints_section() {
        let checks = QueryResult {
            columns: vec!["conname".to_string(), "condef".to_string()],
            rows: vec![
                vec!["users_email_check".to_string(), "CHECK ((email ~* '^[^@]+'::text))".to_string()],
            ],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("contype = 'c'", Ok(checks));
        let mut out = Vec::new();
        describe_table(&db, "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Check constraints:"), "got:\n{text}");
        assert!(text.contains("users_email_check"), "got:\n{text}");
    }

    #[test]
    fn describe_omits_empty_sections() {
        let db = StubDb::new().with("pg_attribute", Ok(make_columns_result()));
        let mut out = Vec::new();
        describe_table(&db, "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("Indexes:"), "empty indexes section should be omitted, got:\n{text}");
        assert!(!text.contains("Foreign-key constraints:"), "got:\n{text}");
    }

    #[test]
    fn extended_describe_prints_triggers_section() {
        let triggers = QueryResult {
            columns: vec!["tgname".to_string(), "tgdef".to_string()],
            rows: vec![
                vec!["audit_users".to_string(), "CREATE TRIGGER audit_users AFTER INSERT ON users FOR EACH ROW EXECUTE FUNCTION audit()".to_string()],
            ],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("pg_trigger", Ok(triggers));
        let mut out = Vec::new();
        describe_table(&db, "users", true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Triggers:"), "got:\n{text}");
        assert!(text.contains("audit_users"), "got:\n{text}");
    }

    #[test]
    fn non_extended_describe_omits_triggers() {
        let triggers = QueryResult {
            columns: vec!["tgname".to_string(), "tgdef".to_string()],
            rows: vec![
                vec!["audit_users".to_string(), "CREATE TRIGGER audit_users ...".to_string()],
            ],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("pg_trigger", Ok(triggers));
        let mut out = Vec::new();
        describe_table(&db, "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("Triggers:"), "non-extended should omit triggers, got:\n{text}");
    }

    fn make_extended_columns_result() -> QueryResult {
        QueryResult {
            columns: vec![
                "column".to_string(), "type".to_string(), "nullable".to_string(),
                "default".to_string(), "storage".to_string(), "stats_target".to_string(),
                "description".to_string(),
            ],
            rows: vec![
                vec!["id".to_string(), "integer".to_string(), "not null".to_string(),
                     "nextval('users_id_seq'::regclass)".to_string(),
                     "plain".to_string(), "-".to_string(), "".to_string()],
                vec!["email".to_string(), "character varying(255)".to_string(), "not null".to_string(),
                     "".to_string(), "extended".to_string(), "-".to_string(), "User email address".to_string()],
            ],
            rows_affected: None,
        }
    }

    #[test]
    fn extended_describe_prints_column_extras() {
        let db = StubDb::new()
            .with("attstorage", Ok(make_extended_columns_result()));
        let mut out = Vec::new();
        describe_table(&db, "users", true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("storage"), "should show storage column, got:\n{text}");
        assert!(text.contains("extended"), "should show storage value, got:\n{text}");
    }
}
