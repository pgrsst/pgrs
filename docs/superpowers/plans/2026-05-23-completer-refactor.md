# Completer Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve four code quality issues in the REPL completer: extract a SQL constant, split a 1408-line file into focused modules, break up an oversized method, and replace a test helper that duplicates production logic.

**Architecture:** `completer.rs` is split into three sibling modules under `repl/`: `tokenizer.rs` (lexer), `alias.rs` (SQL alias resolution + keywords), and a slimmed `completer.rs` (completion/highlighting/hinting). `SqlCompleter::complete_input` is decomposed into three private methods. The `highlight_sql` test helper is deleted and its tests are rewritten to go through the production `SqlHighlighter::highlight` code path.

**Tech Stack:** Rust, reedline, nu_ansi_term.

---

## File Map

| File | Action | Responsibility after change |
|------|--------|-----------------------------|
| `src/adapters/driving/repl/mod.rs` | Modify | Add `mod tokenizer; mod alias;`, add `LIST_DATABASES_SQL` constant |
| `src/adapters/driving/repl/tokenizer.rs` | **Create** | `SqlToken` enum, `tokenize` fn |
| `src/adapters/driving/repl/alias.rs` | **Create** | `SQL_KEYWORDS`, `AliasMap`, `AliasState`, `JoinContext`, `build_alias_map`, `extract_join_context` + their tests |
| `src/adapters/driving/repl/completer.rs` | Modify (reduce) | `CompletionKind`, `common_prefix`, `word_start`, `classify_word`, `SqlCompleter` (with refactored `complete_input`), `SqlHighlighter`, `SqlHinter` + tests using `render_to_ansi` |

---

## Task 1: Issue #4 — Extract `LIST_DATABASES_SQL` constant

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

This is the simplest change: lift the inline SQL string in `handle_l` into a module-level constant.

- [ ] **Step 1: Add the constant above `handle_l` and use it**

In `src/adapters/driving/repl/mod.rs`, replace the inline SQL in `handle_l`:

```rust
// Before — add this constant anywhere above handle_l
const LIST_DATABASES_SQL: &str =
    "SELECT datname AS database \
     FROM pg_database \
     WHERE datistemplate = false \
     ORDER BY datname";

// Change handle_l from:
fn handle_l(conn: &dyn DbConnection, expanded: bool, writer: &mut impl Write) {
    match conn.execute(
        "SELECT datname AS database \
         FROM pg_database \
         WHERE datistemplate = false \
         ORDER BY datname",
    ) {
        Ok(result) => write!(writer, "{}", format_result(&result, expanded)).ok(),
        Err(e) => { eprintln!("error: {}", e); None }
    };
}

// To:
fn handle_l(conn: &dyn DbConnection, expanded: bool, writer: &mut impl Write) {
    match conn.execute(LIST_DATABASES_SQL) {
        Ok(result) => write!(writer, "{}", format_result(&result, expanded)).ok(),
        Err(e) => { eprintln!("error: {}", e); None }
    };
}
```

- [ ] **Step 2: Verify all tests pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: `test result: ok. 257 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "refactor(repl): extract LIST_DATABASES_SQL constant"
```

---

## Task 2: Issue #1 Part A — Create `tokenizer.rs`

**Files:**
- Create: `src/adapters/driving/repl/tokenizer.rs`
- Modify: `src/adapters/driving/repl/mod.rs` (add `mod tokenizer;`)
- Modify: `src/adapters/driving/repl/completer.rs` (remove `SqlToken` and `tokenize`)

Move `SqlToken` and `tokenize` from `completer.rs` into a new focused module. No behavior changes — just relocation.

- [ ] **Step 1: Create `tokenizer.rs` with the moved code**

