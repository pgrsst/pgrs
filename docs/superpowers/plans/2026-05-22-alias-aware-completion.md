# Alias-Aware SQL Autocomplete — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add alias resolution to `SqlCompleter` so `SELECT u.` after `FROM users u` suggests columns from `users`.

**Architecture:** A new `AliasMap` struct (private to `completer.rs`) is built once per completion request by walking the existing `tokenize()` token stream. `complete_qualified` resolves the alias name before schema lookup; `extract_table_refs` appends alias-resolved real table names to the trigger-based path.

**Tech Stack:** Rust, `std::collections::HashMap`, existing `tokenize()` and `SQL_KEYWORDS` in `completer.rs`

---

## File Map

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `AliasMap`, `AliasState`, `build_alias_map`; update `complete_qualified`, `candidates_for_trigger`, `extract_table_refs`, `complete_input` |

No other files are touched.

---

### Task 1: AliasMap struct

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block in `completer.rs`:

```rust
#[test]
fn alias_map_resolve_known_alias() {
    let mut m = AliasMap { map: std::collections::HashMap::new() };
    m.map.insert("u".to_string(), Some("users".to_string()));
    assert_eq!(m.resolve("u"), Some("users"));
}

#[test]
fn alias_map_resolve_unknown_returns_none() {
    let m = AliasMap { map: std::collections::HashMap::new() };
    assert_eq!(m.resolve("x"), None);
}

#[test]
fn alias_map_resolve_subquery_alias_returns_none() {
    let mut m = AliasMap { map: std::collections::HashMap::new() };
    m.map.insert("s".to_string(), None);
    assert_eq!(m.resolve("s"), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test alias_map_resolve 2>&1 | head -20
```
Expected: compile error — `AliasMap` not defined.

- [ ] **Step 3: Add `use` import and `AliasMap` struct**

Add at the top of `completer.rs`, after the existing `use` lines:

```rust
use std::collections::HashMap;
```

Then add after the existing `use` imports, before `const SQL_KEYWORDS`:

```rust
struct AliasMap {
    map: HashMap<String, Option<String>>,
}

impl AliasMap {
    fn resolve<'a>(&self, name: &'a str) -> Option<&str> {
        self.map.get(name).and_then(|v| v.as_deref())
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test alias_map_resolve
```
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): add AliasMap struct with resolve method"
```

---

### Task 2: build_alias_map — FROM/JOIN basic aliases

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]`:

```rust
#[test]
fn build_alias_map_from_without_as() {
    let m = build_alias_map("SELECT * FROM users u");
    assert_eq!(m.resolve("u"), Some("users"));
}

#[test]
fn build_alias_map_from_with_as() {
    let m = build_alias_map("SELECT * FROM users AS u");
    assert_eq!(m.resolve("u"), Some("users"));
}

#[test]
fn build_alias_map_join_alias() {
    let m = build_alias_map("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
    assert_eq!(m.resolve("u"), Some("users"));
    assert_eq!(m.resolve("o"), Some("orders"));
}

#[test]
fn build_alias_map_table_without_alias_not_in_map() {
    let m = build_alias_map("SELECT * FROM users");
    assert_eq!(m.resolve("users"), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test build_alias_map 2>&1 | head -20
```
Expected: compile error — `build_alias_map` not defined.

- [ ] **Step 3: Implement AliasState enum and build_alias_map (basic states only)**

Add after `AliasMap` impl block, before `fn word_start`:

```rust
#[derive(Debug)]
enum AliasState {
    Idle,
    ExpectTable,
    ExpectAlias { candidate: String },
    ExpectAliasName { candidate: String },
}

fn build_alias_map(line: &str) -> AliasMap {
    let mut map: HashMap<String, Option<String>> = HashMap::new();
    let mut state = AliasState::Idle;

    for token in tokenize(line) {
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
                AliasState::Idle
            }
            (AliasState::ExpectAlias { .. }, SqlToken::Other(',')) => AliasState::ExpectTable,
            (AliasState::ExpectAlias { .. }, _) => AliasState::Idle,
            (AliasState::ExpectAliasName { candidate }, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), Some(candidate));
                AliasState::Idle
            }
            (AliasState::ExpectAliasName { .. }, _) => AliasState::Idle,
            (AliasState::Idle, _) => AliasState::Idle,
        };
    }

    AliasMap { map }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test build_alias_map
```
Expected: all 4 `build_alias_map_*` tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): implement build_alias_map for FROM/JOIN aliases"
```

---

### Task 3: build_alias_map — comma-separated + subquery aliases

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]`:

