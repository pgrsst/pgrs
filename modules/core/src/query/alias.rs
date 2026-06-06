use std::collections::HashMap;

use sqlparser::ast::{
    FromTable, ObjectNamePart, Query, SetExpr, Statement, TableFactor, TableObject, TableWithJoins,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

pub const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "ON", "AND", "OR", "NOT", "IN", "IS", "NULL", "AS", "DISTINCT",
    "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "INSERT", "INTO",
    "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "DROP", "ALTER",
    "BEGIN", "COMMIT", "ROLLBACK",
];

pub struct AliasMap {
    map: HashMap<String, Option<String>>,
}

impl AliasMap {
    pub fn resolve(&self, name: &str) -> Option<&str> {
        self.map.get(name).and_then(|v| v.as_deref())
    }

    pub fn real_tables(&self) -> impl Iterator<Item = &str> {
        self.map.values().filter_map(|v| v.as_deref())
    }

    pub fn aliases_for_table(&self, table: &str) -> Vec<&str> {
        self.map
            .iter()
            .filter_map(|(alias, val)| {
                if val.as_deref() == Some(table) { Some(alias.as_str()) } else { None }
            })
            .collect()
    }
}

pub struct JoinContext {
    pub right_table: String,
    pub left_tables: Vec<String>,
}

// --- private helpers ---

fn last_ident(name: &sqlparser::ast::ObjectName) -> Option<String> {
    name.0.last().and_then(|p| match p {
        ObjectNamePart::Identifier(ident) => Some(ident.value.to_lowercase()),
        _ => None,
    })
}

fn collect_factor_alias(factor: &TableFactor, map: &mut HashMap<String, Option<String>>) {
    match factor {
        TableFactor::Table { name, alias: Some(alias), .. } => {
            map.insert(alias.name.value.to_lowercase(), last_ident(name));
        }
        TableFactor::Table { .. } => {}
        TableFactor::Derived { alias, subquery, .. } => {
            if let Some(alias) = alias {
                map.insert(alias.name.value.to_lowercase(), None);
            }
            collect_aliases_from_query(subquery, map);
        }
        _ => {}
    }
}

fn collect_factor_table(factor: &TableFactor, tables: &mut Vec<String>) {
    match factor {
        TableFactor::Table { name, .. } => {
            if let Some(n) = last_ident(name) {
                tables.push(n);
            }
        }
        TableFactor::Derived { subquery, .. } => {
            collect_tables_from_query(subquery, tables);
        }
        _ => {}
    }
}

fn collect_aliases_from_twj(twj: &TableWithJoins, map: &mut HashMap<String, Option<String>>) {
    collect_factor_alias(&twj.relation, map);
    for join in &twj.joins {
        collect_factor_alias(&join.relation, map);
    }
}

fn collect_tables_from_twj(twj: &TableWithJoins, tables: &mut Vec<String>) {
    collect_factor_table(&twj.relation, tables);
    for join in &twj.joins {
        collect_factor_table(&join.relation, tables);
    }
}

fn collect_aliases_from_query(q: &Query, map: &mut HashMap<String, Option<String>>) {
    if let Some(with) = &q.with {
        for cte in &with.cte_tables {
            collect_aliases_from_query(&cte.query, map);
        }
    }
    match q.body.as_ref() {
        SetExpr::Select(sel) => {
            for twj in &sel.from {
                collect_aliases_from_twj(twj, map);
            }
        }
        SetExpr::Query(inner) => collect_aliases_from_query(inner, map),
        _ => {}
    }
}

fn collect_tables_from_query(q: &Query, tables: &mut Vec<String>) {
    if let Some(with) = &q.with {
        for cte in &with.cte_tables {
            collect_tables_from_query(&cte.query, tables);
        }
    }
    match q.body.as_ref() {
        SetExpr::Select(sel) => {
            for twj in &sel.from {
                collect_tables_from_twj(twj, tables);
            }
        }
        SetExpr::Query(inner) => collect_tables_from_query(inner, tables),
        _ => {}
    }
}

