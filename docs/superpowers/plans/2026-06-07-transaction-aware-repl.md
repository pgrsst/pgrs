# Transaction-aware REPL Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `pgrs shell` REPL aware of PostgreSQL transaction state — show it in the prompt, add `\begin`/`\commit`/`\rollback`, warn-and-rollback on exit, and notify when a statement aborts an open transaction.

**Architecture:** `pgrs-core` gains a pure transaction-state module (`query/transaction.rs`): a statement classifier (`tx_effect`, via the existing `sqlparser` path) and a pure state machine (`next_tx_state`). The REPL owns the live `TxState` (mirroring how it already owns `expanded`/`timing`) and feeds those two functions; no live-connection port or adapter changes — `BEGIN`/`COMMIT`/`ROLLBACK` already flow through `DbConnection::execute`.

**Tech Stack:** Rust (Cargo workspace), `sqlparser` 0.62, `reedline`, `postgres` 0.19.

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `modules/core/src/query/classify.rs` | Modify | Make `parse_first_statement` reusable (`pub(super)`). |
| `modules/core/src/query/transaction.rs` | Create | `TxState`, `TxEffect`, `tx_effect()`, `next_tx_state()` + tests. |
| `modules/core/src/query/mod.rs` | Modify | `pub mod transaction;`. |
| `modules/core/src/lib.rs` | Modify | Re-export `TxState`, `TxEffect`, `tx_effect`, `next_tx_state`. |
| `CLAUDE.md` | Modify | Note the new exports + `query/transaction.rs` in the layout/boundary docs. |
| `modules/cli/src/repl/command_handler.rs` | Modify | `handle_sql` returns `bool` (execution succeeded). |
| `modules/cli/src/repl/ui.rs` | Modify | `PgrsPrompt` holds `Rc<Cell<TxState>>`; indicator shows `*`/`!`; help text gains the 3 commands. |
| `modules/cli/src/repl/mod.rs` | Modify | Map `\begin`/`\commit`/`\rollback`; own `Rc<Cell<TxState>>`; update state after SQL; aborted-tx notice; exit confirmation + rollback. |

---

## Task 1: Core — make `parse_first_statement` reusable

**Files:**
- Modify: `modules/core/src/query/classify.rs:10-14`

- [ ] **Step 1: Change visibility so the new module can reuse the parser**

In `modules/core/src/query/classify.rs`, change the `parse_first_statement` signature from private to `pub(super)` (visible within the `query` module) and keep its body unchanged:

```rust
pub(super) fn parse_first_statement(query: &str) -> Option<Statement> {
    Parser::parse_sql(&PostgreSqlDialect {}, query)
        .ok()
        .and_then(|mut stmts| if stmts.is_empty() { None } else { Some(stmts.remove(0)) })
}
```

- [ ] **Step 2: Verify the crate still compiles**

Run: `cargo check -p pgrs-core`
Expected: compiles clean (no warnings about the visibility change; `is_ddl`/`is_dml` still use it).

- [ ] **Step 3: Commit**

```bash
git add modules/core/src/query/classify.rs
git commit -m "refactor(core): expose parse_first_statement within query module"
```

---

## Task 2: Core — `TxEffect` classifier

**Files:**
- Create: `modules/core/src/query/transaction.rs`
- Modify: `modules/core/src/query/mod.rs:1-3`

- [ ] **Step 1: Register the module**

In `modules/core/src/query/mod.rs`, add the line (keep alphabetical-ish order with the rest):

```rust
pub mod alias;
pub mod classify;
pub mod tokenizer;
pub mod transaction;
```

- [ ] **Step 2: Write the failing tests for `tx_effect`**

Create `modules/core/src/query/transaction.rs` with only the type, a stub, and the tests:

