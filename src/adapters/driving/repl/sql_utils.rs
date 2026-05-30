use crate::core::services::schema::service::SchemaSvc;
use crate::core::query::tokenizer::{SqlToken, tokenize};
use crate::core::query::alias::SQL_KEYWORDS;

pub(super) fn is_complete_statement(s: &str) -> bool {
    let s = s.trim_end();
    if !s.ends_with(';') {
        return false;
    }
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                if in_single && chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    in_single = !in_single;
                }
            }
            '"' if !in_single => {
                if in_double && chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    in_double = !in_double;
                }
            }
            _ => {}
        }
    }
    !in_single && !in_double
}

pub(super) fn is_ddl(query: &str) -> bool {
    matches!(
        query
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_uppercase()
            .as_str(),
        "CREATE" | "DROP" | "ALTER" | "TRUNCATE"
    )
}

pub(super) fn is_dml(query: &str) -> bool {
    matches!(
        query
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_uppercase()
            .as_str(),
        "INSERT" | "UPDATE" | "DELETE"
    )
}

pub(super) fn extract_column_refs(query: &str, schema: &dyn SchemaSvc) -> Vec<(String, String)> {
    let mut in_select = false;
    let mut candidates: Vec<String> = Vec::new();

    for token in tokenize(query) {
        if let SqlToken::Word(w) = token {
            let upper = w.to_uppercase();
            if upper == "SELECT" { in_select = true; continue; }
            if upper == "FROM" { break; }
            if in_select && !SQL_KEYWORDS.contains(&upper.as_str()) && w != "*" {
                candidates.push(w.to_lowercase());
            }
        }
    }

    let mut refs = Vec::new();
    for col in candidates {
        for table in schema.tables() {
            if schema.columns_for(table).iter().any(|c| c == &col) {
                refs.push((table.to_string(), col.clone()));
                break;
            }
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_ddl_detects_create() {
        assert!(is_ddl("CREATE TABLE foo (id int);"));
        assert!(is_ddl("create table foo (id int);"));
    }

    #[test]
    fn is_ddl_detects_drop() {
        assert!(is_ddl("DROP TABLE foo;"));
    }

    #[test]
    fn is_ddl_detects_alter() {
        assert!(is_ddl("ALTER TABLE foo ADD COLUMN bar text;"));
    }

    #[test]
    fn is_ddl_detects_truncate() {
        assert!(is_ddl("TRUNCATE TABLE foo;"));
    }

    #[test]
    fn is_ddl_returns_false_for_select() {
        assert!(!is_ddl("SELECT * FROM foo;"));
        assert!(!is_ddl("INSERT INTO foo VALUES (1);"));
        assert!(!is_ddl("UPDATE foo SET x = 1;"));
        assert!(!is_ddl("DELETE FROM foo;"));
    }

    #[test]
    fn is_dml_detects_insert() {
        assert!(is_dml("INSERT INTO foo VALUES (1);"));
        assert!(is_dml("insert into foo values (1);"));
    }

    #[test]
    fn is_dml_detects_update() {
        assert!(is_dml("UPDATE foo SET x = 1;"));
    }

    #[test]
    fn is_dml_detects_delete() {
        assert!(is_dml("DELETE FROM foo;"));
    }

    #[test]
    fn is_dml_returns_false_for_select() {
        assert!(!is_dml("SELECT * FROM foo;"));
        assert!(!is_dml("WITH cte AS (SELECT 1) SELECT * FROM cte;"));
    }

    #[test]
    fn complete_unclosed_double_quoted_identifier_ending_with_semicolon_not_complete() {
        assert!(!is_complete_statement(r#"SELECT "col;"#));
    }

    #[test]
    fn complete_closed_double_quoted_identifier_then_semicolon_is_complete() {
        assert!(is_complete_statement(r#"SELECT "col;name" FROM t;"#));
    }

    #[test]
    fn complete_double_quoted_identifier_with_escaped_double_quote() {
        assert!(is_complete_statement(r#"SELECT "O""Brien" FROM t;"#));
    }

    #[test]
    fn complete_double_quote_inside_single_quote_does_not_open_identifier() {
        assert!(is_complete_statement(r#"SELECT '"quoted"' FROM t;"#));
    }

    #[test]
    fn complete_single_quote_inside_double_quote_does_not_open_string() {
        assert!(is_complete_statement(r#"SELECT "it's" FROM t;"#));
    }

    #[test]
    fn complete_no_semicolon_not_complete() {
        assert!(!is_complete_statement(r#"SELECT "col" FROM t"#));
    }
}