// Insert a dummy identifier after a dangling dot (e.g. "o. FROM") to allow
// sqlparser to parse the FROM clause when the SELECT list is incomplete.
fn fix_dangling_dots(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len() + 8);
    let bytes = sql.as_bytes();
    for i in 0..bytes.len() {
        result.push(bytes[i] as char);
        if bytes[i] == b'.'
            && (i + 1 == bytes.len() || bytes[i + 1].is_ascii_whitespace())
        {
            result.push('x');
        }
    }
    result
}

fn parse_statements(sql: &str) -> Vec<Statement> {
    Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .ok()
        .or_else(|| Parser::parse_sql(&PostgreSqlDialect {}, &fix_dangling_dots(sql)).ok())
        .unwrap_or_default()
}

// --- public API ---

pub fn build_alias_map(sql: &str) -> AliasMap {
    let mut map = HashMap::new();
    for stmt in parse_statements(sql) {
        match stmt {
            Statement::Query(q) => collect_aliases_from_query(&q, &mut map),
            Statement::Update(u) => collect_aliases_from_twj(&u.table, &mut map),
            _ => {}
        }
    }
    AliasMap { map }
}

pub fn extract_referenced_tables(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();
    for stmt in parse_statements(sql) {
        match stmt {
            Statement::Query(q) => collect_tables_from_query(&q, &mut tables),
            Statement::Update(u) => collect_tables_from_twj(&u.table, &mut tables),
            Statement::Insert(i) => {
                if let TableObject::TableName(name) = i.table
                    && let Some(n) = last_ident(&name) {
                        tables.push(n);
                    }
            }
            Statement::Delete(d) => {
                let twjs = match d.from {
                    FromTable::WithFromKeyword(v) | FromTable::WithoutKeyword(v) => v,
                };
                for twj in &twjs {
                    collect_tables_from_twj(twj, &mut tables);
                }
            }
            _ => {}
        }
    }
    tables.dedup();
    tables
}