```rust
//! Transaction-state tracking for an interactive SQL session. Two pure pieces:
//! `tx_effect` classifies a statement's transaction-control effect (via the same
//! `sqlparser` path as `classify`), and `next_tx_state` is the state machine a
//! front-end drives. The live `TxState` is owned by the front-end (the REPL),
//! since `postgres` 0.19 does not expose the protocol-level transaction status.

use sqlparser::ast::Statement;

use super::classify::parse_first_statement;

/// What a single statement does to the surrounding transaction block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxEffect {
    Begin,
    Commit,
    Rollback,
    RollbackToSavepoint,
    Savepoint,
    ReleaseSavepoint,
    None,
}

/// Classify the transaction-control effect of the first statement in `sql`.
/// Anything that is not transaction control (or fails to parse) is `None`.
pub fn tx_effect(sql: &str) -> TxEffect {
    match parse_first_statement(sql) {
        Some(Statement::StartTransaction { .. }) => TxEffect::Begin,
        Some(Statement::Commit { .. }) => TxEffect::Commit,
        Some(Statement::Rollback { savepoint: Some(_), .. }) => TxEffect::RollbackToSavepoint,
        Some(Statement::Rollback { savepoint: None, .. }) => TxEffect::Rollback,
        Some(Statement::Savepoint { .. }) => TxEffect::Savepoint,
        Some(Statement::ReleaseSavepoint { .. }) => TxEffect::ReleaseSavepoint,
        _ => TxEffect::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_begin_and_start_transaction() {
        assert_eq!(tx_effect("BEGIN;"), TxEffect::Begin);
        assert_eq!(tx_effect("begin"), TxEffect::Begin);
        assert_eq!(tx_effect("START TRANSACTION;"), TxEffect::Begin);
    }

    #[test]
    fn classifies_commit_and_end() {
        assert_eq!(tx_effect("COMMIT;"), TxEffect::Commit);
        assert_eq!(tx_effect("END;"), TxEffect::Commit);
    }

    #[test]
    fn classifies_rollback_and_rollback_to_savepoint() {
        assert_eq!(tx_effect("ROLLBACK;"), TxEffect::Rollback);
        assert_eq!(tx_effect("ROLLBACK TO SAVEPOINT sp;"), TxEffect::RollbackToSavepoint);
    }

    #[test]
    fn classifies_savepoint_and_release() {
        assert_eq!(tx_effect("SAVEPOINT sp;"), TxEffect::Savepoint);
        assert_eq!(tx_effect("RELEASE SAVEPOINT sp;"), TxEffect::ReleaseSavepoint);
    }

    #[test]
    fn non_transaction_statements_are_none() {
        assert_eq!(tx_effect("SELECT 1;"), TxEffect::None);
        assert_eq!(tx_effect("INSERT INTO t VALUES (1);"), TxEffect::None);
        assert_eq!(tx_effect("CREATE TABLE t (id int);"), TxEffect::None);
        assert_eq!(tx_effect("not valid sql @#$"), TxEffect::None);
    }
}
```

- [ ] **Step 3: Run the tests to verify they pass**

The `tx_effect` body is already written above, so the tests should pass immediately (this is a classifier over a verified `sqlparser` mapping — the probe in the design confirmed every variant).

Run: `cargo test -p pgrs-core transaction::tests::classifies -- --nocapture` and `cargo test -p pgrs-core transaction::tests::non_transaction`
Expected: PASS (5 tests in the module so far).

- [ ] **Step 4: Commit**

```bash
git add modules/core/src/query/mod.rs modules/core/src/query/transaction.rs
git commit -m "feat(core): classify transaction-control statements (tx_effect)"
```

---

## Task 3: Core — `TxState` + `next_tx_state` state machine

**Files:**
- Modify: `modules/core/src/query/transaction.rs`

- [ ] **Step 1: Write the failing tests for the state machine**

Append these tests inside the existing `mod tests` block in `modules/core/src/query/transaction.rs`:

```rust
    #[test]
    fn begin_from_idle_enters_transaction() {
        assert_eq!(
            next_tx_state(TxState::Idle, TxEffect::Begin, true),
            TxState::InTransaction
        );
    }

    #[test]
    fn failed_begin_stays_idle() {
        assert_eq!(next_tx_state(TxState::Idle, TxEffect::Begin, false), TxState::Idle);
    }

    #[test]
    fn non_tx_statement_keeps_idle() {
        assert_eq!(next_tx_state(TxState::Idle, TxEffect::None, true), TxState::Idle);
    }

    #[test]
    fn error_inside_transaction_marks_failed() {
        assert_eq!(
            next_tx_state(TxState::InTransaction, TxEffect::None, false),
            TxState::Failed
        );
    }

    #[test]
    fn commit_or_rollback_returns_to_idle() {
        assert_eq!(
            next_tx_state(TxState::InTransaction, TxEffect::Commit, true),
            TxState::Idle
        );
        assert_eq!(
            next_tx_state(TxState::InTransaction, TxEffect::Rollback, true),
            TxState::Idle
        );
    }

    #[test]
    fn successful_statement_stays_in_transaction() {
        assert_eq!(
            next_tx_state(TxState::InTransaction, TxEffect::None, true),
            TxState::InTransaction
        );
        assert_eq!(
            next_tx_state(TxState::InTransaction, TxEffect::Savepoint, true),
            TxState::InTransaction
        );
    }

    #[test]
    fn rollback_clears_failed_state() {
        assert_eq!(next_tx_state(TxState::Failed, TxEffect::Rollback, true), TxState::Idle);
    }

    #[test]
    fn commit_in_failed_state_returns_to_idle() {
        // Postgres turns COMMIT in a failed tx into a rollback; either ok or not,
        // the block ends.
        assert_eq!(next_tx_state(TxState::Failed, TxEffect::Commit, false), TxState::Idle);
    }

    #[test]
    fn rollback_to_savepoint_recovers_failed_state() {
        assert_eq!(
            next_tx_state(TxState::Failed, TxEffect::RollbackToSavepoint, true),
            TxState::InTransaction
        );
    }

    #[test]
    fn aborted_statement_keeps_failed_state() {
        // Any non-recovering statement in a failed tx errors with 25P02 and stays failed.
        assert_eq!(next_tx_state(TxState::Failed, TxEffect::None, false), TxState::Failed);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pgrs-core transaction`
Expected: FAIL — `TxState` and `next_tx_state` are not defined yet (compile error).

- [ ] **Step 3: Implement `TxState` and `next_tx_state`**

Add this above the `#[cfg(test)]` block in `modules/core/src/query/transaction.rs`:

```rust
/// The session's transaction status, tracked client-side and surfaced in the
/// REPL prompt. `Copy` so it can live in a `Cell` shared with the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    /// No open transaction (autocommit).
    Idle,
    /// Inside an open transaction block.
    InTransaction,
    /// Inside a transaction that hit an error; only ROLLBACK/COMMIT clears it.
    Failed,
}

/// Pure transition: given the current state, the effect of the statement just
/// run, and whether it succeeded, return the next state. The REPL calls this
/// after every submission. See the design doc for the full transition table.
pub fn next_tx_state(state: TxState, effect: TxEffect, succeeded: bool) -> TxState {
    match state {
        TxState::Idle => match effect {
            TxEffect::Begin if succeeded => TxState::InTransaction,
            _ => TxState::Idle,
        },
        TxState::InTransaction => {
            if !succeeded {
                return TxState::Failed;
            }
            match effect {
                TxEffect::Commit | TxEffect::Rollback => TxState::Idle,
                _ => TxState::InTransaction,
            }
        }
        TxState::Failed => match effect {
            // COMMIT in a failed tx is turned into a rollback by Postgres; either
            // way the block ends, so leaving on the attempt is correct.
            TxEffect::Commit => TxState::Idle,
            TxEffect::Rollback if succeeded => TxState::Idle,
            TxEffect::RollbackToSavepoint if succeeded => TxState::InTransaction,
            _ => TxState::Failed,
        },
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p pgrs-core transaction`
Expected: PASS (all `tx_effect` + `next_tx_state` tests).

- [ ] **Step 5: Commit**

```bash
git add modules/core/src/query/transaction.rs
git commit -m "feat(core): add TxState and next_tx_state transition machine"
```

---

## Task 4: Core — re-export the transaction API

**Files:**
- Modify: `modules/core/src/lib.rs:59-62`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add the re-exports**

In `modules/core/src/lib.rs`, add a line to the SQL-text-helpers re-export block so it reads:

```rust
// --- SQL text helpers used by the REPL front-end for highlighting/tokenizing ---
pub use query::alias::SQL_KEYWORDS;
pub use query::classify::{is_ddl, is_dml};
pub use query::tokenizer::{SqlToken, tokenize};
pub use query::transaction::{next_tx_state, tx_effect, TxEffect, TxState};
```

- [ ] **Step 2: Verify the workspace compiles**

Run: `cargo check --workspace`
Expected: compiles clean — the new symbols are now reachable from `pgrs_core`.

- [ ] **Step 3: Update CLAUDE.md docs**

In `CLAUDE.md`, in the `query/` description line, add `transaction.rs`:

> `query/` — tokenizer.rs (...), alias.rs (...), classify.rs (is_ddl / is_dml, sqlparser-based), **transaction.rs (TxState, TxEffect, tx_effect, next_tx_state — client-side transaction-state tracking)**

And in the **API boundary (strict)** import list, append `tx_effect, next_tx_state, TxState, TxEffect` to the enumerated symbols.

- [ ] **Step 4: Commit**

```bash
git add modules/core/src/lib.rs CLAUDE.md
git commit -m "feat(core): export transaction-state API; document it"
```

---

## Task 5: CLI — `handle_sql` returns whether execution succeeded

**Files:**
- Modify: `modules/cli/src/repl/command_handler.rs:109-149`
- Test: `modules/cli/src/repl/command_handler.rs` (existing `mod tests`)

- [ ] **Step 1: Write the failing test**

Add these two tests to the `mod tests` block in `modules/cli/src/repl/command_handler.rs`:

```rust
    #[test]
    fn handle_sql_returns_true_on_success() {
        let query = StubDb::ok(vec![vec!["1".to_string()]], vec!["id".to_string()]).into_query();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        let ok = handler().handle_sql(&query, "SELECT 1", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| {}, &mut out);
        assert!(ok, "successful execution should return true");
    }

    #[test]
    fn handle_sql_returns_false_on_error() {
        let query = StubDb::err("syntax error").into_query();
        let mut schema = schema_from(&[]);
        let mut out = Vec::new();
        let ok = handler().handle_sql(&query, "SELEKT *", &SqlOptions { expanded: false, timing: false, connection_name: "mydb", analytics: None }, &mut schema, &mut |_| {}, &mut out);
        assert!(!ok, "failed execution should return false");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pgrs-cli handle_sql_returns`
Expected: FAIL — `handle_sql` currently returns `()`, so `let ok = ...; assert!(ok)` is a type/compile error.

- [ ] **Step 3: Change `handle_sql` to return `bool`**

In `modules/cli/src/repl/command_handler.rs`, change the signature and add the returns. The full method becomes:

```rust
    pub(super) fn handle_sql(
        &self,
        query_api: &QueryApi,
        query: &str,
        opts: &SqlOptions<'_>,
        schema: &mut SchemaApi,
        rebuild: &mut impl FnMut(SchemaApi),
        writer: &mut impl Write,
    ) -> bool {
        let start = std::time::Instant::now();
        match query_api.execute(query) {
            Ok(result) => {
                write!(writer, "{}", format_result(&result, opts.expanded)).ok();
                if opts.timing {
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    if ms >= 1000.0 {
                        writeln!(writer, "Time: {:.3} s", ms / 1000.0).ok();
                    } else {
                        writeln!(writer, "Time: {:.3} ms", ms).ok();
                    }
                }

                if let Some(analytics) = opts.analytics
                    && let Err(e) = analytics.record_query(opts.connection_name, query, schema)
                {
                    writeln!(writer, "pgrs: analytics write failed: {e}").ok();
                }

                if is_ddl(query) {
                    match schema.refresh(query_api, opts.connection_name) {
                        Ok(()) => {
                            rebuild(schema.clone());
                            writeln!(writer, "(schema refreshed)").ok();
                        }
                        Err(e) => { writeln!(writer, "error: could not refresh schema: {e}").ok(); }
                    }
                }
                true
            }
            Err(e) => {
                writeln!(writer, "error: {}", e).ok();
                false
            }
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p pgrs-cli handle_sql`
Expected: PASS (new return-value tests plus the existing `handle_sql_*` tests, whose statement-position calls still compile with the dropped `bool`).

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/command_handler.rs
git commit -m "feat(cli): handle_sql returns execution success"
```

---

## Task 6: CLI — prompt transaction indicator

**Files:**
- Modify: `modules/cli/src/repl/ui.rs:1-2,14-31`
- Test: `modules/cli/src/repl/ui.rs` (existing `mod tests`)

- [ ] **Step 1: Update existing `PgrsPrompt` constructions and write the failing test**

`PgrsPrompt` will gain a `tx` field, so the four existing prompt tests must construct it. In `modules/cli/src/repl/ui.rs`, add imports at the top of the `mod tests` block and update each `PgrsPrompt { .. }` literal to include `tx`. Add a new test for the indicator. The relevant additions/edits to `mod tests`:

```rust
    use std::cell::Cell;
    use std::rc::Rc;
    use pgrs_core::TxState;

    fn prompt_with_tx(state: TxState) -> PgrsPrompt {
        PgrsPrompt {
            db_name: "mydb".to_string(),
            environment: None,
            tx: Rc::new(Cell::new(state)),
        }
    }

    #[test]
    fn indicator_is_plain_when_idle() {
        let p = prompt_with_tx(TxState::Idle);
        assert_eq!(p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(), "> ");
    }

    #[test]
    fn indicator_marks_open_transaction() {
        let p = prompt_with_tx(TxState::InTransaction);
        assert_eq!(p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(), "*> ");
    }

    #[test]
    fn indicator_marks_failed_transaction() {
        let p = prompt_with_tx(TxState::Failed);
        assert_eq!(p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(), "!> ");
    }
