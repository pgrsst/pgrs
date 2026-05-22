# Autocomplete Improvements Design

**Date:** 2026-05-22  
**Scope:** `src/adapters/driving/repl/completer.rs`

## Background

pgrs has a SQL REPL with context-aware autocomplete: table names after FROM/JOIN, columns from relevant tables after SELECT/WHERE/ON. The current implementation uses prefix-only matching and returns all columns from all query tables when completing a JOIN ON condition — both are pain points for complex queries.

## Goals

1. **Fuzzy matching** — replace prefix-only filter with subsequence matching so partial abbreviations (e.g. `usr` → `users`, `crat` → `created_at`) produce results.
2. **JOIN ON smart columns** — when completing after `ON`, identify the two tables involved in the current JOIN and surface their columns, with shared column names (likely FK candidates) ranked first.

## Non-goals

- CTE-aware completion
- Schema-qualified completion (`public.`)
- `SELECT *` expansion
- Data type completion

---

## Feature 1: Fuzzy Matching

### Algorithm

Subsequence matching: every character in the query must appear in the candidate **in order**, but not necessarily consecutively.

```
"usr"  vs "users"      → u✓ s✓ r✓  → match
"crat" vs "created_at" → c✓ r✓ a✓ t✓ → match
"eml"  vs "email"      → e✓ m✓ l✓  → match
"xyz"  vs "users"      → x✗         → no match
```

Empty query matches everything (show all candidates when nothing typed yet).

### Implementation

Add a standalone function:

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

Replace the filter in `complete_input`:

```rust
// before
.filter(|(c, _)| c.to_uppercase().starts_with(&prefix_upper))

// after
.filter(|(c, _)| fuzzy_match(c, &prefix_upper))
```

### Behavior

- No regressions: prefix match is a strict subset of subsequence match — all existing matches continue to work.
- Prefix matches will naturally appear early in alphabetical sort; fuzzy-only matches appear after.
- The empty-prefix case (trigger word just typed, cursor after space) is unchanged: all candidates returned.

---

## Feature 2: JOIN ON Smart Column Suggestions

### Current behavior

`ON` is in `COLUMN_TRIGGERS`. When triggered, `candidates_for_trigger` calls `extract_table_refs` which returns all tables mentioned in FROM/JOIN across the whole query. All their columns are returned flat.

For `FROM users u JOIN orders o ON `, this returns columns from both `users` AND `orders` with no ordering — fine for simple cases, confusing for queries with 3+ tables.

### New behavior

When the trigger is `ON`, parse the query to find the two tables involved in the **current** JOIN clause:

- **Right table**: the table named immediately after the most recent `JOIN` keyword before `ON`
- **Left tables**: all other tables referenced in the query

Return:
1. Columns that exist in **both** right table and at least one left table — likely FK/join keys, ranked first
2. All other columns from right table
3. All other columns from left tables

### Parsing approach

Operate on the uppercase token list already available in `complete_input`. Walk backwards from the `ON` trigger to find the nearest `JOIN`, then take the next token as the right-table name. Resolve aliases via the existing `AliasMap`.

```
tokens: [..., "FROM", "USERS", "U", "JOIN", "ORDERS", "O", "ON"]
                                          ^^^^^^ right table = ORDERS → resolve alias map if needed
```

### Edge cases

| Case | Behavior |
|---|---|
| `ON` without preceding `JOIN` (e.g. constraint context) | Fall back to all columns from all table refs |
| Right table not in schema | Fall back to all columns from all table refs |
| Multi-JOIN: `FROM a JOIN b ON b.x = a.y JOIN c ON ` | Right table = `c`, left tables = `a`, `b` |
| Alias resolution | Right table name looked up in alias map before schema lookup |

---

## File Changes

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `fuzzy_match()`, update filter in `complete_input`, add `extract_join_tables()`, update `candidates_for_trigger` ON arm |

No public API changes. No changes outside `completer.rs`.

---

## Tests to Add

### Fuzzy matching
- `usr` matches `users` (subsequence, not prefix)
- `crat` matches `created_at`
- `xyz` does not match `users`
- Empty prefix returns all candidates (existing behavior preserved)
- Existing prefix tests still pass

### JOIN ON context
- `FROM users JOIN orders ON ` → suggest columns from both users and orders
- Shared column names appear in results
- `FROM users u JOIN orders o ON ` → alias resolution works
- Multi-JOIN: third table's columns appear after second JOIN's ON
- ON without JOIN → falls back to all table columns
