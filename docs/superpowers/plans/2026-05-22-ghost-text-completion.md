# Ghost Text Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add inline ghost text (common-prefix hint) to the SQL REPL so Tab accepts the hint, then opens the full completion menu on a second press.

**Architecture:** Two new items in `completer.rs` — a pure `common_prefix()` function and a `SqlHinter` struct implementing `reedline::Hinter`. Wiring and the updated Tab binding go in `repl/mod.rs`. The correct reedline event for accepting a hint is `HistoryHintComplete` (not `CompleteHint`, which does not exist in reedline 0.47). Space-to-accept is deferred: reedline 0.47's `Multiple` event exits immediately on `Exits(Signal)`, so binding Space to `[Enter, InsertChar(' ')]` would submit complete SQL silently; there is no menu-guard event available.

**Tech Stack:** Rust, `reedline 0.47` (`Hinter` trait, `HistoryHintComplete` event, `FileBackedHistory` for test stubs), `nu-ansi-term` (hint styling).

---

## File Map

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `common_prefix()` (pub crate), add `SqlHinter` struct + `Hinter` impl, add `Hinter`/`History` to reedline imports |
| `src/adapters/driving/repl/mod.rs` | Import `SqlHinter`, add `.with_hinter()` to Reedline builder, update Tab binding to prepend `HistoryHintComplete` |

---

### Task 1: `common_prefix` function

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Write the failing tests**

Append inside the `#[cfg(test)] mod tests` block at the bottom of `src/adapters/driving/repl/completer.rs`:

```rust
#[test]
fn common_prefix_multiple_shared() {
    let cands = vec![
        ("transaction".to_string(), CompletionKind::Table),
        ("transaction_detail".to_string(), CompletionKind::Table),
        ("transaction_shipment".to_string(), CompletionKind::Table),
    ];
    assert_eq!(common_prefix(&cands), "transaction");
}

#[test]
fn common_prefix_single_candidate() {
    let cands = vec![("users".to_string(), CompletionKind::Table)];
    assert_eq!(common_prefix(&cands), "users");
}

#[test]
fn common_prefix_empty_candidates() {
    assert_eq!(common_prefix(&[]), "");
}

#[test]
fn common_prefix_no_shared_chars() {
    let cands = vec![
        ("users".to_string(), CompletionKind::Table),
        ("orders".to_string(), CompletionKind::Table),
    ];
    assert_eq!(common_prefix(&cands), "");
}

#[test]
fn common_prefix_partial_overlap() {
    let cands = vec![
        ("users".to_string(), CompletionKind::Table),
        ("user_sessions".to_string(), CompletionKind::Table),
        ("user_profiles".to_string(), CompletionKind::Table),
    ];
    assert_eq!(common_prefix(&cands), "user");
}

#[test]
fn common_prefix_case_insensitive_preserves_first_case() {
    let cands = vec![
        ("Users".to_string(), CompletionKind::Table),
        ("users_sessions".to_string(), CompletionKind::Table),
    ];
    assert_eq!(common_prefix(&cands), "Users");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test common_prefix -- --nocapture
```

Expected: compile error — `common_prefix` not defined yet.

- [ ] **Step 3: Implement `common_prefix`**

In `src/adapters/driving/repl/completer.rs`, add this function **before** `fn word_start`:

```rust
pub(crate) fn common_prefix(candidates: &[(String, CompletionKind)]) -> String {
    if candidates.is_empty() {
        return String::new();
    }
    let first = &candidates[0].0;
    // Count how many leading chars of `first` are a case-insensitive prefix of every other candidate.
    let prefix_len = candidates[1..].iter().fold(first.chars().count(), |acc, (c, _)| {
        first
            .chars()
            .zip(c.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count()
            .min(acc)
    });
    first.chars().take(prefix_len).collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test common_prefix -- --nocapture
```

Expected: 6 new tests pass, all existing 152 still pass.

