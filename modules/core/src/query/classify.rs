//! SQL statement classification (DDL vs DML). This is SQL domain knowledge —
//! "does this statement change the schema / mutate rows" — so it lives in the
//! core and is shared by every front-end. UI-only concerns (e.g. whether a
//! line is a complete statement for multi-line buffering) stay in the front-end.

use sqlparser::ast::{Query, SetExpr, Statement};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

fn parse_first_statement(query: &str) -> Option<Statement> {
    Parser::parse_sql(&PostgreSqlDialect {}, query)
        .ok()
        .and_then(|mut stmts| if stmts.is_empty() { None } else { Some(stmts.remove(0)) })
}

/// True if the statement changes the schema (CREATE/DROP/ALTER/TRUNCATE …) —
/// the signal a front-end uses to know the cached schema needs reloading.
pub fn is_ddl(query: &str) -> bool {
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

/// True if the statement mutates rows (INSERT/UPDATE/DELETE), including DML
/// wrapped in a CTE (`WITH ... INSERT ... RETURNING`).
pub fn is_dml(query: &str) -> bool {
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
        assert!(!is_dml("WITH cte AS (SELECT 1) SELECT * FROM cte;"));
    }
}