Create `src/adapters/driving/repl/tokenizer.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SqlToken {
    Comment(String),
    StringLiteral(String),
    Number(String),
    Word(String),
    Other(char),
}

pub fn tokenize(input: &str) -> Vec<SqlToken> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < len {
        if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
            let start = i;
            while i < len && chars[i] != '\n' { i += 1; }
            tokens.push(SqlToken::Comment(chars[start..i].iter().collect()));
        } else if chars[i] == '\'' {
            let start = i;
            i += 1;
            loop {
                if i >= len { break; }
                if chars[i] == '\'' {
                    i += 1;
                    if i < len && chars[i] == '\'' { i += 1; } else { break; }
                } else { i += 1; }
            }
            tokens.push(SqlToken::StringLiteral(chars[start..i].iter().collect()));
        } else if chars[i].is_ascii_digit() {
            let start = i;
            let mut has_dot = false;
            while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                if chars[i] == '.' { has_dot = true; }
                i += 1;
            }
            tokens.push(SqlToken::Number(chars[start..i].iter().collect()));
        } else if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            tokens.push(SqlToken::Word(chars[start..i].iter().collect()));
        } else {
            tokens.push(SqlToken::Other(chars[i]));
            i += 1;
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_keyword_becomes_word() {
        let tokens = tokenize("SELECT");
        assert_eq!(tokens, vec![SqlToken::Word("SELECT".to_string())]);
    }

    #[test]
    fn tokenize_string_literal() {
        let tokens = tokenize("'hello'");
        assert_eq!(tokens, vec![SqlToken::StringLiteral("'hello'".to_string())]);
    }

    #[test]
    fn tokenize_number_integer() {
        let tokens = tokenize("42");
        assert_eq!(tokens, vec![SqlToken::Number("42".to_string())]);
    }

    #[test]
    fn tokenize_number_decimal() {
        let tokens = tokenize("3.14");
        assert_eq!(tokens, vec![SqlToken::Number("3.14".to_string())]);
    }

    #[test]
    fn tokenize_comment_to_eol() {
        let tokens = tokenize("-- note");
        assert_eq!(tokens, vec![SqlToken::Comment("-- note".to_string())]);
    }

    #[test]
    fn tokenize_escaped_single_quote_in_string() {
        let tokens = tokenize("'O''Brien'");
        assert_eq!(tokens, vec![SqlToken::StringLiteral("'O''Brien'".to_string())]);
    }

    #[test]
    fn tokenize_number_trailing_dot_not_consumed() {
        let tokens = tokenize("10.");
        assert_eq!(tokens[0], SqlToken::Number("10".to_string()));
        assert_eq!(tokens[1], SqlToken::Other('.'));
    }
}
```

- [ ] **Step 2: Declare `mod tokenizer;` in `repl/mod.rs`**

In `src/adapters/driving/repl/mod.rs`, add at the top (after `mod completer;`):

```rust
mod completer;
mod executor;
mod tokenizer;
```

- [ ] **Step 3: Remove `SqlToken` and `tokenize` from `completer.rs` and add the import**

In `src/adapters/driving/repl/completer.rs`:

Remove the `SqlToken` enum definition (lines starting with `#[derive(Debug, Clone, PartialEq)] pub enum SqlToken`) and the `tokenize` function.

Add at the top of `completer.rs`:

```rust
use super::tokenizer::{SqlToken, tokenize};
```

- [ ] **Step 4: Verify compilation and all tests pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: `test result: ok. 264 passed; 0 failed` (257 + 7 new tokenizer tests)

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/tokenizer.rs \
        src/adapters/driving/repl/mod.rs \
        src/adapters/driving/repl/completer.rs
