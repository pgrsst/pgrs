# Autocomplete Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace prefix-only completion with fuzzy (subsequence) matching, and make JOIN ON completion context-aware by surfacing shared FK-candidate columns first.

**Architecture:** Both changes are isolated to `completer.rs`. Task 1 adds a `fuzzy_match()` function and replaces the filter in `complete_input`. Task 2 adds a `JoinContext` struct and `extract_join_context()` function, then splits the `ON` arm out of the grouped `candidates_for_trigger` match so it can apply smarter logic.

**Tech Stack:** Rust, `reedline` (completion trait), no new dependencies.

---

## File Map

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `fuzzy_match()`, update filter in `complete_input`; add `JoinContext`, `extract_join_context()`, split `ON` arm in `candidates_for_trigger` |

---

### Task 1: Fuzzy Matching

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write the failing tests**

In `src/adapters/driving/repl/completer.rs`, inside the `#[cfg(test)] mod tests` block, append:

```rust
#[test]
fn fuzzy_match_empty_query_matches_everything() {
    assert!(fuzzy_match("users", ""));
    assert!(fuzzy_match("orders", ""));
}

#[test]
fn fuzzy_match_prefix_still_works() {
    assert!(fuzzy_match("users", "use"));
}

#[test]
fn fuzzy_match_subsequence_usr_users() {
    assert!(fuzzy_match("users", "usr"));
}

#[test]
fn fuzzy_match_subsequence_crat_created_at() {
    assert!(fuzzy_match("created_at", "crat"));
}

#[test]
fn fuzzy_match_no_match() {
    assert!(!fuzzy_match("users", "xyz"));
}

#[test]
fn fuzzy_match_case_insensitive() {
    assert!(fuzzy_match("Users", "usr"));
    assert!(fuzzy_match("users", "USR"));
}

#[test]
fn complete_input_fuzzy_matches_table_by_subsequence() {
    let schema = schema_with(&["users", "orders"], &[]);
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("SELECT * FROM usr", 17);
    assert!(
        results.iter().any(|(r, _)| r == "users"),
        "expected 'users' via fuzzy 'usr', got: {:?}",
        results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
}

#[test]
fn complete_input_fuzzy_matches_column_by_subsequence() {
    let schema = schema_with(
        &["users"],
        &[("users", &["created_at", "email"])],
    );
    let c = SqlCompleter::new(schema);
    let results = c.complete_input("SELECT crat FROM users", 11);
    assert!(
        results.iter().any(|(r, _)| r == "created_at"),
        "expected 'created_at' via fuzzy 'crat', got: {:?}",
        results.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test fuzzy_match -- --nocapture
```

Expected: compile error — `fuzzy_match` is not defined yet.

- [ ] **Step 3: Add `fuzzy_match` function**

In `src/adapters/driving/repl/completer.rs`, add this function **before** `fn word_start` (around line 241):

```rust
fn fuzzy_match(candidate: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut chars = candidate.chars();
    query
        .chars()
        .all(|q| chars.any(|c| c.eq_ignore_ascii_case(&q)))
}
```

- [ ] **Step 4: Replace the prefix filter in `complete_input`**

Find this line in `complete_input` (around line 303):

```rust
        let mut results: Vec<(String, CompletionKind)> = candidates
            .into_iter()
            .filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))
            .collect();
```

Replace with:

```rust
        let mut results: Vec<(String, CompletionKind)> = candidates
            .into_iter()
            .filter(|(c, _)| fuzzy_match(c, &prefix_upper))
            .collect();
```

- [ ] **Step 5: Run all tests to verify they pass**

```bash
cargo test -- --nocapture
```

Expected: all tests pass, including new fuzzy tests and all existing tests (no regressions).

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): replace prefix filter with fuzzy subsequence matching"
```

---

### Task 2: JOIN ON Smart Column Suggestions

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write the failing tests**

In `src/adapters/driving/repl/completer.rs`, inside the `#[cfg(test)] mod tests` block, append:

```rust
#[test]
fn extract_join_context_finds_right_and_left_tables() {
    let alias_map = build_alias_map("SELECT * FROM users JOIN orders ON");
    let ctx = extract_join_context("SELECT * FROM USERS JOIN ORDERS ON", &alias_map)
        .expect("should find join context");
    assert_eq!(ctx.right_table, "orders");
    assert!(ctx.left_tables.contains(&"users".to_string()), "left_tables: {:?}", ctx.left_tables);
}

#[test]
fn extract_join_context_resolves_aliases() {
    let alias_map = build_alias_map("SELECT * FROM users u JOIN orders o ON");
    let ctx = extract_join_context("SELECT * FROM USERS U JOIN ORDERS O ON", &alias_map)
        .expect("should find join context with aliases");
    assert_eq!(ctx.right_table, "orders");
    assert!(ctx.left_tables.contains(&"users".to_string()), "left_tables: {:?}", ctx.left_tables);
}

#[test]
fn extract_join_context_no_join_returns_none() {
    let alias_map = build_alias_map("SELECT * FROM users");
    let ctx = extract_join_context("SELECT * FROM USERS ON", &alias_map);
    assert!(ctx.is_none(), "expected None when no JOIN present");
}

#[test]
fn extract_join_context_multi_join_uses_last() {
    let alias_map = build_alias_map("SELECT * FROM a JOIN b ON b.x = a.y JOIN c ON");
    let ctx = extract_join_context("SELECT * FROM A JOIN B ON B.X = A.Y JOIN C ON", &alias_map)
        .expect("should find context for last JOIN");
    assert_eq!(ctx.right_table, "c");
    assert!(ctx.left_tables.contains(&"a".to_string()), "left_tables: {:?}", ctx.left_tables);
    assert!(ctx.left_tables.contains(&"b".to_string()), "left_tables: {:?}", ctx.left_tables);
}

#[test]
fn join_on_shared_column_appears_first() {
    let schema = schema_with(
        &["users", "orders"],
        &[
            ("users", &["id", "email"]),
            ("orders", &["id", "user_id"]),
        ],
    );
    let c = SqlCompleter::new(schema);
    let input = "SELECT * FROM users JOIN orders ON ";
    let results = c.complete_input(input, input.len());
    let id_pos = results.iter().position(|(r, _)| r == "id");
    let email_pos = results.iter().position(|(r, _)| r == "email");
    assert!(id_pos.is_some(), "expected 'id' in results");
    assert!(email_pos.is_some(), "expected 'email' in results");
    assert!(
        id_pos.unwrap() < email_pos.unwrap(),
        "shared column 'id' should appear before non-shared 'email'"
    );
}

#[test]
fn join_on_with_aliases_includes_both_tables_columns() {
    let schema = schema_with(
        &["users", "orders"],
        &[
            ("users", &["id", "email"]),
            ("orders", &["id", "user_id"]),
        ],
    );
    let c = SqlCompleter::new(schema);
    let input = "SELECT * FROM users u JOIN orders o ON ";
    let results = c.complete_input(input, input.len());
    assert!(results.iter().any(|(r, _)| r == "user_id"), "expected user_id from orders");
    assert!(results.iter().any(|(r, _)| r == "email"), "expected email from users");
}

#[test]
fn on_without_prior_join_falls_back_to_all_table_cols() {
    let schema = schema_with(
        &["users"],
        &[("users", &["id", "email"])],
    );
    let c = SqlCompleter::new(schema);
    // ON without a preceding JOIN — unusual but must not panic, fall back to table columns
    let input = "SELECT id FROM users ON ";
    let results = c.complete_input(input, input.len());
    assert!(results.iter().any(|(r, _)| r == "id" || r == "email"),
        "fallback should return columns from known tables");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test extract_join_context join_on on_without -- --nocapture
```

Expected: compile errors — `extract_join_context` and `JoinContext` not defined yet.

- [ ] **Step 3: Add `JoinContext` struct and `extract_join_context` function**

In `src/adapters/driving/repl/completer.rs`, add **after** the `build_alias_map` function (after line 118, before `const SQL_KEYWORDS`):

```rust
struct JoinContext {
    right_table: String,
    left_tables: Vec<String>,
}

fn extract_join_context(upper_query: &str, alias_map: &AliasMap) -> Option<JoinContext> {
    let tokens: Vec<&str> = upper_query.split_whitespace().collect();

    let last_join_pos = tokens.iter().rposition(|&t| t == "JOIN")?;

    let right_raw = tokens.get(last_join_pos + 1)?.to_lowercase();
    let right_table = alias_map
        .resolve(&right_raw)
        .map(|s| s.to_string())
        .unwrap_or_else(|| right_raw.clone());

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
```

- [ ] **Step 4: Split the `ON` arm in `candidates_for_trigger`**

In `candidates_for_trigger`, find the current grouped arm (around line 341):

```rust
            "SELECT" | "WHERE" | "ON" | "SET" | "BY" => {
```

Change it to remove `"ON"` from the group, and add a separate `"ON"` arm. The full updated match should look like:

```rust
    fn candidates_for_trigger(&self, trigger: &str, upper_query: &str, alias_map: &AliasMap) -> Vec<(String, CompletionKind)> {
        match trigger {
            "FROM" | "JOIN" | "INTO" | "UPDATE" => self
                .schema
                .tables()
                .iter()
                .map(|t| (t.to_string(), CompletionKind::Table))
                .collect(),
            "ON" => {
                if let Some(ctx) = extract_join_context(upper_query, alias_map) {
                    let right_cols: Vec<String> = self.schema.columns_for(&ctx.right_table).to_vec();
                    let left_cols: Vec<String> = ctx
                        .left_tables
                        .iter()
                        .flat_map(|t| self.schema.columns_for(t).iter().cloned())
                        .collect();

                    // Shared columns (likely FK keys) first
                    let mut result: Vec<(String, CompletionKind)> = right_cols
                        .iter()
                        .filter(|c| left_cols.iter().any(|lc| lc.eq_ignore_ascii_case(c)))
                        .map(|c| (c.clone(), CompletionKind::Column))
                        .collect();

                    // Remaining right table columns
                    for c in right_cols.iter().filter(|c| !left_cols.iter().any(|lc| lc.eq_ignore_ascii_case(c))) {
                        result.push((c.clone(), CompletionKind::Column));
                    }

                    // Left table columns
                    for c in &left_cols {
                        result.push((c.clone(), CompletionKind::Column));
                    }

                    result
                } else {
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
            }
            "SELECT" | "WHERE" | "SET" | "BY" => {
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

- [ ] **Step 5: Run all tests to verify they pass**

```bash
cargo test -- --nocapture
```

Expected: all tests pass including all new JOIN ON tests and all previously existing tests.

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): context-aware JOIN ON completion with shared column priority"
```
