# Ghost Text Completion Design

**Date:** 2026-05-22
**Scope:** `src/adapters/driving/repl/completer.rs`, `src/adapters/driving/repl/mod.rs`

## Background

pgrs has a SQL REPL with context-aware autocomplete and fuzzy matching. The current Tab behavior opens a columnar menu and cycles through items. This design adds an inline ghost text layer (common prefix hint) that appears as the user types, making single-item acceptance faster without opening the menu.

## Goals

1. **Ghost text (inline hint)** — as the user types, show the longest common prefix of all fuzzy matches as dim gray text after the cursor.
2. **Tab #1 accepts ghost text** — if ghost text is visible, Tab completes to the common prefix and hides the ghost text.
3. **Tab #2 opens menu** — if no ghost text (or after accepting it), Tab opens the full completion menu as before.
4. **Tab cycles menu + updates input** — while menu is open, Tab moves to the next item and the input updates in real-time to match.
5. **Space accepts menu selection + inserts space** — a natural shortcut for mid-query completion (e.g., `SELECT transaction_detail FROM`).

## Non-goals

- Changing the visual style of the menu (colors, columns, layout)
- Arrow key navigation changes
- Accepting ghost text with Right Arrow (use Tab only)

---

## Behavior

```
User types: tran
Input:      tran[saction]          ← "saction" = ghost text, dim gray

Tab #1  → CompleteHint → input becomes "transaction", ghost text gone

Tab #2  → menu opens:
  ┌──────────────────────────────┐
  │ > transaction         [table]│
  │   transaction_detail  [table]│
  │   transaction_shipment[table]│
  └──────────────────────────────┘

Tab #3  → hover moves to transaction_detail, input updates to "transaction_detail"

Space   → accept "transaction_detail" + insert space
           input: "transaction_detail ", menu closes, cursor ready for next token

Enter   → accept current selection, no space inserted
```

---

## Common Prefix Algorithm

`common_prefix(candidates: &[(String, CompletionKind)]) -> String`

- Input: list of (value, kind) completion candidates
- Output: longest string that is a case-insensitive prefix of every value
- Preserve case from the first candidate
- Empty input → empty string

Examples:
```
["transaction", "transaction_detail", "transaction_shipment"] → "transaction"
["users", "user_sessions", "user_profiles"]                   → "user"
["id"]                                                         → "id"
[]                                                             → ""
```

---

## SqlHinter

New struct implementing `reedline::Hinter`. Holds a `SchemaService` and a mutable `SqlCompleter` reference (or re-uses the same schema to construct completions).

**`handle(line, pos, ...) -> String`**
1. Call `complete_input(line, pos)` to get all fuzzy-matched candidates
2. Compute `common_prefix` of candidates
3. Extract `current_word` — the token the user is currently typing (same logic as `word_start`)
4. If `common_prefix` starts with `current_word` (case-insensitive) AND is longer → hint suffix = `common_prefix[current_word.len()..]`
5. Store hint suffix internally
6. Return hint suffix (reedline displays this as dim text after cursor)

**`complete_hint() -> String`**
Returns the stored hint suffix. Reedline appends this to the current input when `CompleteHint` fires.

**`next_hint_token() -> String`**
Returns the first word of the stored hint suffix (used by `CompleteHintWord` if needed).

---

## Tab Binding Change

**File:** `src/adapters/driving/repl/mod.rs`

Current:
```rust
ReedlineEvent::UntilFound(vec![
    ReedlineEvent::Menu("completion_menu"),
    ReedlineEvent::MenuNext,
])
```

New:
```rust
ReedlineEvent::UntilFound(vec![
    ReedlineEvent::CompleteHint,
    ReedlineEvent::Menu("completion_menu"),
    ReedlineEvent::MenuNext,
])
```

`UntilFound` tries each event in order and stops at the first that succeeds:
- `CompleteHint` only succeeds if ghost text is active
- `Menu(...)` only succeeds if menu is not already open
- `MenuNext` runs when menu is already open

---

## Space Binding

**File:** `src/adapters/driving/repl/mod.rs`

Add binding after existing keybindings setup:
```rust
keybindings.add_binding(
    KeyModifiers::NONE,
    KeyCode::Char(' '),
    ReedlineEvent::Multiple(vec![
        ReedlineEvent::Enter,
        ReedlineEvent::Edit(vec![EditCommand::InsertChar(' ')]),
    ]),
);
```

This fires when the menu is active. When the menu is not active, reedline's default handler inserts a literal space as normal — the binding only intercepts when a menu is open.

> **Note:** Verify at implementation time that reedline's `Multiple` event with `Enter` accepts the menu item without submitting the line. If `Enter` submits the line when the menu is open, use the appropriate menu-accept event instead (e.g. `MenuComplete` if reedline exposes it). The intent is: accept selected item, then insert a space character.

---

## Reedline Builder Change

**File:** `src/adapters/driving/repl/mod.rs`

Add `SqlHinter` to the Reedline builder alongside the existing `SqlCompleter`:

```rust
let hinter = SqlHinter::new(schema.clone());

let mut rl = Reedline::create()
    .with_completer(Box::new(completer))
    .with_hinter(Box::new(hinter))          // ← new
    .with_highlighter(Box::new(highlighter))
    // ... rest unchanged
```

---

## File Changes

| File | Change |
|---|---|
| `src/adapters/driving/repl/completer.rs` | Add `common_prefix()`, add `SqlHinter` struct implementing `reedline::Hinter` |
| `src/adapters/driving/repl/mod.rs` | Add `SqlHinter` to builder, update Tab binding, add Space binding |

---

## Tests to Add

### `common_prefix`
- Multiple candidates with shared prefix → correct prefix
- Single candidate → returns that candidate
- No candidates → empty string
- Candidates with no shared prefix → empty string
- Case-insensitive matching, preserves first candidate's case

### `SqlHinter`
- Typing prefix with multiple matches → hint shows common prefix suffix
- Typing prefix with single match → hint shows full completion suffix
- Typing full word (no further completion) → hint empty
- Typing prefix that matches nothing → hint empty
- `complete_hint()` returns stored suffix
