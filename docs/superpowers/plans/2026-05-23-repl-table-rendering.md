# REPL Table Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render REPL query results as a clean minimal table with middle-truncation and a `\x` expanded mode.

**Architecture:** All rendering logic stays in `src/adapters/driving/repl/executor.rs`; the `\x` toggle state lives in the REPL loop in `src/adapters/driving/repl/mod.rs`. `QueryResult` is unchanged.

**Tech Stack:** Rust, `unicode-width` (already a dependency), `cargo test`.

---

## File Structure

- Modify: `src/adapters/driving/repl/executor.rs` — add `truncate_middle`, rewrite minimal renderer, add expanded renderer, change `format_result`/`print_result` signatures.
- Modify: `src/adapters/driving/repl/mod.rs` — add `expanded` flag, handle `\x`, pass flag to `print_result`, add `\x` to help text.

Existing helpers to keep: `normalize_val`, `colorize_cell`, `visible_len`.

---

### Task 1: Middle-truncate helper

**Files:**
- Modify: `src/adapters/driving/repl/executor.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `executor.rs`:

```rust
#[test]
fn truncate_middle_keeps_short_values() {
    assert_eq!(truncate_middle("12345678910"), "12345678910");
}

#[test]
fn truncate_middle_exactly_40_unchanged() {
    let s = "a".repeat(40);
    assert_eq!(truncate_middle(&s), s);
}

#[test]
fn truncate_middle_long_value_has_ellipsis_total_40() {
    let s = "a".repeat(60);
    let out = truncate_middle(&s);
    assert_eq!(out.chars().count(), 40);
    assert!(out.contains("..."));
    assert!(out.starts_with(&"a".repeat(19)));
    assert!(out.ends_with(&"a".repeat(18)));
}