```rust
#[test]
fn build_alias_map_comma_separated() {
    let m = build_alias_map("SELECT * FROM users u, orders o");
    assert_eq!(m.resolve("u"), Some("users"));
    assert_eq!(m.resolve("o"), Some("orders"));
}

#[test]
fn build_alias_map_subquery_with_as() {
    let m = build_alias_map("SELECT * FROM (SELECT id FROM users) AS s");
    assert_eq!(m.resolve("s"), None);
}

#[test]
fn build_alias_map_subquery_without_as() {
    let m = build_alias_map("SELECT * FROM (SELECT id FROM users) s");
    assert_eq!(m.resolve("s"), None);
}
```

- [ ] **Step 2: Run tests to identify which ones fail**

```bash
cargo test build_alias_map 2>&1 | tail -20
```
Expected: `build_alias_map_subquery_with_as` and `build_alias_map_subquery_without_as` fail. `build_alias_map_comma_separated` may already pass.

- [ ] **Step 3: Replace AliasState enum and build_alias_map with subquery-aware version**

Replace the entire `AliasState` enum and `build_alias_map` function with:

```rust
#[derive(Debug)]
enum AliasState {
    Idle,
    ExpectTable,
    ExpectAlias { candidate: String },
    ExpectAliasName { candidate: String },
    InSubquery { depth: usize },
    ExpectSubqueryAlias,
    ExpectSubqueryAliasName,
}

fn build_alias_map(line: &str) -> AliasMap {
    let mut map: HashMap<String, Option<String>> = HashMap::new();
    let mut state = AliasState::Idle;

    for token in tokenize(line) {
        state = match (state, token) {
            (AliasState::Idle, SqlToken::Word(w))
                if matches!(w.to_uppercase().as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO") =>
            {
                AliasState::ExpectTable
            }
            (AliasState::ExpectTable, SqlToken::Other('(')) => {
                AliasState::InSubquery { depth: 1 }
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
                AliasState::Idle
            }
            (AliasState::ExpectAlias { .. }, SqlToken::Other(',')) => AliasState::ExpectTable,
            (AliasState::ExpectAlias { .. }, _) => AliasState::Idle,
            (AliasState::ExpectAliasName { candidate }, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), Some(candidate));
                AliasState::Idle
            }
            (AliasState::ExpectAliasName { .. }, _) => AliasState::Idle,
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
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if w.to_uppercase() == "AS" =>
            {
                AliasState::ExpectSubqueryAliasName
            }
            (AliasState::ExpectSubqueryAlias, SqlToken::Word(w))
                if !SQL_KEYWORDS.contains(&w.to_uppercase().as_str()) =>
            {
                map.insert(w.to_lowercase(), None);
                AliasState::Idle
            }
            (AliasState::ExpectSubqueryAlias, _) => AliasState::Idle,
            (AliasState::ExpectSubqueryAliasName, SqlToken::Word(w)) => {
                map.insert(w.to_lowercase(), None);
                AliasState::Idle
            }
            (AliasState::ExpectSubqueryAliasName, _) => AliasState::Idle,
            (AliasState::Idle, _) => AliasState::Idle,
        };
    }

    AliasMap { map }
}
```

- [ ] **Step 4: Run all alias map tests**

```bash
cargo test build_alias_map
```
Expected: all 7 `build_alias_map_*` tests pass.

- [ ] **Step 5: Run full test suite**

```bash
cargo test
```
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): extend alias map with subquery alias support"
```

---

### Task 4: Wire AliasMap into complete_qualified

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)]`:

```rust
#[test]
fn alias_simple() {
    let schema = schema_with(&["users"], &[("users", &["id", "email", "created_at"])]);
    let c = SqlCompleter::new(schema);
    // cursor at pos 9 — "SELECT u." — alias defined later in full line
    let results = c.complete_input("SELECT u. FROM users u", 9);
    assert!(
        results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
        "expected id [column] via alias u, got: {:?}",
        results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
    assert!(results.iter().any(|(r, _)| r == "email"));
}

#[test]
fn alias_with_as() {
    let schema = schema_with(&["users"], &[("users", &["id", "email"])]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("SELECT u. FROM users AS u", 9);
    assert!(results.iter().any(|(r, _)| r == "id"), "expected id via AS alias");
    assert!(results.iter().any(|(r, _)| r == "email"));
}

#[test]
fn alias_prefix_filter() {
    let schema = schema_with(&["users"], &[("users", &["id", "email", "created_at"])]);
    let c = SqlCompleter::new(schema);
    // "SELECT u.em" — pos=11
    let results = c.complete_input("SELECT u.em FROM users u", 11);
    assert!(results.iter().any(|(r, _)| r == "email"), "expected email");
    assert!(!results.iter().any(|(r, _)| r == "id"), "id should not appear");
    assert!(!results.iter().any(|(r, _)| r == "created_at"), "created_at should not appear");
}

#[test]
fn multi_alias() {
    let schema = schema_with(
        &["users", "orders"],
        &[("users", &["id", "email"]), ("orders", &["id", "user_id"])],
    );
    let c = SqlCompleter::new(schema);
    // "SELECT o." — pos=9 — alias o resolves to orders
    let results = c.complete_input("SELECT o. FROM users u JOIN orders o ON u.id = o.user_id", 9);
    assert!(
        results.iter().any(|(r, _)| r == "user_id"),
        "expected user_id from orders via alias o, got: {:?}",
        results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
    assert!(!results.iter().any(|(r, _)| r == "email"), "email from users should not appear");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test alias_simple alias_with_as alias_prefix_filter multi_alias 2>&1 | tail -20
```
Expected: tests fail — alias not recognized.

- [ ] **Step 3: Update complete_qualified to accept AliasMap**

Replace the existing `complete_qualified` method:

```rust
fn complete_qualified(&self, table_name: &str, col_prefix: &str, alias_map: &AliasMap) -> Vec<(String, CompletionKind)> {
    let resolved = alias_map.resolve(table_name).unwrap_or(table_name);
    let cols = self.schema.columns_for(resolved);
    if !cols.is_empty() {
        cols.iter()
            .filter(|c| c.to_uppercase().starts_with(col_prefix))
            .map(|c| (c.to_string(), CompletionKind::Column))
            .collect()
    } else {
        // Table not found: fallback to all columns
        self.schema
            .tables()
            .iter()
            .flat_map(|t| self.schema.columns_for(t).iter().cloned())
            .filter(|c| c.to_uppercase().starts_with(col_prefix))
            .map(|c| (c, CompletionKind::Column))
            .collect()
    }
}
```

- [ ] **Step 4: Build alias_map in complete_input and thread into complete_qualified**

Replace the body of `complete_input` with:

```rust
pub fn complete_input(&self, line: &str, pos: usize) -> Vec<(String, CompletionKind)> {
    let alias_map = build_alias_map(line);
    let input = &line[..pos];

    // Qualified name: "table.col_prefix" or "schema.table.col_prefix"
    let last_ws = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let token = &input[last_ws..];
    if let Some(dot_pos) = token.rfind('.') {
        let table_name = token[..dot_pos]
            .split('.')
            .next_back()
            .unwrap_or(&token[..dot_pos])
            .to_lowercase();
        let col_prefix = token[dot_pos + 1..].to_uppercase();
        return self.complete_qualified(&table_name, &col_prefix, &alias_map);
    }

    let upper = input.to_uppercase();
    let tokens: Vec<&str> = upper.split_whitespace().collect();

    let current_word = if input.ends_with(char::is_whitespace) || input.is_empty() {
        ""
    } else {
        tokens.last().copied().unwrap_or("")
    };

    let effective_trigger = if TABLE_TRIGGERS.contains(&current_word) || COLUMN_TRIGGERS.contains(&current_word) {
        current_word
    } else if input.ends_with(char::is_whitespace) {
        tokens.last().copied().unwrap_or("")
    } else if tokens.len() >= 2 {
        tokens[tokens.len() - 2]
    } else {
        ""
    };

    let full_upper = line.to_uppercase();
    let candidates = self.candidates_for_trigger(effective_trigger, &full_upper);

    let is_trigger = TABLE_TRIGGERS.contains(&current_word) || COLUMN_TRIGGERS.contains(&current_word);
    let prefix_upper = if is_trigger { String::new() } else { current_word.to_uppercase() };

    let mut results: Vec<(String, CompletionKind)> = candidates
        .into_iter()
        .filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))
        .collect();

    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.dedup_by(|a, b| a.0 == b.0);
    results
}
```

