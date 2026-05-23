use std::collections::HashMap;
use super::tokenizer::{SqlToken, tokenize};

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
}

#[derive(Debug)]
enum AliasState {
    Idle,
    ExpectTable,
    ExpectAlias { candidate: String },
    ExpectQualifiedTable,  // saw "schema.", now expect the actual table name
    ExpectAliasName { candidate: String },
    PostAlias,
    InSubquery { depth: usize },
    ExpectSubqueryAlias,
    ExpectSubqueryAliasName,
}

pub fn build_alias_map(line: &str) -> AliasMap {
    // NOTE: schema-qualified table names (e.g. FROM public.users u) are not handled —
    // the dot is parsed as Other('.') which disrupts the alias extraction for that table.
    let mut map: HashMap<String, Option<String>> = HashMap::new();
    let mut state = AliasState::Idle;

    for token in tokenize(line) {
        if let SqlToken::Other(c) = token {
            if c.is_whitespace() {
                continue;
            }
            state = match (state, c) {
                (AliasState::ExpectTable, '(') => AliasState::InSubquery { depth: 1 },
                (AliasState::ExpectAlias { .. }, '.') => AliasState::ExpectQualifiedTable,
                (AliasState::ExpectAlias { .. }, ',') => AliasState::ExpectTable,
                (AliasState::PostAlias, ',') => AliasState::ExpectTable,
                (AliasState::InSubquery { depth }, '(') => AliasState::InSubquery { depth: depth + 1 },
                (AliasState::InSubquery { depth }, ')') => {
                    if depth == 1 {
                        AliasState::ExpectSubqueryAlias
                    } else {
                        AliasState::InSubquery { depth: depth - 1 }
                    }
                }
                (AliasState::InSubquery { depth }, _) => AliasState::InSubquery { depth },
                (s, _) => s,
            };
            continue;
        }
        state = match (state, token) {
            (AliasState::Idle, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::ExpectTable, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                AliasState::ExpectAlias { candidate: w.to_lowercase() }
            }
            (AliasState::ExpectTable, _) => AliasState::Idle,
            (AliasState::ExpectAlias { candidate }, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                AliasState::ExpectAliasName { candidate }
            }
            (AliasState::ExpectAlias { candidate }, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                map.insert(w.to_lowercase(), Some(candidate));
                AliasState::PostAlias
            }
            (AliasState::ExpectAlias { .. }, _) => AliasState::Idle,
            // After "schema.", the next word is the actual table name.
            (AliasState::ExpectQualifiedTable, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                AliasState::ExpectAlias { candidate: w.to_lowercase() }
            }
            (AliasState::ExpectQualifiedTable, _) => AliasState::Idle,
            (AliasState::ExpectAliasName { candidate }, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), Some(candidate));
                AliasState::PostAlias
            }
            (AliasState::ExpectAliasName { .. }, _) => AliasState::Idle,
            (AliasState::PostAlias, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::PostAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                AliasState::ExpectSubqueryAliasName
            }
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                map.insert(w.to_lowercase(), None);
                AliasState::PostAlias
            }
            (AliasState::ExpectSubqueryAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAliasName, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), None);
                AliasState::PostAlias
            }
            (AliasState::ExpectSubqueryAliasName, _) => AliasState::Idle,
            (s, _) => s,
        };
    }

    AliasMap { map }
}

pub struct JoinContext {
    pub right_table: String,
    pub left_tables: Vec<String>,
}

pub fn extract_join_context(upper_query: &str, alias_map: &AliasMap) -> Option<JoinContext> {
    let tokens: Vec<&str> = upper_query.split_whitespace().collect();

    let last_join_pos = tokens.iter().rposition(|&t| t == "JOIN")?;

    let right_raw = tokens.get(last_join_pos + 1)?.to_lowercase();
    // Strip schema prefix: "public.orders" → "orders"
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
}