#[test]
fn truncate_middle_char_based_multibyte() {
    // 50 CJK chars -> truncated to 40 chars, not bytes
    let s = "あ".repeat(50);
    let out = truncate_middle(&s);
    assert_eq!(out.chars().count(), 40);
    assert!(out.contains("..."));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib truncate_middle`
Expected: FAIL — `cannot find function truncate_middle`.

- [ ] **Step 3: Implement `truncate_middle`**

Add near the top of `executor.rs` (after `normalize_val`):

```rust
const MAX_CELL_CHARS: usize = 40;

fn truncate_middle(val: &str) -> String {
    let chars: Vec<char> = val.chars().collect();
    if chars.len() <= MAX_CELL_CHARS {
        return val.to_string();
    }
    let keep = MAX_CELL_CHARS - 3; // room for "..."
    let prefix = keep.div_ceil(2); // 19
    let suffix = keep - prefix;     // 18
    let head: String = chars[..prefix].iter().collect();
    let tail: String = chars[chars.len() - suffix..].iter().collect();
    format!("{}...{}", head, tail)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib truncate_middle`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/executor.rs
git commit -m "feat(repl): add middle-truncate helper for long cells"
```

---

### Task 2: Minimal-style renderer with truncation + `expanded` param

**Files:**
- Modify: `src/adapters/driving/repl/executor.rs`
- Modify: `src/adapters/driving/repl/mod.rs` (caller, to keep build green)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
#[test]
fn minimal_uses_box_underline_and_two_space_gap() {
    let result = QueryResult {
        columns: vec!["id".to_string(), "email".to_string()],
        rows: vec![vec!["1".to_string(), "alice@example.com".to_string()]],
        rows_affected: None,
    };
    let out = format_result(&result, false);
    // underline uses U+2500, not ASCII '-' or '+'
    assert!(out.contains('─'), "expected box-drawing underline, got:\n{out}");
    assert!(!out.contains('|'), "minimal style has no pipes, got:\n{out}");
    assert!(!out.contains('+'), "minimal style has no plus, got:\n{out}");
    // two-space gap between columns in the header
    assert!(out.contains("id  email"), "expected 2-space gap, got:\n{out}");
}

#[test]
fn minimal_truncates_long_cell() {
    let long = "x".repeat(60);
    let result = QueryResult {
        columns: vec!["v".to_string()],
        rows: vec![vec![long.clone()]],
        rows_affected: None,
    };
    let out = format_result(&result, false);
    assert!(out.contains("..."), "expected truncated cell, got:\n{out}");
    assert!(!out.contains(&long), "full value should not appear, got:\n{out}");
}
```

Update the existing minimal-mode tests to pass `false` as the new argument:
`formats_single_row`, `formats_empty_result`, `column_width_fits_longest_value`,
`zero_row_select_shows_column_headers`, `dml_shows_rows_affected_label`,
`dml_single_row_affected_singular`, `select_row_count_does_not_say_affected` —
each call site `format_result(&result)` becomes `format_result(&result, false)`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib`
Expected: FAIL — `format_result` takes 1 argument / new assertions fail.

- [ ] **Step 3: Rewrite the minimal renderer**

Replace `print_result` and `format_result` in `executor.rs` with:

```rust
pub fn print_result(result: &QueryResult, expanded: bool) {
    print!("{}", format_result(result, expanded));
}

pub fn format_result(result: &QueryResult, expanded: bool) -> String {
    if result.columns.is_empty() {
        let count = result.rows_affected.unwrap_or(result.rows.len() as u64);
        return if result.rows_affected.is_some() {
            format!("({} {})\n", count, if count == 1 { "row affected" } else { "rows affected" })
        } else {
            format!("({} {})\n", count, if count == 1 { "row" } else { "rows" })
        };
    }

    if expanded {
        return format_expanded(result);
    }

    format_minimal(result)
}

fn format_minimal(result: &QueryResult) -> String {
    // pre-truncate each cell value (after t/f normalization)
    let cells: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|r| r.iter().map(|v| truncate_middle(normalize_val(v))).collect())
        .collect();

    let col_widths: Vec<usize> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let max_val = cells.iter().map(|r| visible_len(&r[i])).max().unwrap_or(0);
            col.len().max(max_val)
        })
        .collect();

    let mut out = String::new();

    // header
    let header: Vec<String> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| format!("{:<width$}", col, width = col_widths[i]))
        .collect();
    out.push_str(&header.join("  "));
    out.push('\n');

    // underline
    let underline: Vec<String> = col_widths.iter().map(|w| "─".repeat(*w)).collect();
    out.push_str(&underline.join("  "));
    out.push('\n');

    // rows
    for row in &cells {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| {
                let colored = colorize_cell(val);
                let padding = col_widths[i].saturating_sub(visible_len(val));
                format!("{}{}", colored, " ".repeat(padding))
            })
            .collect();
        out.push_str(&line.join("  "));
        out.push('\n');
    }

    let count = result.rows.len();
    out.push_str(&format!(
        "({} {})\n",
        count,
        if count == 1 { "row" } else { "rows" }
    ));

    out
}
```

Note: `colorize_cell` currently calls `normalize_val` internally. Since cells are
already normalized + truncated, colorize on an already-normalized value is
idempotent (`normalize_val("true") == "true"`), so behavior is unchanged.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib`
Expected: still FAIL to **compile** because `mod.rs` calls `print_result` with one arg. Fix in Step 5 before re-running.

- [ ] **Step 5: Update the caller in `mod.rs`**

In `src/adapters/driving/repl/mod.rs`, find the execute match arm:

```rust
                match conn.execute(trimmed) {
                    Ok(result) => print_result(&result),
                    Err(e) => eprintln!("error: {}", e),
                }
```

Change to:

```rust
                match conn.execute(trimmed) {
                    Ok(result) => print_result(&result, false),
                    Err(e) => eprintln!("error: {}", e),
                }
```

- [ ] **Step 6: Run tests and clippy to verify they pass**

Run: `cargo test --lib && cargo clippy`
Expected: PASS, no clippy errors.

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/executor.rs src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): render results in minimal style with truncation"
```

---

### Task 3: Expanded (record) renderer

**Files:**
- Modify: `src/adapters/driving/repl/executor.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
#[test]
fn expanded_uses_record_header_and_labels() {
    let result = QueryResult {
        columns: vec!["id".to_string(), "email".to_string()],
        rows: vec![
            vec!["1".to_string(), "alice@example.com".to_string()],
            vec!["2".to_string(), "bob@example.com".to_string()],
        ],
        rows_affected: None,
    };
    let out = format_result(&result, true);
    assert!(out.contains("-[ RECORD 1 ]"), "missing record 1 header:\n{out}");
    assert!(out.contains("-[ RECORD 2 ]"), "missing record 2 header:\n{out}");
    assert!(out.contains("email | alice@example.com"), "label padding wrong:\n{out}");
    assert!(out.contains("id    | 1"), "label padding wrong:\n{out}");
}

#[test]
fn expanded_does_not_truncate() {
    let long = "y".repeat(60);
    let result = QueryResult {
        columns: vec!["v".to_string()],
        rows: vec![vec![long.clone()]],
        rows_affected: None,
    };
    let out = format_result(&result, true);
    assert!(out.contains(&long), "expanded mode must show full value:\n{out}");
    assert!(!out.contains("..."), "expanded mode must not truncate:\n{out}");
}

#[test]
fn expanded_empty_columns_shows_footer_only() {
    let result = QueryResult {
        columns: vec![],
        rows: vec![],
        rows_affected: Some(3),
    };
    let out = format_result(&result, true);
    assert!(out.contains("(3 rows affected)"), "expected footer:\n{out}");
    assert!(!out.contains("RECORD"), "no records expected:\n{out}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib expanded`
Expected: FAIL — `cannot find function format_expanded`.

- [ ] **Step 3: Implement `format_expanded`**

Add to `executor.rs`:

```rust
fn format_expanded(result: &QueryResult) -> String {
    let label_width = result.columns.iter().map(|c| c.len()).max().unwrap_or(0);
    let mut out = String::new();

    for (idx, row) in result.rows.iter().enumerate() {
        let title = format!("-[ RECORD {} ]", idx + 1);
        let pad = (label_width + 3).saturating_sub(visible_len(&title));
        out.push_str(&title);
        out.push_str(&"-".repeat(pad));
        out.push('\n');

        for (i, col) in result.columns.iter().enumerate() {
            let val = normalize_val(&row[i]);
            let colored = colorize_cell(val);
            out.push_str(&format!("{:<width$} | {}\n", col, colored, width = label_width));
        }
    }

    let count = result.rows.len();
    out.push_str(&format!(
        "({} {})\n",
        count,
        if count == 1 { "row" } else { "rows" }
    ));

    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib expanded`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/executor.rs
git commit -m "feat(repl): add expanded record renderer"
```

---

### Task 4: Wire `\x` toggle into the REPL loop

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `mod.rs`:

```rust
#[test]
fn help_text_mentions_x_command() {
    let text = repl_help_text();
    assert!(text.contains("\\x"), "help should mention \\x, got: {text}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib help_text_mentions_x_command`
Expected: FAIL — help text has no `\x`.

- [ ] **Step 3: Add `\x` to help text**

In `repl_help_text()`, change the body to include the `\x` line:

```rust
fn repl_help_text() -> &'static str {
    "  Type any SQL and end it with ';' to run it (Enter alone continues a\n  multi-line statement until the ';').\n\n  \\dt        list tables\n  \\x         toggle expanded display\n  \\help, \\?  show this help\n  \\q, exit   quit (or Ctrl+D)"
}
```

- [ ] **Step 4: Add the toggle flag and command handling**

In `run`, before the `loop {`, add:

```rust
    let mut expanded = false;
```

Inside the loop, after the `\dt` block and before the `trimmed.is_empty()` check, add:

```rust
                if trimmed == "\\x" {
                    expanded = !expanded;
                    println!("Expanded display is {}.", if expanded { "on" } else { "off" });
                    continue;
                }
```

Change the execute arm to pass the flag:

```rust
                match conn.execute(trimmed) {
                    Ok(result) => print_result(&result, expanded),
                    Err(e) => eprintln!("error: {}", e),
                }
```

- [ ] **Step 5: Run tests and clippy to verify they pass**

Run: `cargo test --lib && cargo clippy`
Expected: PASS, no clippy errors.

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): toggle expanded display with \\x command"
```

---

### Task 5: Manual smoke test

**Files:** none (verification only)

- [ ] **Step 1: Build**

Run: `cargo build`
Expected: success.

- [ ] **Step 2: Connect and verify (requires a reachable connection)**

Run the REPL against a known connection, then:
- Run a `SELECT` with several columns → minimal table with `─` underline, 2-space gaps.
- Run a query with a >40-char cell → middle-truncated with `...`.
- Type `\x` → `Expanded display is on.`; rerun the wide query → record format, full values.
- Type `\x` again → `Expanded display is off.`
- Type `\help` → confirm `\x` is listed.

Report what was observed. If no connection is available, state that the manual smoke test could not be run.

---

## Notes

- All unit tests run with `cargo test --lib`.
- No `QueryResult` or trait changes; purely presentation.