```

Also update the four existing tests (`prompt_left_with_environment_shows_env`, `prompt_left_without_environment_omits_env`, `prompt_left_includes_database_name`, `prompt_left_format_is_pgrs_parens_name`) to add `tx: Rc::new(Cell::new(TxState::Idle)),` to each `PgrsPrompt { .. }` literal.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pgrs-cli --lib ui`
Expected: FAIL — `PgrsPrompt` has no `tx` field yet (compile error).

- [ ] **Step 3: Add the `tx` field and the indicator logic**

In `modules/cli/src/repl/ui.rs`, add imports near the top:

```rust
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
```

and `use pgrs_core::{SchemaApi, TxState};` (extend the existing `pgrs_core` import).

Change the struct and the indicator method:

```rust
pub(super) struct PgrsPrompt {
    pub(super) db_name: String,
    pub(super) environment: Option<String>,
    /// Shared with the REPL loop, which updates it after each statement so the
    /// prompt reflects the current transaction status.
    pub(super) tx: Rc<Cell<TxState>>,
}
```

```rust
    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        match self.tx.get() {
            TxState::Idle => Cow::Borrowed("> "),
            TxState::InTransaction => Cow::Borrowed("*> "),
            TxState::Failed => Cow::Borrowed("!> "),
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p pgrs-cli --lib ui`
Expected: PASS (3 new indicator tests + 4 updated prompt-left tests).

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/ui.rs
git commit -m "feat(cli): show transaction status in REPL prompt indicator"
```

---

## Task 7: CLI — `\begin` / `\commit` / `\rollback` commands + help

**Files:**
- Modify: `modules/cli/src/repl/mod.rs:71-100` (parser)
- Modify: `modules/cli/src/repl/ui.rs:64-79` (help table)
- Test: `modules/cli/src/repl/mod.rs` (existing `mod tests`), `modules/cli/src/repl/ui.rs`

These three backslash commands are thin aliases that route to the existing SQL
path as `ReplCommand::Sql("BEGIN" | "COMMIT" | "ROLLBACK")`, so transaction-state
tracking and analytics update uniformly with no extra dispatch arm.

- [ ] **Step 1: Write the failing parser tests**

Add to the `mod tests` block in `modules/cli/src/repl/mod.rs`:

```rust
    #[test]
    fn tx_command_aliases_map_to_sql() {
        assert!(matches!(ReplCommand::parse("\\begin"), ReplCommand::Sql("BEGIN")));
        assert!(matches!(ReplCommand::parse("\\commit"), ReplCommand::Sql("COMMIT")));
        assert!(matches!(ReplCommand::parse("\\rollback"), ReplCommand::Sql("ROLLBACK")));
    }