- [ ] **Step 5: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): add common_prefix for ghost-text hint computation"
```

---

### Task 2: `SqlHinter` struct

**Files:**
- Modify: `src/adapters/driving/repl/completer.rs`

- [ ] **Step 1: Add `Hinter` and `History` to reedline imports**

Find the existing import at the top of `src/adapters/driving/repl/completer.rs`:

```rust
use reedline::{Completer, Highlighter, Span, StyledText, Suggestion};
```

Replace with:

```rust
use reedline::{Completer, Highlighter, Hinter, History, Span, StyledText, Suggestion};
```

- [ ] **Step 2: Write the failing tests**

Append inside the `#[cfg(test)] mod tests` block:

```rust
fn empty_history() -> reedline::FileBackedHistory {
    reedline::FileBackedHistory::new(0).expect("in-memory history")
}

#[test]
fn hinter_shows_suffix_for_partial_table_match() {
    let schema = schema_with(&["transaction", "transaction_detail"], &[]);
    let mut h = SqlHinter::new(schema);
    let history = empty_history();
    let hint = h.handle("SELECT * FROM tran", 18, &history, false, "");
    assert_eq!(hint, "saction");
}

#[test]
fn hinter_empty_when_no_candidates() {
    let schema = schema_with(&["users"], &[]);
    let mut h = SqlHinter::new(schema);
    let history = empty_history();
    let hint = h.handle("SELECT * FROM xyz", 17, &history, false, "");
    assert_eq!(hint, "");
}

#[test]
fn hinter_empty_when_word_already_equals_prefix() {
    // "users" typed in full, common_prefix == current_word → no hint
    let schema = schema_with(&["users"], &[]);
    let mut h = SqlHinter::new(schema);
    let history = empty_history();
    let input = "SELECT * FROM users";
    let hint = h.handle(input, input.len(), &history, false, "");
    assert_eq!(hint, "");
}

#[test]
fn hinter_complete_hint_returns_stored_suffix() {
    let schema = schema_with(&["transaction", "transaction_detail"], &[]);
    let mut h = SqlHinter::new(schema);
    let history = empty_history();
    h.handle("SELECT * FROM tran", 18, &history, false, "");
    assert_eq!(h.complete_hint(), "saction");
}

#[test]
fn hinter_complete_hint_empty_before_first_handle() {
    let schema = schema_with(&["users"], &[]);
    let h = SqlHinter::new(schema);
    assert_eq!(h.complete_hint(), "");
}

#[test]
fn hinter_shows_column_suffix_via_dot_notation() {
    let schema = schema_with(
        &["users"],
        &[("users", &["email", "email_verified"])],
    );
    let mut h = SqlHinter::new(schema);
    let history = empty_history();
    let input = "SELECT users.em";
    let hint = h.handle(input, input.len(), &history, false, "");
    // common_prefix(["email","email_verified"]) = "email", current_word = "em" → suffix = "ail"
    assert_eq!(hint, "ail");
}

#[test]
fn hinter_clears_after_word_grows_past_prefix() {
    // After accepting "transaction", typing further chars should clear the hint
    let schema = schema_with(&["transaction"], &[]);
    let mut h = SqlHinter::new(schema);
    let history = empty_history();
    // "transactio" → hint = "n"
    let hint1 = h.handle("FROM transactio", 15, &history, false, "");
    assert_eq!(hint1, "n");
    // "transactions" (past the only match) → no hint
    let hint2 = h.handle("FROM transactions", 17, &history, false, "");
    assert_eq!(hint2, "");
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test hinter_ -- --nocapture
```

Expected: compile error — `SqlHinter` not defined yet.

- [ ] **Step 4: Implement `SqlHinter`**

In `src/adapters/driving/repl/completer.rs`, add after the `SqlHighlighter` impl block (at the end of the non-test code, before `#[cfg(test)]`):

