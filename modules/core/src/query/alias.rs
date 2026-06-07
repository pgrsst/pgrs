use std::collections::HashMap;

use sqlparser::ast::{
    Expr, FromTable, Function, FunctionArg, FunctionArgExpr, FunctionArguments, JoinConstraint,
    JoinOperator, ObjectNamePart, Query, Select, SelectItem, SelectItemQualifiedWildcardKind,
    SetExpr, Statement, TableFactor, TableObject, TableWithJoins,
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

/// A column mention pulled from a query before it is resolved to a table.
enum ColumnToken {
    /// Unqualified column, e.g. `email` — table inferred from the FROM clause.
    Bare(String),
    /// Qualified column, e.g. `u.email` or `users.email` — table given explicitly.
    Qualified { qualifier: String, name: String },
}

/// Schema columns for `table` (case-insensitive table match), if known.
fn schema_columns<'a>(table: &str, schema: &'a [(&str, &[String])]) -> Option<&'a [String]> {
    schema
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(table))
        .map(|(_, cols)| *cols)
}

/// Walk an expression tree (WHERE / ON / projection exprs / function args) and
/// collect every column identifier it mentions. Unhandled variants are simply
/// skipped — a missed exotic column only means a slightly less complete stat.
fn collect_expr_columns(expr: &Expr, out: &mut Vec<ColumnToken>) {
    match expr {
        Expr::Identifier(ident) => out.push(ColumnToken::Bare(ident.value.to_lowercase())),
        Expr::CompoundIdentifier(parts) => {
            let mut rev = parts.iter().rev();
            if let Some(name) = rev.next() {
                let name = name.value.to_lowercase();
                match rev.next() {
                    Some(qualifier) => out.push(ColumnToken::Qualified {
                        qualifier: qualifier.value.to_lowercase(),
                        name,
                    }),
                    None => out.push(ColumnToken::Bare(name)),
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_columns(left, out);
            collect_expr_columns(right, out);
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr)
        | Expr::Collate { expr, .. }
        | Expr::Cast { expr, .. } => collect_expr_columns(expr, out),
        Expr::InList { expr, list, .. } => {
            collect_expr_columns(expr, out);
            for e in list {
                collect_expr_columns(e, out);
            }
        }
        Expr::Between { expr, low, high, .. } => {
            collect_expr_columns(expr, out);
            collect_expr_columns(low, out);
            collect_expr_columns(high, out);
        }
        Expr::Like { expr, pattern, .. } => {
            collect_expr_columns(expr, out);
            collect_expr_columns(pattern, out);
        }
        Expr::Tuple(list) => {
            for e in list {
                collect_expr_columns(e, out);
            }
        }
        Expr::Function(func) => collect_function_columns(func, out),
        _ => {}
    }
}

/// Collect column identifiers from a function's arguments, e.g. `count(id)`.
fn collect_function_columns(func: &Function, out: &mut Vec<ColumnToken>) {
    let FunctionArguments::List(list) = &func.args else {
        return;
    };
    for arg in &list.args {
        let arg_expr = match arg {
            FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } => arg,
            FunctionArg::Unnamed(arg) => arg,
        };
        if let FunctionArgExpr::Expr(e) = arg_expr {
            collect_expr_columns(e, out);
        }
    }
}

/// The ON expression of a join, for the constraint-bearing join operators.
fn join_on_expr(op: &JoinOperator) -> Option<&Expr> {
    use JoinOperator::*;
    let constraint = match op {
        Join(c) | Inner(c) | Left(c) | LeftOuter(c) | Right(c) | RightOuter(c) | FullOuter(c)
        | CrossJoin(c) | Semi(c) | LeftSemi(c) | RightSemi(c) | Anti(c) | LeftAnti(c)
        | RightAnti(c) | StraightJoin(c) => c,
        _ => return None,
    };
    match constraint {
        JoinConstraint::On(expr) => Some(expr),
        _ => None,
    }
}

/// First `SELECT` reachable from a query body (descends plain nested queries).
fn first_select(body: &SetExpr) -> Option<&Select> {
    match body {
        SetExpr::Select(sel) => Some(sel),
        SetExpr::Query(inner) => first_select(&inner.body),
        _ => None,
    }
}

/// Resolve one column token to a `(table, column)` pair against the schema.
///
/// Qualified tokens are attributed to the table their alias/name points at.
/// Bare tokens are matched against the tables actually referenced in the query
/// (so a shared column name lands on the queried table, not the first schema
/// match); when no table was referenced we fall back to the whole schema.
fn resolve_token(
    token: &ColumnToken,
    alias_map: &AliasMap,
    referenced: &[String],
    schema: &[(&str, &[String])],
) -> Option<(String, String)> {
    let has_col = |table: &str, col: &str| {
        schema_columns(table, schema).is_some_and(|cols| cols.iter().any(|c| c.eq_ignore_ascii_case(col)))
    };
    match token {
        ColumnToken::Qualified { qualifier, name } => {
            let table = alias_map.resolve(qualifier).map(str::to_string).unwrap_or_else(|| qualifier.clone());
            has_col(&table, name).then(|| (table, name.clone()))
        }
        ColumnToken::Bare(name) => {
            let owner = if referenced.is_empty() {
                schema.iter().map(|(t, _)| t.to_string()).find(|t| has_col(t, name))
            } else {
                referenced.iter().find(|t| has_col(t, name)).cloned()
            };
            owner.map(|t| (t, name.clone()))
        }
    }
}

/// Resolve the columns named by a (possibly qualified) wildcard. `qualifier`
/// is `None` for `*` (expands every referenced table) or the alias/table for
/// `t.*`. Emits one `(table, column)` per expanded column.
fn expand_wildcard(
    qualifier: Option<&str>,
    alias_map: &AliasMap,
    referenced: &[String],
    schema: &[(&str, &[String])],
    out: &mut Vec<(String, String)>,
) {
    let tables: Vec<String> = match qualifier {
        None => referenced.to_vec(),
        Some(q) => vec![alias_map.resolve(q).map(str::to_string).unwrap_or_else(|| q.to_string())],
    };
    for table in tables {
        if let Some(cols) = schema_columns(&table, schema) {
            for col in cols {
                out.push((table.clone(), col.to_lowercase()));
            }
        }
    }
}

/// Resolve the column references in a query against a known schema view
/// (`&[(table, columns)]`), returning `(table, column)` pairs. Covers the
/// SELECT projection (including `*` / `alias.*` wildcards), the WHERE clause and
/// JOIN ... ON conditions, so `\stats <table>` reflects real column usage rather
/// than only explicitly projected identifiers. Lives in `query/` alongside
/// `extract_referenced_tables` so SQL parsing stays out of the API facade;
/// callers pass a schema view rather than the function reaching up into the
/// schema service.
pub fn extract_column_refs(query: &str, schema: &[(&str, &[String])]) -> Vec<(String, String)> {
    let stmts = parse_statements(query);
    let Some(select) = stmts.iter().find_map(|stmt| match stmt {
        Statement::Query(q) => first_select(&q.body),
        _ => None,
    }) else {
        return vec![];
    };

    let alias_map = build_alias_map(query);
    let referenced = extract_referenced_tables(query);

    let mut refs: Vec<(String, String)> = Vec::new();
    let mut tokens: Vec<ColumnToken> = Vec::new();

    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => {
                expand_wildcard(None, &alias_map, &referenced, schema, &mut refs)
            }
            SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(name), _) => {
                if let Some(q) = last_ident(name) {
                    expand_wildcard(Some(&q), &alias_map, &referenced, schema, &mut refs);
                }
            }
            SelectItem::UnnamedExpr(e)
            | SelectItem::ExprWithAlias { expr: e, .. }
            | SelectItem::ExprWithAliases { expr: e, .. } => collect_expr_columns(e, &mut tokens),
            _ => {}
        }
    }

    if let Some(selection) = &select.selection {
        collect_expr_columns(selection, &mut tokens);
    }
    for twj in &select.from {
        for join in &twj.joins {
            if let Some(on) = join_on_expr(&join.join_operator) {
                collect_expr_columns(on, &mut tokens);
            }
        }
    }

    for token in &tokens {
        if let Some(pair) = resolve_token(token, &alias_map, &referenced, schema) {
            refs.push(pair);
        }
    }

    refs.dedup();
    refs.sort();
    refs.dedup();
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

    #[test]
    fn extract_column_refs_expands_unqualified_wildcard() {
        let cols = vec!["id".to_string(), "email".to_string()];
        let schema: Vec<(&str, &[String])> = vec![("users", cols.as_slice())];
        let refs = extract_column_refs("SELECT * FROM users", &schema);
        assert!(refs.contains(&("users".to_string(), "id".to_string())));
        assert!(refs.contains(&("users".to_string(), "email".to_string())));
    }

    #[test]
    fn extract_column_refs_expands_qualified_wildcard() {
        let cols = vec!["id".to_string(), "email".to_string()];
        let schema: Vec<(&str, &[String])> = vec![("users", cols.as_slice())];
        let refs = extract_column_refs("SELECT u.* FROM users u", &schema);
        assert!(refs.contains(&("users".to_string(), "id".to_string())));
        assert!(refs.contains(&("users".to_string(), "email".to_string())));
    }

    #[test]
    fn extract_column_refs_captures_where_columns() {
        let cols = vec!["id".to_string(), "email".to_string()];
        let schema: Vec<(&str, &[String])> = vec![("users", cols.as_slice())];
        let refs = extract_column_refs("SELECT id FROM users WHERE email = 'x'", &schema);
        assert!(refs.contains(&("users".to_string(), "id".to_string())));
        assert!(refs.contains(&("users".to_string(), "email".to_string())));
    }

    #[test]
    fn extract_column_refs_captures_join_on_columns() {
        let users = vec!["id".to_string()];
        let orders = vec!["user_id".to_string(), "id".to_string()];
        let schema: Vec<(&str, &[String])> =
            vec![("users", users.as_slice()), ("orders", orders.as_slice())];
        let refs = extract_column_refs(
            "SELECT u.id FROM users u JOIN orders o ON o.user_id = u.id",
            &schema,
        );
        assert!(refs.contains(&("users".to_string(), "id".to_string())));
        assert!(refs.contains(&("orders".to_string(), "user_id".to_string())));
    }

    #[test]
    fn extract_column_refs_attributes_qualified_to_correct_table() {
        // both tables have `id`; the alias must decide which table owns the ref
        let users = vec!["id".to_string()];
        let orders = vec!["id".to_string()];
        let schema: Vec<(&str, &[String])> =
            vec![("orders", orders.as_slice()), ("users", users.as_slice())];
        let refs = extract_column_refs("SELECT u.id FROM users u", &schema);
        assert_eq!(refs, vec![("users".to_string(), "id".to_string())]);
    }

    #[test]
    fn extract_column_refs_bare_column_prefers_referenced_table() {
        // `accounts` is listed first in schema but not referenced by the query;
        // a bare `id` must attribute to the queried table, not the first match.
        let accounts = vec!["id".to_string()];
        let users = vec!["id".to_string()];
        let schema: Vec<(&str, &[String])> =
            vec![("accounts", accounts.as_slice()), ("users", users.as_slice())];
        let refs = extract_column_refs("SELECT id FROM users", &schema);
        assert_eq!(refs, vec![("users".to_string(), "id".to_string())]);
    }
}
