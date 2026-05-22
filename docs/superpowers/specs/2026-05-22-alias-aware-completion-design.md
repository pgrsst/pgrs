# Alias-Aware SQL Autocomplete

**Date:** 2026-05-22
**Scope:** `src/adapters/driving/repl/completer.rs` only

## Problem

The current `SqlCompleter` resolves `table.column` completions by treating the token before the dot as a literal table name. This means `SELECT u.` after `FROM users u` produces no suggestions because `"u"` is not a known table — it's an alias. The completer has no concept of aliases.

## Goal

When the user types a qualified name (`alias.col_prefix`), resolve the alias to its real table name and suggest columns from that table. Support:

- `FROM users u` and `FROM users AS u`
- Multiple aliases in one query: `FROM users u JOIN orders o`
- Subquery aliases: `FROM (SELECT ...) AS s` — fallback to all columns
- Alias resolution in trigger-based completion (WHERE, SELECT)

## Design

### AliasMap

A new private struct in `completer.rs`:

```rust
struct AliasMap {
    map: HashMap<String, Option<String>>,
    //          alias   → Some(real_table) or None (subquery)
}

impl AliasMap {
    fn resolve<'a>(&self, name: &'a str) -> Option<&str> {
        self.map.get(name).and_then(|v| v.as_deref())
    }
}
```

`None` means the alias refers to a subquery — we cannot know its columns, so completions fall back to all columns or return empty.

### build_alias_map

A free function that runs one pass over the token stream produced by the existing `tokenize()`:

```
State machine transitions:

Idle
  FROM | JOIN | UPDATE | INTO  → ExpectTable

ExpectTable
  `(`       → InSubquery { depth: 1 }
  WORD (non-keyword)  → ExpectAlias { candidate: word }
  anything else       → Idle

ExpectAlias { candidate }
  `AS`                → ExpectAliasName { candidate }
  WORD (non-keyword)  → insert map[word] = Some(candidate), → Idle
  `,`                 → Idle   (table used without alias)
  keyword / other     → Idle

ExpectAliasName { candidate }
  WORD  → insert map[word] = Some(candidate), → Idle
  other → Idle

InSubquery { depth }
  `(`  → depth += 1
  `)`  → depth -= 1; if depth == 0 → ExpectSubqueryAlias
  else → stay

ExpectSubqueryAlias
  `AS`                → ExpectSubqueryAliasName
  WORD (non-keyword)  → insert map[word] = None, → Idle
  other               → Idle

ExpectSubqueryAliasName
  WORD  → insert map[word] = None, → Idle
  other → Idle
```

Keywords are detected via the existing `SQL_KEYWORDS` constant so the scanner does not mistake a keyword for an alias.

### Integration into SqlCompleter

`build_alias_map` is called once at the top of `complete_input`, producing an `AliasMap` that lives for the duration of one completion request.

**complete_qualified** receives the alias_map and resolves before lookup:

```rust
fn complete_qualified(&self, table_name: &str, col_prefix: &str, alias_map: &AliasMap) {
    let resolved = alias_map.resolve(table_name).unwrap_or(table_name);
    let cols = self.schema.columns_for(resolved);
    // fallback logic unchanged
}
```

**extract_table_refs** is updated to also include real table names resolved from the alias_map, so that `SELECT`/`WHERE` trigger-based completion can suggest columns for aliased tables even when the query uses only alias names:

```rust
fn extract_table_refs(&self, upper_query: &str, alias_map: &AliasMap) -> Vec<String> {
    // existing token-window scan (unchanged)
    // + alias_map values (Some entries) appended
}
```

No public API changes. `Completer::complete` signature is unchanged.

## File Changes

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `AliasMap`, `build_alias_map`, update `complete_qualified` and `extract_table_refs` |

No other files touched.

## Tests

New tests added to the existing `#[cfg(test)]` block:

| Test name | Scenario | Expected |
|---|---|---|
| `alias_simple` | `SELECT u. FROM users u` | columns of `users` |
| `alias_with_as` | `SELECT u. FROM users AS u` | columns of `users` |
| `multi_alias` | `FROM users u JOIN orders o`, type `o.` | columns of `orders` |
| `alias_prefix_filter` | `SELECT u.em FROM users u` | only `email`, not `id` |
| `subquery_alias_fallback` | `SELECT s. FROM (SELECT x FROM foo) AS s` | all columns (fallback) or empty |
| `no_alias_unchanged` | `SELECT users. FROM users` | columns of `users` (existing behaviour) |
| `alias_in_where_trigger` | `FROM users u WHERE ` (trigger) | columns of `users` via alias resolution |

All existing tests must continue to pass.
