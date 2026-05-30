use sqlparser::ast::{Query, SetExpr, Statement};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

fn parse_first_statement(query: &str) -> Option<Statement> {
    Parser::parse_sql(&PostgreSqlDialect {}, query)
        .ok()
        .and_then(|mut stmts| if stmts.is_empty() { None } else { Some(stmts.remove(0)) })
}

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
        parse_first_statement(query),
        Some(
            Statement::CreateTable(_)
                | Statement::CreateView(_)
                | Statement::CreateIndex(_)
                | Statement::Drop { .. }
                | Statement::AlterTable(_)
                | Statement::AlterIndex { .. }
                | Statement::AlterView { .. }
                | Statement::Truncate(_)
        )
    )
}

fn query_contains_dml(q: &Query) -> bool {
    if let Some(with) = &q.with
        && with
            .cte_tables
            .iter()
            .any(|cte| query_contains_dml(&cte.query))
    {
        return true;
    }
    matches!(
        q.body.as_ref(),
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_)
    )
}

pub(super) fn is_dml(query: &str) -> bool {
    match parse_first_statement(query) {
        Some(Statement::Insert(_) | Statement::Update(_) | Statement::Delete(_)) => true,
        Some(Statement::Query(q)) => query_contains_dml(&q),
        _ => false,
    }
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
    }

    #[test]
    fn is_dml_detects_cte_wrapped_insert() {
        assert!(is_dml(
            "WITH cte AS (INSERT INTO foo VALUES (1) RETURNING id) SELECT * FROM cte;"
        ));
    }

    #[test]
    fn is_dml_detects_cte_wrapped_update() {
        assert!(is_dml(
            "WITH cte AS (UPDATE foo SET x = 1 RETURNING id) SELECT * FROM cte;"
        ));
    }

    #[test]
    fn is_dml_detects_cte_wrapped_delete() {
        assert!(is_dml(
            "WITH cte AS (DELETE FROM foo RETURNING id) SELECT * FROM cte;"
        ));
    }

    #[test]
    fn is_dml_plain_cte_select_is_false() {
        assert!(!is_dml(
            "WITH cte AS (SELECT 1) SELECT * FROM cte;"
        ));
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
