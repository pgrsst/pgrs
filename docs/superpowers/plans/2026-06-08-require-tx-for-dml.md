# Require an Explicit Transaction for DML — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject `INSERT`/`UPDATE`/`DELETE` in the `shell` REPL when no explicit transaction is open, so the user always has a `ROLLBACK` escape hatch.

**Architecture:** A small, unit-testable helper `dml_requires_tx(state, sql)` in `modules/cli/src/repl/mod.rs` combines the already-exported `is_dml` (core) with the REPL's existing `TxState`. The `ReplCommand::Sql` arm calls it before executing; if blocked, it prints an error and skips execution, analytics, and any tx-state change. No core changes.

**Tech Stack:** Rust (Cargo workspace), `pgrs-core` API facade (`is_dml`, `TxState`), `pgrs-cli` reedline REPL.

---

### Task 1: Add the `dml_requires_tx` guard helper (with tests)

**Files:**
- Modify: `modules/cli/src/repl/mod.rs:15` (import `is_dml`)
- Modify: `modules/cli/src/repl/mod.rs` (add helper near `is_yes`, ~line 50)
- Test: `modules/cli/src/repl/mod.rs` (add to existing `#[cfg(test)] mod tests`, ~line 302)

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` block in `modules/cli/src/repl/mod.rs` (e.g. right after the `quit_confirmation_accepts_yes_only` test). Note the test module currently only imports `super::ReplCommand` — add the `TxState` import and reference the helper via `super::dml_requires_tx`:

```rust
    #[test]
    fn dml_without_transaction_is_blocked() {
        use pgrs_core::TxState;
        assert!(super::dml_requires_tx(TxState::Idle, "INSERT INTO t VALUES (1)"));
        assert!(super::dml_requires_tx(TxState::Idle, "UPDATE t SET x = 1"));
        assert!(super::dml_requires_tx(TxState::Idle, "DELETE FROM t"));
    }

    #[test]
    fn cte_wrapped_dml_without_transaction_is_blocked() {
        use pgrs_core::TxState;
        assert!(super::dml_requires_tx(
            TxState::Idle,
            "WITH c AS (INSERT INTO t VALUES (1) RETURNING id) SELECT * FROM c"
        ));
    }

    #[test]
    fn dml_inside_transaction_is_allowed() {
        use pgrs_core::TxState;
        assert!(!super::dml_requires_tx(TxState::InTransaction, "INSERT INTO t VALUES (1)"));
        assert!(!super::dml_requires_tx(TxState::Failed, "DELETE FROM t"));
    }

    #[test]
    fn non_dml_is_never_blocked() {
        use pgrs_core::TxState;
        assert!(!super::dml_requires_tx(TxState::Idle, "SELECT * FROM t"));
        assert!(!super::dml_requires_tx(TxState::Idle, "CREATE TABLE t (id int)"));
        assert!(!super::dml_requires_tx(TxState::Idle, "BEGIN"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pgrs-cli dml_ 2>&1 | tail -20`
Expected: FAIL — `cannot find function dml_requires_tx in module super` (compile error).

- [ ] **Step 3: Add the import**

Change the core import on `modules/cli/src/repl/mod.rs:15` from:

```rust
use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi, TxState, next_tx_state, tx_effect};
```

to:

```rust
use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi, TxState, is_dml, next_tx_state, tx_effect};
```

- [ ] **Step 4: Implement the helper**

Add this directly above the `is_yes` function (around line 50, before `/// True only for an explicit affirmative confirmation.`):

```rust
/// True if `sql` is a row-mutating statement (INSERT/UPDATE/DELETE, including
/// CTE-wrapped DML) submitted with no open transaction. Such statements are
/// rejected so the user always retains a ROLLBACK escape hatch.
fn dml_requires_tx(state: TxState, sql: &str) -> bool {
    state == TxState::Idle && is_dml(sql)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p pgrs-cli dml_ 2>&1 | tail -20`
Expected: PASS — 4 new tests pass.

- [ ] **Step 6: Commit**

```bash
git add modules/cli/src/repl/mod.rs
git commit -m "feat(cli): add dml_requires_tx guard helper

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Enforce the guard in the REPL `Sql` dispatch arm

**Files:**
- Modify: `modules/cli/src/repl/mod.rs:259-282` (the `ReplCommand::Sql(sql)` arm)

- [ ] **Step 1: Add the guard at the top of the `Sql` arm**

In the `ReplCommand::Sql(sql) => { ... }` arm (currently starting at line 259), insert the guard as the very first thing inside the arm, before the `let ok = handler.handle_sql(...)` call:

```rust
                        ReplCommand::Sql(sql) => {
                            if dml_requires_tx(*tx.lock().unwrap(), sql) {
                                writeln!(
                                    stdout,
                                    "error: INSERT/UPDATE/DELETE requires an explicit transaction. Run BEGIN (or \\begin) first."
                                ).ok();
                                continue;
                            }
                            let ok = handler.handle_sql(
```

The rest of the arm (the `handle_sql` call, `tx`-state update, and the
`Failed`-state notice) stays exactly as it is. `continue` skips the loop
iteration so the blocked statement is never executed, never recorded, and
leaves `TxState` untouched.

- [ ] **Step 2: Verify the whole workspace compiles and tests pass**

Run: `cargo test --workspace 2>&1 | tail -25`
Expected: PASS — all existing tests plus Task 1's tests pass, no warnings about unused `dml_requires_tx`.

- [ ] **Step 3: Manual smoke check (optional but recommended)**

Build and eyeball the behaviour against a real DB if one is available:

Run: `cargo build -p pgrs-cli 2>&1 | tail -5`
Expected: clean build. (Full manual REPL check — `INSERT` at idle prompt `>` is rejected, but works after `\begin` at prompt `*>` — can be done with `pgrs shell <name>` if a connection exists.)

- [ ] **Step 4: Commit**

```bash
git add modules/cli/src/repl/mod.rs
git commit -m "feat(cli): reject DML when no transaction is open

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Document the guardrail in `\help`

**Files:**
- Modify: `modules/cli/src/repl/ui.rs:99-104` (the `repl_help_text` format string)
- Test: `modules/cli/src/repl/ui.rs` (add to existing `mod tests`)

- [ ] **Step 1: Write the failing test**

Add this test inside the existing `#[cfg(test)] mod tests` block in `modules/cli/src/repl/ui.rs`:

```rust
    #[test]
    fn help_mentions_transaction_requirement_for_dml() {
        let text = repl_help_text();
        assert!(
            text.to_uppercase().contains("BEGIN"),
            "help should explain DML needs a transaction, got: {text}"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p pgrs-cli help_mentions_transaction_requirement_for_dml 2>&1 | tail -15`
Expected: FAIL — the help text does not yet mention BEGIN. (Note: the `\begin` command row contains "BEGIN" too; if this test unexpectedly passes, change the assertion to look for the distinct phrase `text.contains("INSERT/UPDATE/DELETE")` and proceed.)

- [ ] **Step 3: Add the note to the help text**

In `repl_help_text` (`modules/cli/src/repl/ui.rs:99`), extend the leading
explanatory paragraph. Change:

```rust
    format!(
        "  Type any SQL and end it with ';' to run it (Enter alone continues a\n\
           multi-line statement until the ';').\n\n\
         {commands}"
    )
```

to:

```rust
    format!(
        "  Type any SQL and end it with ';' to run it (Enter alone continues a\n\
           multi-line statement until the ';').\n\
           INSERT/UPDATE/DELETE require an open transaction — run BEGIN (\\begin) first.\n\n\
         {commands}"
    )
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p pgrs-cli help_mentions_transaction_requirement_for_dml 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/ui.rs
git commit -m "docs(cli): note DML-needs-transaction rule in \\help

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Document the guardrail in CLAUDE.md

**Files:**
- Modify: `CLAUDE.md` (the "shell vs connect" or "Multi-line statements" area of the pgrs-cli notes)

- [ ] **Step 1: Add a documentation line**

In `CLAUDE.md`, immediately after the existing **`shell` vs `connect`:** bullet,
add a new bolded note:

```markdown
**DML transaction guard:** In the `shell` REPL, `INSERT`/`UPDATE`/`DELETE` (and CTE-wrapped DML) are rejected unless a transaction is open — the user must run `BEGIN`/`\begin` first. Enforced in `repl/mod.rs` via `dml_requires_tx` (built on core's `is_dml` + the tracked `TxState`); `connect`/`psql` is unaffected.
```

- [ ] **Step 2: Verify the workspace still builds (docs-only, sanity)**

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: clean — no code changed, confirms the tree is still green.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document DML transaction guard in CLAUDE.md

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review Notes

- **Spec coverage:** Always-on enforcement (Task 2) ✓; DML-only via `is_dml` (Task 1) ✓; REPL-only, no core/`connect` changes ✓; blocked statement not executed/recorded and `TxState` untouched via `continue` (Task 2) ✓; error message verbatim from spec (Task 2) ✓; `\help` note (Task 3) ✓; CLAUDE.md note (Task 4) ✓; unit tests for the truth table (Task 1) ✓.
- **Placeholder scan:** none — every code/test step shows full content.
- **Type consistency:** `dml_requires_tx(TxState, &str) -> bool` defined in Task 1 and called identically in Task 2; `is_dml`/`TxState` are existing exported core symbols.
