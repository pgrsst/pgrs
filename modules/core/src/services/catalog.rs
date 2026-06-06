//! pg_catalog / system-catalog lookups behind the `\d`, `\d+`, and `\l` REPL
//! commands. These queries are PostgreSQL-specific, so they live in the core
//! (behind the `QueryApi` facade) instead of leaking into the UI layer — a
//! front-end only ever sees the structured [`TableDescription`] / database list.

use crate::domain::error::DomainError;
use crate::ports::db_connection::{DbConnection, QueryResult};

/// A named database object together with its definition — an index, a
/// foreign-key/check constraint, or a trigger.
#[derive(Debug, Clone)]
pub struct NamedDef {
    pub name: String,
    pub definition: String,
}

/// Structured result of describing a single table (`\d` / `\d+`).
///
/// The column listing is kept as a [`QueryResult`] so front-ends can reuse
/// their generic table formatter; the remaining sections are plain value lists.
#[derive(Debug, Clone)]
pub struct TableDescription {
    pub schema: String,
    pub name: String,
    pub extended: bool,
    pub columns: QueryResult,
    pub indexes: Vec<NamedDef>,
    pub foreign_keys: Vec<NamedDef>,
    pub checks: Vec<NamedDef>,
    pub triggers: Vec<NamedDef>,
}

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

const LIST_DATABASES_SQL: &str = "\
    SELECT datname AS database \
    FROM pg_database \
    WHERE datistemplate = false \
    ORDER BY datname";

/// Reject anything that isn't a bare (optionally schema-qualified) identifier
/// before it is spliced into a catalog query as a literal.
fn validate_table_name(name: &str) -> Result<(), DomainError> {
    if name.is_empty() {
        return Err(DomainError::ValidationError(
            "table name cannot be empty".to_string(),
        ));
    }
    if name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        Ok(())
    } else {
        Err(DomainError::ValidationError(
            "invalid table name: only letters, digits, underscores, and dots are allowed"
                .to_string(),
        ))
    }
}

