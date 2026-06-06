use std::io::Write;

use pgrs_core::{NamedDef, QueryApi, TableDescription};

use super::executor::format_result;

/// `\d <table>` / `\d+ <table>`: ask the core for a structured table description
/// and render it. All pg_catalog knowledge lives in the core; this is pure
/// presentation.
pub fn describe_table(
    db: &QueryApi,
    table: &str,
    extended: bool,
    writer: &mut impl Write,
) -> Result<(), String> {
    let desc = db.describe_table(table, extended).map_err(|e| e.to_string())?;
    print_description(&desc, writer)
}

fn print_description(desc: &TableDescription, writer: &mut impl Write) -> Result<(), String> {
    writeln!(writer, "Table \"{}.{}\"", desc.schema, desc.name).map_err(|e| e.to_string())?;
    writeln!(writer).map_err(|e| e.to_string())?;
    write!(writer, "{}", format_result(&desc.columns, false)).map_err(|e| e.to_string())?;

    print_named_list(&desc.indexes, "Indexes", writer);
    print_named_list(&desc.foreign_keys, "Foreign-key constraints", writer);
    print_named_list(&desc.checks, "Check constraints", writer);
    if desc.extended {
        print_named_list(&desc.triggers, "Triggers", writer);
    }

    Ok(())
}

fn print_named_list(items: &[NamedDef], header: &str, writer: &mut impl Write) {
    if items.is_empty() {
        return;
    }
    writeln!(writer, "\n{}:", header).ok();
    for item in items {
        writeln!(writer, "    \"{}\" {}", item.name, item.definition).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use pgrs_core::{DbConnection, DomainError, QueryApi, QueryResult, SchemaPort};

    /// Routes catalog SQL (issued by the core) to canned results by substring.
    struct StubDb {
        responses: HashMap<&'static str, Result<QueryResult, DomainError>>,
    }

    impl StubDb {
        fn new() -> Self {
            Self { responses: HashMap::new() }
        }

        fn with(mut self, key: &'static str, result: Result<QueryResult, DomainError>) -> Self {
            self.responses.insert(key, result);
            self
        }
    }

    impl DbConnection for StubDb {
        fn execute(&self, query: &str) -> Result<QueryResult, DomainError> {
            for (key, result) in &self.responses {
                if query.contains(key) {
                    return result.clone();
                }
            }
            Ok(QueryResult { columns: vec![], rows: vec![], rows_affected: None })
        }
    }

    impl SchemaPort for StubDb {
        fn list_columns(&self) -> Result<HashMap<String, Vec<String>>, DomainError> {
            Ok(HashMap::new())
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
            ],
            rows_affected: None,
        }
    }

    #[test]
    fn describe_prints_table_header_and_columns() {
        let db = StubDb::new().with("pg_attribute", Ok(make_columns_result()));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", false, &mut out).unwrap();
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
            rows: vec![vec![
                "users_pkey".to_string(),
                "CREATE UNIQUE INDEX users_pkey ON public.users USING btree (id)".to_string(),
            ]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("pg_indexes", Ok(indexes));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Indexes:"), "got:\n{text}");
        assert!(text.contains("users_pkey"), "got:\n{text}");
    }

    #[test]
    fn describe_prints_fk_section() {
        let fk = QueryResult {
            columns: vec!["conname".to_string(), "condef".to_string()],
            rows: vec![vec![
                "users_role_id_fkey".to_string(),
                "FOREIGN KEY (role_id) REFERENCES roles(id)".to_string(),
            ]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("contype = 'f'", Ok(fk));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Foreign-key constraints:"), "got:\n{text}");
        assert!(text.contains("users_role_id_fkey"), "got:\n{text}");
    }

    #[test]
    fn describe_prints_check_constraints_section() {
        let checks = QueryResult {
            columns: vec!["conname".to_string(), "condef".to_string()],
            rows: vec![vec![
                "users_email_check".to_string(),
                "CHECK ((email ~* '^[^@]+'::text))".to_string(),
            ]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("contype = 'c'", Ok(checks));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Check constraints:"), "got:\n{text}");
        assert!(text.contains("users_email_check"), "got:\n{text}");
    }

    #[test]
    fn describe_omits_empty_sections() {
        let db = StubDb::new().with("pg_attribute", Ok(make_columns_result()));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("Indexes:"), "empty indexes section should be omitted, got:\n{text}");
        assert!(!text.contains("Foreign-key constraints:"), "got:\n{text}");
    }

    #[test]
    fn describe_unknown_relation_returns_error() {
        // No "pg_attribute" response -> core reports the relation as missing.
        let db = StubDb::new();
        let mut out = Vec::new();
        let err = describe_table(&QueryApi::from_repl(Box::new(db)), "ghost", false, &mut out)
            .unwrap_err();
        assert!(err.contains("ghost"), "error should name the relation, got: {err}");
    }

    #[test]
    fn describe_rejects_invalid_table_name() {
        let db = StubDb::new();
        let mut out = Vec::new();
        let err =
            describe_table(&QueryApi::from_repl(Box::new(db)), "bad'name", false, &mut out)
                .unwrap_err();
        assert!(err.contains("invalid table name"), "got: {err}");
    }

    #[test]
    fn extended_describe_prints_triggers_section() {
        let triggers = QueryResult {
            columns: vec!["tgname".to_string(), "tgdef".to_string()],
            rows: vec![vec![
                "audit_users".to_string(),
                "CREATE TRIGGER audit_users AFTER INSERT ON users FOR EACH ROW EXECUTE FUNCTION audit()".to_string(),
            ]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("attstorage", Ok(make_columns_result()))
            .with("pg_trigger", Ok(triggers));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Triggers:"), "got:\n{text}");
        assert!(text.contains("audit_users"), "got:\n{text}");
    }

    #[test]
    fn non_extended_describe_omits_triggers() {
        let triggers = QueryResult {
            columns: vec!["tgname".to_string(), "tgdef".to_string()],
            rows: vec![vec!["audit_users".to_string(), "CREATE TRIGGER audit_users ...".to_string()]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(make_columns_result()))
            .with("pg_trigger", Ok(triggers));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", false, &mut out).unwrap();
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
            ],
            rows_affected: None,
        }
    }

    #[test]
    fn extended_describe_prints_column_extras() {
        let db = StubDb::new().with("attstorage", Ok(make_extended_columns_result()));
        let mut out = Vec::new();
        describe_table(&QueryApi::from_repl(Box::new(db)), "users", true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("storage"), "should show storage column, got:\n{text}");
        assert!(text.contains("plain"), "should show storage value, got:\n{text}");
    }
}