```rust
pub struct SqlHinter {
    completer: SqlCompleter,
    current_hint: String,
    style: Style,
}

impl SqlHinter {
    pub fn new(schema: SchemaService) -> Self {
        Self {
            completer: SqlCompleter::new(schema),
            current_hint: String::new(),
            style: Style::new().fg(Color::DarkGray),
        }
    }
}

impl Hinter for SqlHinter {
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        _history: &dyn History,
        use_ansi_coloring: bool,
        _cwd: &str,
    ) -> String {
        let candidates = self.completer.complete_input(line, pos);
        let prefix = common_prefix(&candidates);

        let start = word_start(line, pos);
        let current_word = &line[start..pos];

        self.current_hint = if !prefix.is_empty()
            && prefix.len() > current_word.len()
            && prefix.to_lowercase().starts_with(&current_word.to_lowercase())
        {
            prefix[current_word.len()..].to_string()
        } else {
            String::new()
        };

        if use_ansi_coloring && !self.current_hint.is_empty() {
            self.style.paint(&self.current_hint).to_string()
        } else {
            self.current_hint.clone()
        }
    }

    fn complete_hint(&self) -> String {
        self.current_hint.clone()
    }

    fn next_hint_token(&self) -> String {
        self.current_hint
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }
}
```

- [ ] **Step 5: Run all tests**

```bash
cargo test -- --nocapture
```

Expected: all 159+ tests pass (7 new hinter tests + 6 common_prefix tests + all 152 existing).

- [ ] **Step 6: Commit**

```bash
git add src/adapters/driving/repl/completer.rs
git commit -m "feat(completer): add SqlHinter with common-prefix ghost text"
```

---

### Task 3: Wire SqlHinter + update Tab binding

**Files:**
- Modify: `src/adapters/driving/repl/mod.rs`

> **Note on Space-to-accept:** This binding cannot be cleanly implemented via reedline 0.47 key bindings. `Multiple([Enter, InsertChar(' ')])` works when a menu is open, but when no menu is active `Enter` either submits the line (complete SQL ending `;`) or inserts a newline (incomplete SQL) — both break normal Space. No menu-guard event exists in reedline 0.47 to make this conditional. Accept via **Enter** is available. Space-to-accept is deferred.

- [ ] **Step 1: Import `SqlHinter` in `mod.rs`**

Find:

```rust
use completer::{SqlCompleter, SqlHighlighter};
```

Replace with:

```rust
use completer::{SqlCompleter, SqlHighlighter, SqlHinter};
```

- [ ] **Step 2: Construct `SqlHinter` alongside existing completer and highlighter**

In the `run` function, find:

```rust
    let highlighter = SqlHighlighter::new(schema.clone());
    let completer = SqlCompleter::new(schema);
```

Replace with:

```rust
    let highlighter = SqlHighlighter::new(schema.clone());
    let hinter = SqlHinter::new(schema.clone());
    let completer = SqlCompleter::new(schema);
```

- [ ] **Step 3: Add `SqlHinter` to the Reedline builder**

Find:

```rust
    let mut rl = Reedline::create()
        .with_completer(Box::new(completer))
        .with_highlighter(Box::new(highlighter))
```

Replace with:

```rust
    let mut rl = Reedline::create()
        .with_completer(Box::new(completer))
        .with_hinter(Box::new(hinter))
        .with_highlighter(Box::new(highlighter))
```

- [ ] **Step 4: Update the Tab keybinding to prepend `HistoryHintComplete`**

Find:

```rust
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
```

Replace with:

```rust
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            // Accept ghost text if visible (HistoryHintComplete works for any Hinter, not just history).
            // Returns Inapplicable when no hint is active, so the chain continues.
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
```

- [ ] **Step 5: Verify compilation and run tests**

```bash
cargo test -- --nocapture
```

Expected: all tests pass. No compile errors.

- [ ] **Step 6: Run the REPL and verify behavior manually**

```bash
cargo run -- shell <any-saved-connection-name>
```

Manual checks:
1. Type a partial table name (e.g. `tran`) → dim gray ghost text should appear after cursor showing common-prefix suffix
2. Press Tab → ghost text accepted, input completes to the common prefix
3. Press Tab again → completion menu opens
4. Press Tab again → moves to next item, input updates in real-time
5. Press Enter → accepts current menu item

If you don't have a real connection, verify compilation is clean:

```bash
cargo build
```

- [ ] **Step 7: Commit**

```bash
git add src/adapters/driving/repl/mod.rs
git commit -m "feat(repl): wire SqlHinter and update Tab to accept ghost text first"
```