fn fetch_schema<D: DbConnection + ?Sized>(db: &D, table: &str) -> String {
    let sql = SCHEMA_SQL.replace("TABLE_NAME", table);
    db.execute(&sql)
        .ok()
        .and_then(|r| r.rows.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .unwrap_or_else(|| "public".to_string())
}

fn fetch_named<D: DbConnection + ?Sized>(db: &D, sql_template: &str, table: &str) -> Vec<NamedDef> {
    let sql = sql_template.replace("TABLE_NAME", table);
    db.execute(&sql)
        .map(|result| {
            result
                .rows
                .into_iter()
                .map(|row| {
                    let mut it = row.into_iter();
                    NamedDef {
                        name: it.next().unwrap_or_default(),
                        definition: it.next().unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Describe a single table: columns plus indexes, FK/check constraints, and
/// (when `extended`) triggers. Errors with [`DomainError::NotFound`] if the
/// relation does not exist.
pub fn describe_table<D: DbConnection + ?Sized>(
    db: &D,
    table: &str,
    extended: bool,
) -> Result<TableDescription, DomainError> {
    validate_table_name(table)?;

    let col_sql = if extended {
        COLUMNS_EXTENDED_SQL.replace("TABLE_NAME", table)
    } else {
        COLUMNS_SQL.replace("TABLE_NAME", table)
    };

    let not_found =
        || DomainError::NotFound(format!("Did not find any relation named \"{}\".", table));

    let columns = match db.execute(&col_sql) {
        Ok(result) if !result.rows.is_empty() => result,
        _ => return Err(not_found()),
    };

    Ok(TableDescription {
        schema: fetch_schema(db, table),
        name: table.to_string(),
        extended,
        columns,
        indexes: fetch_named(db, INDEXES_SQL, table),
        foreign_keys: fetch_named(db, FK_SQL, table),
        checks: fetch_named(db, CHECK_SQL, table),
        triggers: if extended {
            fetch_named(db, TRIGGERS_SQL, table)
        } else {
            Vec::new()
        },
    })
}

/// List user-visible database names (the `\l` command).
pub fn list_databases<D: DbConnection + ?Sized>(db: &D) -> Result<Vec<String>, DomainError> {
    let result = db.execute(LIST_DATABASES_SQL)?;
    Ok(result
        .rows
        .into_iter()
        .filter_map(|mut row| if row.is_empty() { None } else { Some(row.remove(0)) })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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

    fn columns_result() -> QueryResult {
        QueryResult {
            columns: vec!["column".into(), "type".into(), "nullable".into(), "default".into()],
            rows: vec![vec!["id".into(), "integer".into(), "not null".into(), "".into()]],
            rows_affected: None,
        }
    }

    #[test]
    fn validate_accepts_valid_names() {
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("public.users").is_ok());
        assert!(validate_table_name("user_roles").is_ok());
    }

    #[test]
    fn validate_rejects_empty_name() {
        let err = validate_table_name("").unwrap_err();
        assert!(err.to_string().contains("cannot be empty"), "got: {err}");
    }

    #[test]
    fn validate_rejects_special_chars() {
        assert!(validate_table_name("users; DROP TABLE users").is_err());
        assert!(validate_table_name("users'").is_err());
        assert!(validate_table_name("users\"").is_err());
        assert!(validate_table_name("users-table").is_err());
    }

    #[test]
    fn describe_rejects_invalid_name() {
        let db = StubDb::new();
        let err = describe_table(&db, "bad'name", false).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn describe_missing_relation_is_not_found() {
        // No "pg_attribute" response -> empty columns -> NotFound.
        let db = StubDb::new();
        let err = describe_table(&db, "ghost", false).unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)), "got: {err:?}");
    }

    #[test]
    fn describe_collects_columns_and_sections() {
        let indexes = QueryResult {
            columns: vec!["indexname".into(), "indexdef".into()],
            rows: vec![vec!["users_pkey".into(), "CREATE UNIQUE INDEX ...".into()]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("pg_attribute", Ok(columns_result()))
            .with("pg_indexes", Ok(indexes));
        let desc = describe_table(&db, "users", false).unwrap();
        assert_eq!(desc.name, "users");
        assert_eq!(desc.schema, "public"); // SCHEMA_SQL stub falls through to default
        assert_eq!(desc.columns.rows.len(), 1);
        assert_eq!(desc.indexes.len(), 1);
        assert_eq!(desc.indexes[0].name, "users_pkey");
        assert!(desc.triggers.is_empty(), "non-extended omits triggers");
    }

    #[test]
    fn describe_extended_fetches_triggers() {
        let triggers = QueryResult {
            columns: vec!["tgname".into(), "tgdef".into()],
            rows: vec![vec!["audit".into(), "CREATE TRIGGER audit ...".into()]],
            rows_affected: None,
        };
        let db = StubDb::new()
            .with("attstorage", Ok(columns_result()))
            .with("pg_trigger", Ok(triggers));
        let desc = describe_table(&db, "users", true).unwrap();
        assert!(desc.extended);
        assert_eq!(desc.triggers.len(), 1);
        assert_eq!(desc.triggers[0].name, "audit");
    }

    #[test]
    fn list_databases_extracts_first_column() {
        let db = StubDb::new().with(
            "pg_database",
            Ok(QueryResult {
                columns: vec!["database".into()],
                rows: vec![vec!["app".into()], vec!["analytics".into()]],
                rows_affected: None,
            }),
        );
        let dbs = list_databases(&db).unwrap();
        assert_eq!(dbs, vec!["app".to_string(), "analytics".to_string()]);
    }

    #[test]
    fn list_databases_propagates_error() {
        let db = StubDb::new()
            .with("pg_database", Err(DomainError::QueryError("connection lost".into())));
        assert!(list_databases(&db).is_err());
    }
}
