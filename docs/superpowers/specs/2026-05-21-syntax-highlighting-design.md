# SQL Syntax Highlighting Design

**Date:** 2026-05-21  
**Status:** Approved

## Overview

Add syntax highlighting to the pgrs interactive SQL REPL:

1. **Input line highlighting** — colorize SQL tokens as the user types
2. **Output result coloring** — colorize special cell values (`true`, `false`, `NULL`) in query results

No new dependencies. All changes stay within existing files.

---

## Input Line Highlighting

### Where

`src/adapters/driving/repl/completer.rs` — the `Highlighter` impl for `SqlCompleter` is currently empty. Fill in `highlight()` and `highlight_char()`.

### Token Types & Colors

| Token | ANSI Style | Examples |
|---|---|---|
| SQL keyword | Bold cyan | `SELECT`, `FROM`, `WHERE`, `JOIN` |
| Table name | Bold yellow | `users`, `orders` (matched against schema) |
| Column name | Green | `id`, `email` (matched against schema) |
| String literal | Yellow | `'hello'`, `'2024-01-01'` |
| Number | Magenta | `42`, `3.14` |
| Comment | Dim | `-- this is a comment` |
| Everything else | Default | operators, punctuation, whitespace |

### Tokenizer Logic

Add a free function `highlight_sql(line: &str, tables: &[String], columns: &[String]) -> String` inside `completer.rs`. It scans `line` char-by-char with a simple state machine:

1. **String literal** — on `'`, scan forward to closing `'`, wrap entire span in yellow
2. **Line comment** — on `--`, scan to end of line, wrap in dim
3. **Number** — on ASCII digit, scan digits and optional `.`, wrap in magenta
4. **Word** — on `[a-zA-Z_]`, scan `[a-zA-Z0-9_]`; then classify:
   - Uppercase match in `SQL_KEYWORDS` → bold cyan
   - Match in `tables` (case-insensitive) → bold yellow
   - Match in `columns` (case-insensitive) → green
   - Otherwise → default (no color)
5. **Other characters** — append as-is

Classification is case-insensitive; keywords are matched by uppercasing the scanned word.

### rustyline Integration

Override two methods on the existing `impl Highlighter for SqlCompleter`:

```rust
fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
    let tables = self.schema.tables();
    let columns: Vec<String> = tables.iter()
        .flat_map(|t| self.schema.columns_for(t).into_iter())
        .collect();
    Cow::Owned(highlight_sql(line, tables, &columns))
}

fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
    true  // must return true for rustyline to call highlight()
}
```

---

## Output Result Coloring

### Where

`src/adapters/driving/repl/executor.rs` — `format_result()` formats query output. Currently uses raw `.len()` for column width calculation and plain string formatting per cell.

### Cell Color Rules

| Cell value (case-insensitive) | Color |
|---|---|
| `true` | Bold green |
| `false` | Bold red |
| `null` | Dim (grey) |
| Anything else | Default (no color) |

### Implementation

Add two helpers in `executor.rs`:

```rust
fn colorize_cell(val: &str) -> String
fn visible_len(val: &str) -> usize
```

`colorize_cell` wraps the value in the appropriate ANSI escape sequence.  
`visible_len` strips ANSI codes before measuring length (ANSI sequences are invisible but add to `.len()`).

Update `col_widths` to use `visible_len` instead of `.len()` so column alignment is based on visible characters only. Apply `colorize_cell` when building each cell string.

---

## ANSI Color Reference

Use raw escape sequences (no new crate):

| Style | Escape |
|---|---|
| Reset | `\x1b[0m` |
| Bold | `\x1b[1m` |
| Dim | `\x1b[2m` |
| Cyan | `\x1b[36m` |
| Bold cyan | `\x1b[1;36m` |
| Yellow | `\x1b[33m` |
| Bold yellow | `\x1b[1;33m` |
| Green | `\x1b[32m` |
| Bold green | `\x1b[1;32m` |
| Magenta | `\x1b[35m` |
| Bold red | `\x1b[1;31m` |

---

## Files Changed

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `highlight_sql()`, fill in `Highlighter` impl |
| `src/adapters/driving/repl/executor.rs` | Add `colorize_cell()`, `visible_len()`, update `format_result()` |

No new files. No new dependencies.

---

## Out of Scope

- Prompt coloring (`pgrs> `)
- Hint coloring (autocomplete hints)
- Multi-line query highlighting (each line highlighted independently)
- Terminal capability detection (assume ANSI support)