git commit -m "refactor(repl): extract SqlToken and tokenize into tokenizer.rs"
```

---

## Task 3: Issue #1 Part B — Create `alias.rs`

**Files:**
- Create: `src/adapters/driving/repl/alias.rs`
- Modify: `src/adapters/driving/repl/mod.rs` (add `mod alias;`)
- Modify: `src/adapters/driving/repl/completer.rs` (remove moved code, add import, fix `extract_table_refs`)

Move `SQL_KEYWORDS`, `AliasMap`, `AliasState`, `JoinContext`, `build_alias_map`, `extract_join_context` out of `completer.rs`. Add `real_tables()` method to `AliasMap` so callers don't need access to the private `map` field.

- [ ] **Step 1: Create `alias.rs`**

Create `src/adapters/driving/repl/alias.rs`:

```rust
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
            state = match (state, SqlToken::Other(c)) {
                (AliasState::ExpectTable, SqlToken::Other('(')) => {
                    AliasState::InSubquery { depth: 1 }
                }
                (AliasState::ExpectAlias { .. }, SqlToken::Other(',')) => AliasState::ExpectTable,
                (AliasState::PostAlias, SqlToken::Other(',')) => AliasState::ExpectTable,
                (AliasState::InSubquery { depth }, SqlToken::Other('(')) => {
                    AliasState::InSubquery { depth: depth + 1 }
                }
                (AliasState::InSubquery { depth }, SqlToken::Other(')')) => {
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
    let right_table = alias_map
        .resolve(&right_raw)
        .map(|s| s.to_string())
        .unwrap_or(right_raw);

    let left_tables: Vec<String> = tokens
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| {
            if (w[0] == "FROM" || w[0] == "JOIN" || w[0] == "UPDATE") && i != last_join_pos {
                Some(w[1].to_lowercase())
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
}
```

- [ ] **Step 2: Declare `mod alias;` in `repl/mod.rs`**

In `src/adapters/driving/repl/mod.rs`, update the module declarations:

```rust
mod alias;
mod completer;
mod executor;
mod tokenizer;
```

- [ ] **Step 3: Remove moved code from `completer.rs` and add imports**

In `src/adapters/driving/repl/completer.rs`:

**Remove** these items (they now live in `alias.rs`):
- `struct AliasMap { map: ... }` and its `impl`
- `enum AliasState { ... }` and its states
- `fn build_alias_map(line: &str) -> AliasMap`
- `struct JoinContext { right_table, left_tables }`
- `fn extract_join_context(...)`
- `const SQL_KEYWORDS: &[&str]`

**Add** at the top of `completer.rs`:

```rust
use super::alias::{build_alias_map, extract_join_context, AliasMap, JoinContext, SQL_KEYWORDS};
use super::tokenizer::{SqlToken, tokenize};
```

(Remove the existing `use super::tokenizer::{SqlToken, tokenize};` if present from Task 2, replace with the combined import above.)

**Fix `extract_table_refs`** to use `alias_map.real_tables()` instead of accessing the private `map` field:

```rust
fn extract_table_refs(&self, upper_query: &str, alias_map: &AliasMap) -> Vec<String> {
    let tokens: Vec<&str> = upper_query.split_whitespace().collect();
    let trigger = ["FROM", "JOIN", "UPDATE"];
    let mut refs: Vec<String> = tokens
        .windows(2)
        .filter_map(|w| trigger.contains(&w[0]).then_some(w[1].to_lowercase()))
        .collect();
    for real_table in alias_map.real_tables() {
        if !refs.iter().any(|r| r == real_table) {
            refs.push(real_table.to_string());
        }
    }
    refs
}
```

**Remove** the alias-only tests from `completer.rs` (they now live in `alias.rs`). Tests to remove:
- `alias_map_resolve_known_alias`
- `alias_map_resolve_unknown_returns_none`
- `alias_map_resolve_subquery_alias_returns_none`
- `build_alias_map_from_without_as`
- `build_alias_map_from_with_as`
- `build_alias_map_comma_separated`
- `build_alias_map_join_alias`
- `build_alias_map_subquery_with_as`
- `build_alias_map_subquery_without_as`
- `build_alias_map_table_without_alias_not_in_map`
- `extract_join_context_finds_right_and_left_tables`
- `extract_join_context_no_join_returns_none`
- `extract_join_context_multi_join_uses_last`
- `extract_join_context_resolves_aliases`

Keep all other tests (alias integration tests that use `SqlCompleter`, common_prefix tests, completion tests, highlighting tests, hinting tests).

Also add imports inside the `#[cfg(test)]` block in `completer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::alias::build_alias_map;  // for alias integration tests that call build_alias_map
    use std::collections::HashMap;
    // ...
}
```

- [ ] **Step 4: Verify compilation and all tests pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: `test result: ok. 278 passed; 0 failed` (264 + 14 alias tests)

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/alias.rs \
        src/adapters/driving/repl/mod.rs \
        src/adapters/driving/repl/completer.rs
git commit -m "refactor(repl): extract alias resolution into alias.rs"
```

---

## Task 4: Issue #2 — Extract methods from `complete_input`

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

`complete_input` is 65 lines with three concerns: qualified-name completion, trigger resolution, and filtering/sorting. Extract each concern into a focused private method or function.

- [ ] **Step 1: Add `try_complete_qualified` method to `SqlCompleter`**

In `completer.rs`, inside `impl SqlCompleter`, add:

```rust
fn try_complete_qualified(
    &self,
    input: &str,
    alias_map: &AliasMap,
) -> Option<Vec<(String, CompletionKind)>> {
    let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let token = &input[last_ws..];
    let dot_pos = token.rfind('.')?;
    let table_name = token[..dot_pos]
        .split('.')
        .next_back()
        .unwrap_or(&token[..dot_pos])
        .to_lowercase();
    let col_prefix = token[dot_pos + 1..].to_uppercase();
    Some(self.complete_qualified(&table_name, &col_prefix, alias_map))
}
```

- [ ] **Step 2: Add `resolve_trigger_and_word` free function**

In `completer.rs`, outside `impl SqlCompleter` (but inside the file), add:

```rust
fn resolve_trigger_and_word(input: &str) -> (String, String) {
    let upper = input.to_uppercase();
    let tokens: Vec<&str> = upper.split_whitespace().collect();

    let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
        String::new()
    } else {
        tokens.last().copied().unwrap_or("").to_string()
    };

    let effective_trigger = if TABLE_TRIGGERS.contains(&current_word.as_str())
        || COLUMN_TRIGGERS.contains(&current_word.as_str())
    {
        current_word.clone()
    } else if input.ends_with(char::is_whitespace) {
        tokens.last().copied().unwrap_or("").to_string()
    } else if tokens.len() >= 2 {
        tokens[tokens.len() - 2].to_string()
    } else {
        String::new()
    };

    (effective_trigger, current_word)
}
```

- [ ] **Step 3: Add `filter_and_sort` method to `SqlCompleter`**

In `impl SqlCompleter`, add:

```rust
fn filter_and_sort(
    &self,
    candidates: Vec<(String, CompletionKind)>,
    effective_trigger: &str,
    current_word: &str,
) -> Vec<(String, CompletionKind)> {
    let is_trigger = TABLE_TRIGGERS.contains(&current_word)
        || COLUMN_TRIGGERS.contains(&current_word);
    let prefix_upper = if is_trigger {
        String::new()
    } else {
        current_word.to_uppercase()
    };

    // For JOIN ON, preserve intentional shared-column priority ordering
    if effective_trigger == "ON" {
        let mut seen = std::collections::HashSet::new();
        return candidates
            .into_iter()
            .filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))
            .filter(|(c, _)| seen.insert(c.clone()))
            .collect();
    }

    let mut results: Vec<(String, CompletionKind)> = candidates
        .into_iter()
        .filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))
        .collect();

    results.sort_by(|a, b| match (&a.1, &b.1) {
        (CompletionKind::Keyword, CompletionKind::Keyword) => a.0.cmp(&b.0),
        _ => a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)),
    });
    results.dedup_by(|a, b| a.0 == b.0);
    results
}
```

- [ ] **Step 4: Rewrite `complete_input` to use the extracted methods**

Replace the current `complete_input` body with:

```rust
pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
    let alias_map = build_alias_map(line);
    let input = &line[..pos];

    if let Some(result) = self.try_complete_qualified(input, &alias_map) {
        return result;
    }

    let (effective_trigger, current_word) = resolve_trigger_and_word(input);
    let candidates =
        self.candidates_for_trigger(&effective_trigger, &line.to_uppercase(), &alias_map);
    self.filter_and_sort(candidates, &effective_trigger, &current_word)
}
```

- [ ] **Step 5: Verify all tests still pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: same count as after Task 3, 0 failed.

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "refactor(repl): decompose complete_input into focused methods"
```

---

## Task 5: Issue #3 — Replace `highlight_sql` test helper

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

Remove the `#[cfg(test)]` `highlight_sql` free function and replace all uses with a `render_to_ansi` helper that goes through the production `SqlHighlighter::highlight` code path. This ensures style changes in `CompletionKind::style()` are caught by tests.

- [ ] **Step 1: Add `render_to_ansi` test helper**

In the `#[cfg(test)] mod tests` block inside `completer.rs`, add:

```rust
use reedline::Highlighter;

fn render_to_ansi(line: &str, schema: SchemaService) -> String {
    let h = SqlHighlighter::new(schema);
    h.highlight(line, 0)
        .buffer
        .iter()
        .map(|(style, text)| style.paint(text).to_string())
        .collect()
}
```

- [ ] **Step 2: Rewrite the `highlight_*` tests to use `render_to_ansi`**

Replace the following tests. Note: assertions now use `CompletionKind::X.style().paint(...)` so that if a color changes in production, the test breaks immediately.

```rust
#[test]
fn highlight_keyword_bold_cyan() {
    let rendered = render_to_ansi("SELECT", schema_with(&[], &[]));
    let expected = CompletionKind::Keyword.style().paint("SELECT").to_string();
    assert!(rendered.contains(&expected), "keyword not styled correctly, got: {rendered}");
}

#[test]
fn highlight_string_literal_yellow() {
    let rendered = render_to_ansi("'hello'", schema_with(&[], &[]));
    // String literals are not in CompletionKind — check the actual nu_ansi_term color
    use nu_ansi_term::Color;
    let expected = Color::Yellow.paint("'hello'").to_string();
    assert!(rendered.contains(&expected), "string literal not yellow, got: {rendered}");
}

#[test]
fn highlight_number_magenta() {
    let rendered = render_to_ansi("42", schema_with(&[], &[]));
    use nu_ansi_term::Color;
    let expected = Color::Magenta.paint("42").to_string();
    assert!(rendered.contains(&expected), "number not magenta, got: {rendered}");
}

#[test]
fn highlight_comment_dim() {
    let rendered = render_to_ansi("-- comment", schema_with(&[], &[]));
    use nu_ansi_term::Style;
    let expected = Style::new().dimmed().paint("-- comment").to_string();
    assert!(rendered.contains(&expected), "comment not dim, got: {rendered}");
}

#[test]
fn highlight_table_name_bold_yellow() {
    let schema = schema_with(&["users"], &[]);
    let rendered = render_to_ansi("users", schema);
    let expected = CompletionKind::Table.style().paint("users").to_string();
    assert!(rendered.contains(&expected), "table not styled correctly, got: {rendered}");
}

#[test]
fn highlight_column_name_green() {
    let schema = schema_with(&[], &[("_dummy", &["email"])]);
    let rendered = render_to_ansi("email", schema);
    let expected = CompletionKind::Column.style().paint("email").to_string();
    assert!(rendered.contains(&expected), "column not styled correctly, got: {rendered}");
}

#[test]
fn highlight_plain_word_no_escape() {
    let rendered = render_to_ansi("foo", schema_with(&[], &[]));
    assert!(!rendered.contains('\x1b'), "unknown word should have no ANSI escape, got: {rendered}");
}

#[test]
fn highlight_number_trailing_dot_not_consumed() {
    let rendered = render_to_ansi("10.", schema_with(&[], &[]));
    use nu_ansi_term::Color;
    let expected_num = Color::Magenta.paint("10").to_string();
    assert!(rendered.contains(&expected_num), "10 should be magenta, got: {rendered}");
    assert!(rendered.ends_with('.'), "trailing dot should be plain, got: {rendered}");
}

#[test]
fn highlight_number_decimal_consumed() {
    let rendered = render_to_ansi("3.14", schema_with(&[], &[]));
    use nu_ansi_term::Color;
    let expected = Color::Magenta.paint("3.14").to_string();
    assert!(rendered.contains(&expected), "3.14 should be one magenta span, got: {rendered}");
}

#[test]
fn highlight_string_with_escaped_quote() {
    let rendered = render_to_ansi("'O''Brien'", schema_with(&[], &[]));
    use nu_ansi_term::Color;
    let expected = Color::Yellow.paint("'O''Brien'").to_string();
    assert_eq!(rendered, expected);
}

#[test]
fn highlight_mixed_query() {
    let schema = schema_with(&["users"], &[]);
    let rendered = render_to_ansi("SELECT * FROM users WHERE id = 1", schema);
    let kw_select = CompletionKind::Keyword.style().paint("SELECT").to_string();
    let tbl_users = CompletionKind::Table.style().paint("users").to_string();
    use nu_ansi_term::Color;
    let num_1 = Color::Magenta.paint("1").to_string();
    assert!(rendered.contains(&kw_select), "SELECT should be keyword style, got: {rendered}");
    assert!(rendered.contains(&tbl_users), "users should be table style, got: {rendered}");
    assert!(rendered.contains(&num_1), "1 should be magenta, got: {rendered}");
}
```

- [ ] **Step 3: Remove the old `highlight_sql` function**

Delete the `#[cfg(test)]` `fn highlight_sql(line: &str, tables: &[String], columns: &[String]) -> String` from `completer.rs` (and its two blank lines). It is no longer referenced.

- [ ] **Step 4: Verify all tests pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: same count, 0 failed.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "refactor(repl): replace highlight_sql test helper with render_to_ansi via production SqlHighlighter"
```

---

## Self-Review

**Spec coverage:**
- Issue #1 (split file) → Tasks 2 + 3 ✅
- Issue #2 (split complete_input) → Task 4 ✅
- Issue #3 (replace highlight_sql) → Task 5 ✅
- Issue #4 (extract constant) → Task 1 ✅

**Placeholder scan:** No TBDs, all code blocks complete, all test assertions present.

**Type consistency:**
- `AliasMap::real_tables()` defined in Task 3 Step 1, used in Task 3 Step 3 ✅
- `try_complete_qualified` / `resolve_trigger_and_word` / `filter_and_sort` defined and called in Task 4 ✅
- `render_to_ansi` defined and used within Task 5 ✅
- `JoinContext.right_table` / `JoinContext.left_tables` made `pub` in alias.rs so `candidates_for_trigger` in `completer.rs` can access them ✅

**Note on test count:** The number of passing tests increases as new tokenizer and alias tests are added in Tasks 2 and 3. The highlight tests in Task 5 replace existing tests one-for-one, so the count stays the same after Task 5.