Note: `candidates_for_trigger` keeps its original signature here — that changes in Task 5.

- [ ] **Step 5: Run new tests**

```bash
cargo test alias_simple alias_with_as alias_prefix_filter multi_alias
```
Expected: all 4 pass.

- [ ] **Step 6: Run full test suite**

```bash
cargo test
```
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): resolve aliases in qualified dot-completion"
```

---

### Task 5: Wire AliasMap into extract_table_refs and candidates_for_trigger

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write test**

Add to `#[cfg(test)]`:

```rust
#[test]
fn alias_in_where_trigger() {
    let schema = schema_with(
        &["users"],
        &[("users", &["id", "email"])],
    );
    let c = SqlCompleter::new(schema);
    let input = "SELECT u.id FROM users u WHERE ";
    let results = c.complete_input(input, input.len());
    assert!(
        results.iter().any(|(r, k)| r == "email" && matches!(k, CompletionKind::Column)),
        "expected email via WHERE trigger, got: {:?}",
        results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
    assert!(
        results.iter().any(|(r, k)| r == "id" && matches!(k, CompletionKind::Column)),
        "expected id via WHERE trigger"
    );
}
```

- [ ] **Step 2: Run test (may already pass — confirm either way)**

```bash
cargo test alias_in_where_trigger
```
Note the result — it validates trigger path works correctly with alias context.

- [ ] **Step 3: Update extract_table_refs signature and body**

Replace the existing `extract_table_refs` method. Return type changes from `Vec<&'a str>` to `Vec<String>`, and it now accepts `alias_map`:

```rust
fn extract_table_refs(&self, upper_query: &str, alias_map: &AliasMap) -> Vec<String> {
    let tokens: Vec<&str> = upper_query.split_whitespace().collect();
    let trigger = ["FROM", "JOIN", "UPDATE"];
    let mut refs: Vec<String> = tokens
        .windows(2)
        .filter_map(|w| trigger.contains(&w[0]).then_some(w[1].to_lowercase()))
        .collect();
    for real_table in alias_map.map.values().filter_map(|v| v.as_deref()) {
        if !refs.contains(&real_table.to_string()) {
            refs.push(real_table.to_string());
        }
    }
    refs
}
```

- [ ] **Step 4: Update candidates_for_trigger to accept and thread alias_map**

Replace `candidates_for_trigger`. The `SELECT`/`WHERE` branch also drops the `to_lowercase()` call since `extract_table_refs` now returns lowercase strings directly:

```rust
fn candidates_for_trigger(&self, trigger: &str, upper_query: &str, alias_map: &AliasMap) -> Vec<(String, CompletionKind)> {
    match trigger {
        "FROM" | "JOIN" | "INTO" | "UPDATE" => self
            .schema
            .tables()
            .iter()
            .map(|t| (t.to_string(), CompletionKind::Table))
            .collect(),
        "SELECT" | "WHERE" | "ON" | "SET" | "BY" => {
            let table_refs = self.extract_table_refs(upper_query, alias_map);
            if table_refs.is_empty() {
                SQL_KEYWORDS
                    .iter()
                    .map(|k| (k.to_string(), CompletionKind::Keyword))
                    .collect()
            } else {
                table_refs
                    .iter()
                    .flat_map(|t| {
                        self.schema
                            .columns_for(t)
                            .iter()
                            .map(|c| (c.to_string(), CompletionKind::Column))
                    })
                    .collect()
            }
        }
        _ => SQL_KEYWORDS
            .iter()
            .map(|k| (k.to_string(), CompletionKind::Keyword))
            .collect(),
    }
}
```

- [ ] **Step 5: Pass alias_map to candidates_for_trigger in complete_input**

In `complete_input`, change the `candidates_for_trigger` call from:

```rust
let candidates = self.candidates_for_trigger(effective_trigger, &full_upper);
```

to:

```rust
let candidates = self.candidates_for_trigger(effective_trigger, &full_upper, &alias_map);
```

- [ ] **Step 6: Run full test suite**

```bash
cargo test
```
Expected: all tests pass including `alias_in_where_trigger`.

- [ ] **Step 7: Run clippy**

```bash
cargo clippy
```
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): thread alias map through trigger-based column completion"
```