```

Add to the `mod tests` block in `modules/cli/src/repl/ui.rs`:

```rust
    #[test]
    fn help_text_mentions_transaction_commands() {
        let text = repl_help_text();
        assert!(text.contains("\\begin"), "help should mention \\begin, got: {text}");
        assert!(text.contains("\\commit"), "help should mention \\commit, got: {text}");
        assert!(text.contains("\\rollback"), "help should mention \\rollback, got: {text}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pgrs-cli tx_command_aliases_map_to_sql` and `cargo test -p pgrs-cli help_text_mentions_transaction_commands`
Expected: FAIL — `\begin` currently parses to `ReplCommand::Sql("\\begin")`; help omits the commands.

- [ ] **Step 3: Add the parser arms**

In `modules/cli/src/repl/mod.rs`, inside `ReplCommand::parse`, add three arms to the literal `match trimmed` block (next to `"\\refresh" => ...`):

```rust
            "\\begin" => ReplCommand::Sql("BEGIN"),
            "\\commit" => ReplCommand::Sql("COMMIT"),
            "\\rollback" => ReplCommand::Sql("ROLLBACK"),
```

(`ReplCommand::Sql` takes `&'a str`; the `'static` literals satisfy any `'a`.)

- [ ] **Step 4: Add the help entries**

In `modules/cli/src/repl/ui.rs`, add three rows to `REPL_COMMANDS` (place them just before the `\\help` row):

```rust
    ("\\begin",              "begin a transaction (BEGIN)"),
    ("\\commit",             "commit the current transaction (COMMIT)"),
    ("\\rollback",           "roll back the current transaction (ROLLBACK)"),
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p pgrs-cli tx_command_aliases_map_to_sql help_text_mentions_transaction_commands`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add modules/cli/src/repl/mod.rs modules/cli/src/repl/ui.rs
git commit -m "feat(cli): add \\begin/\\commit/\\rollback REPL command aliases"
```

---

## Task 8: CLI — wire `TxState` into the loop + aborted-transaction notice

**Files:**
- Modify: `modules/cli/src/repl/mod.rs:9-14` (imports), `:133-235` (run loop)

- [ ] **Step 1: Add imports**

In `modules/cli/src/repl/mod.rs`, extend the imports near the top:

```rust
use std::cell::Cell;
use std::collections::HashMap;
use std::io::{self, Write};
use std::rc::Rc;

use reedline::{Reedline, Signal};

use pgrs_core::{AnalyticsApi, QueryApi, SchemaApi, TxState, next_tx_state, tx_effect};
```

- [ ] **Step 2: Create the shared `TxState` and give it to the prompt**

In `run`, after `schema.load(...)` and building `rl`, create the shared cell and pass a clone into `PgrsPrompt`. Replace the existing `let prompt = ui::PgrsPrompt { ... };` block with:

```rust
        let tx = Rc::new(Cell::new(TxState::Idle));

        let prompt = ui::PgrsPrompt {
            db_name: db_name.clone(),
            environment: environment.clone(),
            tx: Rc::clone(&tx),
        };
```

- [ ] **Step 3: Update transaction state after each SQL submission**

In the `ReplCommand::Sql(sql) =>` arm of the loop, replace the single `handler.handle_sql(...)` call with a version that captures success and advances the state machine, printing the aborted-transaction notice on the `InTransaction -> Failed` edge:

```rust
                        ReplCommand::Sql(sql) => {
                            let ok = handler.handle_sql(
                                &query,
                                sql,
                                &SqlOptions {
                                    expanded,
                                    timing,
                                    connection_name: &connection_name,
                                    analytics: Some(&analytics),
                                },
                                &mut schema,
                                &mut |s| rebuild_reedline(&mut rl, &analytics, &connection_name, s),
                                &mut stdout,
                            );
                            let prev = tx.get();
                            let next = next_tx_state(prev, tx_effect(sql), ok);
                            tx.set(next);
                            if prev == TxState::InTransaction && next == TxState::Failed {
                                writeln!(
                                    stdout,
                                    "Transaction aborted. Run \\rollback (or ROLLBACK) to recover."
                                ).ok();
                            }
                        }
```

- [ ] **Step 4: Verify it compiles and the existing tests still pass**

Run: `cargo test -p pgrs-cli`
Expected: PASS — no behavior tests broke; the loop now tracks state. (Manual REPL verification of the prompt marker happens in Task 9's verification, since exit handling lands there.)

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/mod.rs
git commit -m "feat(cli): track transaction state across REPL statements"
```

---

## Task 9: CLI — exit protection (confirm + rollback)

**Files:**
- Modify: `modules/cli/src/repl/mod.rs` (add helpers + quit handling in the loop)
- Test: `modules/cli/src/repl/mod.rs` (existing `mod tests`)

- [ ] **Step 1: Write the failing test for the confirmation parser**

Add to the `mod tests` block in `modules/cli/src/repl/mod.rs`:

```rust
    #[test]
    fn quit_confirmation_accepts_yes_only() {
        assert!(super::is_yes("y"));
        assert!(super::is_yes("Y"));
        assert!(super::is_yes("yes"));
        assert!(super::is_yes("  Yes  "));
        assert!(!super::is_yes("n"));
        assert!(!super::is_yes(""));
        assert!(!super::is_yes("nope"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p pgrs-cli quit_confirmation_accepts_yes_only`
Expected: FAIL — `is_yes` is not defined (compile error).

- [ ] **Step 3: Add `is_yes` and `handle_quit_request` helpers**

In `modules/cli/src/repl/mod.rs`, add these free functions near `rebuild_reedline` (module scope):

```rust
/// True only for an explicit affirmative confirmation.
fn is_yes(input: &str) -> bool {
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Handle a quit request. Returns `true` if the REPL should exit. With an open
/// transaction, warns and asks for confirmation; on "yes" (or EOF, since we
/// can't keep prompting) it issues `ROLLBACK` and exits, otherwise it cancels.
fn handle_quit_request(
    query: &QueryApi,
    tx: &Rc<Cell<TxState>>,
    writer: &mut impl Write,
) -> bool {
    if tx.get() == TxState::Idle {
        return true;
    }
    writeln!(writer, "A transaction is in progress. Roll back and quit? [y/N]").ok();
    writer.flush().ok();

    let mut input = String::new();
    let confirmed = match io::stdin().read_line(&mut input) {
        Ok(0) | Err(_) => true, // EOF or read error: cannot keep asking — roll back and quit.
        Ok(_) => is_yes(&input),
    };

    if confirmed {
        if let Err(e) = query.execute("ROLLBACK") {
            writeln!(writer, "warning: rollback failed: {e}").ok();
        }
        tx.set(TxState::Idle);
        true
    } else {
        writeln!(writer, "Quit cancelled.").ok();
        false
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p pgrs-cli quit_confirmation_accepts_yes_only`
Expected: PASS.

- [ ] **Step 5: Route both quit paths through the helper**

In the `run` loop of `modules/cli/src/repl/mod.rs`:

Replace the `ReplCommand::Quit => break,` arm with:

```rust
                        ReplCommand::Quit => {
                            if handle_quit_request(&query, &tx, &mut stdout) {
                                break;
                            }
                        }
```

Replace the signal arm `Ok(Signal::CtrlC) | Ok(Signal::CtrlD) | Ok(Signal::ExternalBreak(_)) => break,` with:

```rust
                Ok(Signal::CtrlC) | Ok(Signal::CtrlD) | Ok(Signal::ExternalBreak(_)) => {
                    let mut stdout = io::stdout();
                    if handle_quit_request(&query, &tx, &mut stdout) {
                        break;
                    }
                }
```

(The signal arm needs its own `stdout`, since the existing one is scoped inside the `Signal::Success` arm.)

- [ ] **Step 6: Verify the whole workspace**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo clippy --workspace`
Expected: no warnings.

- [ ] **Step 7: Manual verification against a live database**

If a Postgres connection is configured (e.g. `localdev`), run:

```bash
cargo run -- shell <name>
```

Confirm, in order:
1. Prompt shows `pgrs(<db>)> ` (idle).
2. `BEGIN;` (or `\begin`) → prompt becomes `pgrs(<db>)*> `.
3. A failing statement (e.g. `SELECT * FROM nonexistent_table;`) → error printed, "Transaction aborted." notice, prompt becomes `pgrs(<db>)!> `.
4. `\rollback` → prompt returns to `pgrs(<db>)> `.
5. `\begin` again, then `\q` → "A transaction is in progress. Roll back and quit? [y/N]"; `n` cancels (still in REPL, prompt `*>`); `\q` then `y` rolls back and exits.

If no live DB is available, note that this step is skipped and rely on the unit tests.

- [ ] **Step 8: Commit**

```bash
git add modules/cli/src/repl/mod.rs
git commit -m "feat(cli): confirm-and-rollback on exit with an open transaction"
```

---

## Self-Review Notes

- **Spec coverage:** prompt indicator (Task 6), `\begin`/`\commit`/`\rollback` (Task 7), exit protection (Task 9), error/aborted notice (Task 8), core classifier + state machine (Tasks 2–3), re-exports + docs (Task 4). All four spec capabilities are covered.
- **Type consistency:** `TxState`/`TxEffect`/`tx_effect`/`next_tx_state` names are identical across core definition (Tasks 2–3), re-export (Task 4), and CLI use (Tasks 6, 8); `handle_sql` returns `bool` (Task 5) and is consumed as `ok` in Task 8.
- **Known limitation:** `tx_effect` inspects only the first statement of a submission (parallels `is_ddl`); documented in the design doc.