/// Resolve column references in a SELECT projection against a known schema view
/// (`&[(table, columns)]`), returning `(table, column)` pairs. Lives in `query/`
/// alongside `extract_referenced_tables` so SQL parsing stays out of the API
/// facade; callers pass a schema view rather than the function reaching up into
/// the schema service.
pub fn extract_column_refs(query: &str, schema: &[(&str, &[String])]) -> Vec<(String, String)> {
    use sqlparser::ast::{Expr, SelectItem, SetExpr, Statement};

    let candidates: Vec<String> = Parser::parse_sql(&PostgreSqlDialect {}, query)
        .ok()
        .and_then(|mut stmts| if stmts.is_empty() { None } else { Some(stmts.remove(0)) })
        .and_then(|stmt| match stmt {
            Statement::Query(q) => match *q.body {
                SetExpr::Select(sel) => Some(sel.projection),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| match item {
            SelectItem::UnnamedExpr(Expr::Identifier(ident)) => Some(ident.value.to_lowercase()),
            SelectItem::ExprWithAlias { expr: Expr::Identifier(ident), .. } => {
                Some(ident.value.to_lowercase())
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                parts.last().map(|i| i.value.to_lowercase())
            }
            _ => None,
        })
        .collect();

    let mut refs = Vec::new();
    for col in candidates {
        for (table, columns) in schema {
            if columns.iter().any(|c| c.to_lowercase() == col) {
                refs.push((table.to_string(), col.clone()));
                break;
            }
        }
    }
    refs
}

// extract_join_context uses simple string splitting because it operates on
// partial (potentially unparseable) queries typed in the REPL mid-sentence.
pub fn extract_join_context(upper_query: &str, alias_map: &AliasMap) -> Option<JoinContext> {
    let tokens: Vec<&str> = upper_query.split_whitespace().collect();

    let last_join_pos = tokens.iter().rposition(|&t| t == "JOIN")?;

    let right_raw = tokens.get(last_join_pos + 1)?.to_lowercase();
    let right_base = right_raw.rsplit('.').next().unwrap_or(&right_raw);
    let right_table = alias_map
        .resolve(right_base)
        .map(|s| s.to_string())
        .unwrap_or_else(|| right_base.to_string());

    let left_tables: Vec<String> = tokens
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| {
            if (w[0] == "FROM" || w[0] == "JOIN" || w[0] == "UPDATE") && i != last_join_pos {
                let raw = w[1].to_lowercase();
                let base = raw.rsplit('.').next().unwrap_or(&raw).to_string();
                Some(base)
            } else {
                None
            }
        })
        .map(|raw| {
            alias_map
                .resolve(&raw)
                .map(|s| s.to_string())
                .unwrap_or(raw)
        })
        .filter(|t| t != &right_table)
        .collect();

    Some(JoinContext { right_table, left_tables })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_map_resolve_known_alias() {
        let map = build_alias_map("SELECT * FROM users u");
        assert_eq!(map.resolve("u"), Some("users"));
    }

    #[test]
    fn alias_map_resolve_unknown_returns_none() {
        let map = build_alias_map("SELECT * FROM users u");
        assert_eq!(map.resolve("x"), None);
    }

    #[test]
    fn alias_map_resolve_subquery_alias_returns_none() {
        let map = build_alias_map("SELECT * FROM (SELECT 1) sub");
        assert_eq!(map.resolve("sub"), None);
    }

    #[test]
    fn build_alias_map_from_without_as() {
        let map = build_alias_map("SELECT * FROM users u");
        assert_eq!(map.resolve("u"), Some("users"));
    }

    #[test]
    fn build_alias_map_from_with_as() {
        let map = build_alias_map("SELECT * FROM users AS u");
        assert_eq!(map.resolve("u"), Some("users"));
    }

    #[test]
    fn build_alias_map_comma_separated() {
        let map = build_alias_map("SELECT * FROM users u, orders o");
        assert_eq!(map.resolve("u"), Some("users"));
        assert_eq!(map.resolve("o"), Some("orders"));
    }

    #[test]
    fn build_alias_map_join_alias() {
        let map = build_alias_map("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        assert_eq!(map.resolve("u"), Some("users"));
        assert_eq!(map.resolve("o"), Some("orders"));
    }

    #[test]
    fn build_alias_map_subquery_with_as() {
        let map = build_alias_map("SELECT * FROM (SELECT id FROM users) AS sub");
        assert_eq!(map.resolve("sub"), None);
    }

    #[test]
    fn build_alias_map_subquery_without_as() {
        let map = build_alias_map("SELECT * FROM (SELECT id FROM users) sub");
        assert_eq!(map.resolve("sub"), None);
    }

    #[test]
    fn build_alias_map_table_without_alias_not_in_map() {
        let map = build_alias_map("SELECT * FROM users");
        assert_eq!(map.resolve("users"), None);
    }

    #[test]
    fn extract_join_context_finds_right_and_left_tables() {
        let map = build_alias_map("SELECT * FROM users JOIN orders ON users.id = orders.user_id");
        let ctx = extract_join_context(
            "SELECT * FROM USERS JOIN ORDERS ON USERS.ID = ORDERS.USER_ID",
            &map,
        )
        .unwrap();
        assert_eq!(ctx.right_table, "orders");
        assert!(ctx.left_tables.contains(&"users".to_string()));
    }

    #[test]
    fn extract_join_context_no_join_returns_none() {
        let map = build_alias_map("SELECT * FROM users");
        let ctx = extract_join_context("SELECT * FROM USERS", &map);
        assert!(ctx.is_none());
    }

    #[test]
    fn extract_join_context_multi_join_uses_last() {
        let map = build_alias_map(
            "SELECT * FROM users JOIN orders ON users.id = orders.user_id JOIN products ON orders.product_id = products.id",
        );
        let ctx = extract_join_context(
            "SELECT * FROM USERS JOIN ORDERS ON USERS.ID = ORDERS.USER_ID JOIN PRODUCTS ON ORDERS.PRODUCT_ID = PRODUCTS.ID",
            &map,
        )
        .unwrap();
        assert_eq!(ctx.right_table, "products");
    }

    #[test]
    fn extract_join_context_resolves_aliases() {
        let map = build_alias_map("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        let ctx = extract_join_context(
            "SELECT * FROM USERS U JOIN ORDERS O ON U.ID = O.USER_ID",
            &map,
        )
        .unwrap();
        assert_eq!(ctx.right_table, "orders");
        assert!(ctx.left_tables.contains(&"users".to_string()));
    }

    #[test]
    fn build_alias_map_schema_qualified_table_with_alias() {
        let map = build_alias_map("SELECT * FROM public.users u");
        assert_eq!(map.resolve("u"), Some("users"), "alias 'u' should resolve to 'users', not 'public'");
    }

    #[test]
    fn build_alias_map_schema_qualified_table_with_as_alias() {
        let map = build_alias_map("SELECT * FROM public.users AS u");
        assert_eq!(map.resolve("u"), Some("users"));
    }

    #[test]
    fn build_alias_map_schema_qualified_join() {
        let map = build_alias_map("SELECT * FROM public.users u JOIN public.orders o ON u.id = o.user_id");
        assert_eq!(map.resolve("u"), Some("users"));
        assert_eq!(map.resolve("o"), Some("orders"));
    }

    #[test]
    fn extract_join_context_schema_qualified_tables() {
        let map = build_alias_map("SELECT * FROM public.users JOIN public.orders ON users.id = orders.user_id");
        let ctx = extract_join_context(
            "SELECT * FROM PUBLIC.USERS JOIN PUBLIC.ORDERS ON USERS.ID = ORDERS.USER_ID",
            &map,
        ).unwrap();
        assert_eq!(ctx.right_table, "orders");
        assert!(ctx.left_tables.contains(&"users".to_string()));
    }

    #[test]
    fn aliases_for_table_single_alias() {
        let map = build_alias_map("SELECT * FROM users u");
        let mut aliases = map.aliases_for_table("users");
        aliases.sort();
        assert_eq!(aliases, vec!["u"]);
    }

    #[test]
    fn aliases_for_table_multiple_aliases() {
        let map = build_alias_map("SELECT u.id, u2.id FROM users u JOIN users u2 ON u.id != u2.id");
        let mut aliases = map.aliases_for_table("users");
        aliases.sort();
        assert_eq!(aliases, vec!["u", "u2"]);
    }

    #[test]
    fn aliases_for_table_no_alias_returns_empty() {
        let map = build_alias_map("SELECT * FROM users");
        assert!(map.aliases_for_table("users").is_empty());
    }

    #[test]
    fn aliases_for_table_unknown_table_returns_empty() {
        let map = build_alias_map("SELECT * FROM users u");
        assert!(map.aliases_for_table("orders").is_empty());
    }

    #[test]
    fn extract_referenced_tables_simple_from() {
        let tables = extract_referenced_tables("SELECT * FROM users");
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn extract_referenced_tables_with_alias() {
        let tables = extract_referenced_tables("SELECT * FROM users u");
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn extract_referenced_tables_join() {
        let mut tables = extract_referenced_tables("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
        tables.sort();
        assert_eq!(tables, vec!["orders", "users"]);
    }

    #[test]
    fn extract_referenced_tables_no_from_returns_empty() {
        let tables = extract_referenced_tables("SELECT 1");
        assert!(tables.is_empty());
    }

    #[test]
    fn extract_referenced_tables_schema_qualified() {
        let tables = extract_referenced_tables("SELECT * FROM public.users");
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn extract_column_refs_resolves_unqualified_columns() {
        let users_cols = vec!["id".to_string(), "email".to_string()];
        let schema: Vec<(&str, &[String])> = vec![("users", users_cols.as_slice())];
        let refs = extract_column_refs("SELECT id, email FROM users", &schema);
        assert!(refs.contains(&("users".to_string(), "id".to_string())));
        assert!(refs.contains(&("users".to_string(), "email".to_string())));
    }

    #[test]
    fn extract_column_refs_picks_last_part_of_compound_identifier() {
        let users_cols = vec!["id".to_string()];
        let schema: Vec<(&str, &[String])> = vec![("users", users_cols.as_slice())];
        let refs = extract_column_refs("SELECT u.id FROM users u", &schema);
        assert_eq!(refs, vec![("users".to_string(), "id".to_string())]);
    }

    #[test]
    fn extract_column_refs_ignores_unknown_columns() {
        let users_cols = vec!["id".to_string()];
        let schema: Vec<(&str, &[String])> = vec![("users", users_cols.as_slice())];
        let refs = extract_column_refs("SELECT missing FROM users", &schema);
        assert!(refs.is_empty());
    }
}
