# SQL Syntax Highlighting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add real-time SQL syntax highlighting to the pgrs REPL input line, plus colorize `true`/`false`/`NULL` values in query output.

**Architecture:** The input highlighter is a char-by-char state machine (`highlight_sql`) wired into rustyline's existing `Highlighter` trait on `SqlCompleter`. The output colorizer adds `colorize_cell` and `visible_len` helpers to `executor.rs` so column alignment stays correct despite invisible ANSI codes.

**Tech Stack:** Rust, rustyline 14, raw ANSI escape sequences (no new crates)

---

## File Map

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `highlight_sql()`, implement `highlight()` and `highlight_char()` on `Highlighter` |
| `src/adapters/driving/repl/executor.rs` | Add `colorize_cell()`, `visible_len()`, update `col_widths` and cell formatting in `format_result()` |

---

## Task 1: `highlight_sql` tokenizer

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write failing tests for `highlight_sql`**

Add these tests at the bottom of the `#[cfg(test)]` block in `completer.rs`:

```rust
#[test]
fn highlight_keyword_bold_cyan() {
    let result = highlight_sql("SELECT", &[], &[]);
    assert!(result.contains("\x1b[1;36m"), "expected bold cyan escape");
    assert!(result.contains("SELECT"));
    assert!(result.contains("\x1b[0m"), "expected reset");
}

#[test]
fn highlight_string_literal_yellow() {
    let result = highlight_sql("'hello'", &[], &[]);
    assert!(result.contains("\x1b[33m"), "expected yellow escape");
    assert!(result.contains("'hello'"));
}

#[test]
fn highlight_number_magenta() {
    let result = highlight_sql("42", &[], &[]);
    assert!(result.contains("\x1b[35m"), "expected magenta escape");
    assert!(result.contains("42"));
}

#[test]
fn highlight_comment_dim() {
    let result = highlight_sql("-- comment", &[], &[]);
    assert!(result.contains("\x1b[2m"), "expected dim escape");
    assert!(result.contains("-- comment"));
}

#[test]
fn highlight_table_name_bold_yellow() {
    let tables = vec!["users".to_string()];
    let result = highlight_sql("users", &tables, &[]);
    assert!(result.contains("\x1b[1;33m"), "expected bold yellow for table");
}

#[test]
fn highlight_column_name_green() {
    let columns = vec!["email".to_string()];
    let result = highlight_sql("email", &[], &columns);
    assert!(result.contains("\x1b[32m"), "expected green for column");
}

#[test]
fn highlight_plain_word_no_escape() {
    let result = highlight_sql("foo", &[], &[]);
    assert!(!result.contains("\x1b["), "expected no escape for unknown word");
}

#[test]
fn highlight_mixed_query() {
    let tables = vec!["users".to_string()];
    let result = highlight_sql("SELECT * FROM users WHERE id = 1", &tables, &[]);
    assert!(result.contains("\x1b[1;36m"), "SELECT should be bold cyan");
    assert!(result.contains("\x1b[1;33m"), "users should be bold yellow");
    assert!(result.contains("\x1b[35m"), "1 should be magenta");
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test highlight_ -- --nocapture 2>&1 | head -40
```

Expected: compilation error or test failures — `highlight_sql` does not exist yet.

- [ ] **Step 3: Add `highlight_sql` function**

Add this function above the `impl SqlCompleter` block in `completer.rs`:

```rust
use std::borrow::Cow;

pub fn highlight_sql(line: &str, tables: &[String], columns: &[String]) -> String {
    let mut out = String::with_capacity(line.len() * 2);
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Line comment: --
        if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            let span: String = chars[start..i].iter().collect();
            out.push_str(&format!("\x1b[2m{}\x1b[0m", span));
        }
        // String literal: '...'
        else if chars[i] == '\'' {
            let start = i;
            i += 1;
            while i < len && chars[i] != '\'' {
                i += 1;
            }
            if i < len { i += 1; } // consume closing '
            let span: String = chars[start..i].iter().collect();
            out.push_str(&format!("\x1b[33m{}\x1b[0m", span));
        }
        // Number: digit
        else if chars[i].is_ascii_digit() {
            let start = i;
            let mut has_dot = false;
            while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot)) {
                if chars[i] == '.' { has_dot = true; }
                i += 1;
            }
            let span: String = chars[start..i].iter().collect();
            out.push_str(&format!("\x1b[35m{}\x1b[0m", span));
        }
        // Word: letter or underscore
        else if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            if SQL_KEYWORDS.contains(&upper.as_str()) {
                out.push_str(&format!("\x1b[1;36m{}\x1b[0m", word));
            } else if tables.iter().any(|t| t.eq_ignore_ascii_case(&word)) {
                out.push_str(&format!("\x1b[1;33m{}\x1b[0m", word));
            } else if columns.iter().any(|c| c.eq_ignore_ascii_case(&word)) {
                out.push_str(&format!("\x1b[32m{}\x1b[0m", word));
            } else {
                out.push_str(&word);
            }
        }
        // Everything else
        else {
            out.push(chars[i]);
            i += 1;
        }
    }

    out
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test highlight_ -- --nocapture 2>&1 | tail -20
```

Expected: all `highlight_*` tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat: add highlight_sql tokenizer for SQL syntax highlighting"
```

---

## Task 2: Wire `highlight_sql` into rustyline `Highlighter`

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Replace the empty `Highlighter` impl**

Find this in `completer.rs`:

```rust
impl Highlighter for SqlCompleter {}
```

Replace with:

```rust
impl Highlighter for SqlCompleter {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let tables = self.schema.tables();
        let columns: Vec<String> = tables
            .iter()
            .flat_map(|t| self.schema.columns_for(t).into_iter())
            .collect();
        Cow::Owned(highlight_sql(line, tables, &columns))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}
```

- [ ] **Step 2: Confirm `use std::borrow::Cow` is present at top of file**

`highlight_sql` already returns `String`, but `highlight()` returns `Cow<'l, str>`. The import is needed. Check the top of `completer.rs` for:

```rust
use std::borrow::Cow;
```

If it's missing, add it alongside the other `use` statements.

- [ ] **Step 3: Build to confirm no compile errors**

```bash
cargo build 2>&1 | grep -E "^error"
```

Expected: no output (clean build).

- [ ] **Step 4: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat: wire highlight_sql into rustyline Highlighter for live input coloring"
```

---

## Task 3: Output cell colorizer helpers

**Files:**
- Modify: `src/adapters/driving/repl/executor.rs`

- [ ] **Step 1: Write failing tests for `colorize_cell` and `visible_len`**

Add to the `#[cfg(test)]` block in `executor.rs`:

```rust
#[test]
fn colorize_true_bold_green() {
    let result = colorize_cell("true");
    assert!(result.contains("\x1b[1;32m"), "expected bold green for true");
    assert!(result.contains("true"));
    assert!(result.contains("\x1b[0m"));
}

#[test]
fn colorize_false_bold_red() {
    let result = colorize_cell("false");
    assert!(result.contains("\x1b[1;31m"), "expected bold red for false");
    assert!(result.contains("false"));
}

#[test]
fn colorize_null_dim() {
    let result = colorize_cell("null");
    assert!(result.contains("\x1b[2m"), "expected dim for null");
    assert!(result.contains("null"));
}

#[test]
fn colorize_null_case_insensitive() {
    let result = colorize_cell("NULL");
    assert!(result.contains("\x1b[2m"), "expected dim for NULL");
}

#[test]
fn colorize_plain_value_unchanged() {
    let result = colorize_cell("hello");
    assert_eq!(result, "hello");
}

#[test]
fn visible_len_strips_ansi() {
    let colored = "\x1b[1;32mtrue\x1b[0m";
    assert_eq!(visible_len(colored), 4);
}

#[test]
fn visible_len_plain_string() {
    assert_eq!(visible_len("hello"), 5);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test colorize_ visible_len -- --nocapture 2>&1 | head -20
```

Expected: compile error — functions don't exist yet.

- [ ] **Step 3: Add `colorize_cell` and `visible_len` functions**

Add these above `pub fn print_result` in `executor.rs`:

```rust
fn colorize_cell(val: &str) -> String {
    match val.to_lowercase().as_str() {
        "true"  => format!("\x1b[1;32m{}\x1b[0m", val),
        "false" => format!("\x1b[1;31m{}\x1b[0m", val),
        "null"  => format!("\x1b[2m{}\x1b[0m", val),
        _       => val.to_string(),
    }
}

fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' { in_escape = false; }
        } else {
            len += 1;
        }
    }
    len
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test colorize_ visible_len -- --nocapture 2>&1 | tail -20
```

Expected: all new tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/executor.rs
git commit -m "feat: add colorize_cell and visible_len helpers for output coloring"
```

---

## Task 4: Apply colorizer to `format_result`

**Files:**
- Modify: `src/adapters/driving/repl/executor.rs`

- [ ] **Step 1: Update `col_widths` to use `visible_len`**

In `format_result`, find:

```rust
let col_widths: Vec<usize> = result
    .columns
    .iter()
    .enumerate()
    .map(|(i, col)| {
        let max_val = result.rows.iter().map(|r| r[i].len()).max().unwrap_or(0);
        col.len().max(max_val)
    })
    .collect();
```

Replace with:

```rust
let col_widths: Vec<usize> = result
    .columns
    .iter()
    .enumerate()
    .map(|(i, col)| {
        let max_val = result.rows.iter().map(|r| visible_len(&r[i])).max().unwrap_or(0);
        col.len().max(max_val)
    })
    .collect();
```

- [ ] **Step 2: Apply `colorize_cell` when building cell strings**

Find:

```rust
let cells: Vec<String> = row
    .iter()
    .enumerate()
    .map(|(i, val)| format!("{:<width$}", val, width = col_widths[i]))
    .collect();
```

Replace with:

```rust
let cells: Vec<String> = row
    .iter()
    .enumerate()
    .map(|(i, val)| {
        let colored = colorize_cell(val);
        let padding = col_widths[i].saturating_sub(visible_len(val));
        format!("{}{}", colored, " ".repeat(padding))
    })
    .collect();
```

- [ ] **Step 3: Run all tests to confirm nothing regressed**

```bash
cargo test 2>&1 | tail -20
```

Expected: all tests pass including the pre-existing ones (`formats_single_row`, `formats_empty_result`, `column_width_fits_longest_value`).

- [ ] **Step 4: Commit**

```bash
git add src/adapters/driving/repl/executor.rs
git commit -m "feat: colorize true/false/null values in query output"
```

---

## Task 5: Manual smoke test

**Files:** none — verification only

- [ ] **Step 1: Build release binary**

```bash
cargo build --release 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 2: Run `cargo clippy` and fix any warnings**

```bash
cargo clippy 2>&1 | grep -E "^error|warning\[" | head -20
```

Fix any warnings that appear before proceeding.

- [ ] **Step 3: Verify end-to-end**

Connect to a real Postgres instance with `pgrs shell <name>`. Verify:

1. Type `SELECT` — letters appear in bold cyan
2. Type `SELECT * FROM ` — after `FROM`, type a real table name from your schema — it turns bold yellow
3. Type `'hello'` — string turns yellow
4. Type `42` — number turns magenta
5. Type `-- comment` — text turns dim
6. Run a query that returns boolean or null columns — `true` appears bold green, `false` bold red, `null` dim grey

- [ ] **Step 4: Final commit if clippy fixes were made**

```bash
git add -p
git commit -m "fix: address clippy warnings in syntax highlighting"
```

Only needed if Step 2 produced warnings that were fixed.
