# Transaction-aware REPL — Design

**Date:** 2026-06-07
**Status:** Approved (design), pending implementation plan

## Goal

Make the `pgrs shell` REPL aware of the current PostgreSQL transaction so the
user can see and safely manage transaction state. Four user-facing capabilities:

1. **Prompt indicator** — the prompt reflects whether a transaction is open or
   has failed (psql-style `*` / `!` markers).
2. **`\begin` / `\commit` / `\rollback` commands** — thin backslash aliases for
   the corresponding SQL, alongside typing the SQL directly.
3. **Exit protection** — quitting (`\q`, Ctrl+D, Ctrl+C) with an open transaction
   prompts for confirmation, then rolls back before exiting.
4. **Error awareness** — when a statement fails inside a transaction, the REPL
   tells the user the transaction is aborted (notify-only, like psql).

## Constraint that drives the design

The `postgres` crate (0.19) does **not** expose the protocol-level transaction
status indicator (the `I`/`T`/`E` byte from `ReadyForQuery`) that psql relies on.
Therefore pgrs must track transaction state itself.

Approaches considered:

- **A — client-side classification (chosen).** Classify each executed statement
  with `sqlparser` (already used in `query/classify.rs`) and feed a pure state
  machine. No extra round-trips, consistent with `is_ddl`/`is_dml`, fully
  unit-testable, fits the hexagonal boundary. Trade-off: can drift from the
  server on edge cases (functions that `COMMIT`, multi-statement `simple_query`
  lines). ~95% coverage for a hand-typed REPL.
- **B — ask the server each statement.** Rejected: no reliable SQL exists to ask
  "am I in a transaction block".
- **C — expose the protocol status byte from the adapter.** Rejected: not in the
  crate's public API; would require a fork or low-level protocol work.

## Architecture

Dependency direction is unchanged: `pgrs-cli` → `pgrs_core::api` → services →
ports ← adapters. No live-connection port or adapter changes are needed —
`BEGIN`/`COMMIT`/`ROLLBACK` already flow through `DbConnection::execute`. The
session state persists server-side across `execute()` calls on the same client.

State ownership mirrors the existing pattern where the REPL owns UI/session flags
(`expanded`, `timing`): the **REPL owns the current `TxState`**, and core provides
the pure classification + transition functions. Core stays stateless about the
live connection.

### Core changes (`pgrs-core`)

New module `modules/core/src/query/transaction.rs`, re-exported at the crate root
next to `is_ddl` / `is_dml`:

```rust
pub enum TxState { Idle, InTransaction, Failed }   // derives Copy (usable in Cell)

pub enum TxEffect {
    Begin, Commit, Rollback,
    Savepoint, RollbackToSavepoint, ReleaseSavepoint,
    None,
}

/// Classify the first statement's transaction-control effect (sqlparser).
pub fn tx_effect(sql: &str) -> TxEffect;

/// Pure transition. `succeeded` = did the statement execute without error.
pub fn next_tx_state(state: TxState, effect: TxEffect, succeeded: bool) -> TxState;
```

Transition rules:

| From            | Event                              | To              |
|-----------------|------------------------------------|-----------------|
| `Idle`          | `Begin` (ok)                       | `InTransaction` |
| `Idle`          | anything else                      | `Idle`          |
| `InTransaction` | statement failed                   | `Failed`        |
| `InTransaction` | `Commit` / `Rollback` (ok)         | `Idle`          |
| `InTransaction` | anything else (ok)                 | `InTransaction` |
| `Failed`        | `Rollback` (ok) or `Commit`        | `Idle`          |
| `Failed`        | `RollbackToSavepoint` (ok)         | `InTransaction` |
| `Failed`        | anything else                      | `Failed`        |

(`Commit` while `Failed`: Postgres turns it into a rollback and returns to idle,
so we treat it as a transition to `Idle`.)

The export list in `modules/core/src/lib.rs` / `CLAUDE.md`'s API-boundary section
gains `TxState`, `TxEffect`, `tx_effect`, `next_tx_state`.

**No changes** to `DbConnection`, `QueryApi`, `PostgresDb`, or the ports.

### CLI changes (`pgrs-cli`)

**Prompt (`repl/ui.rs`).** `PgrsPrompt` gains `tx: Rc<Cell<TxState>>`.
`render_prompt_indicator` becomes:

- `Idle` → `"> "`
- `InTransaction` → `"*> "`
- `Failed` → `"!> "`

**Loop (`repl/mod.rs`).**

- Hold `tx: Rc<Cell<TxState>>`, shared with `PgrsPrompt`.
- After each SQL submission: `tx.set(next_tx_state(tx.get(), tx_effect(sql), ok))`,
  where `ok` is whether execution succeeded. On an `InTransaction → Failed`
  transition, print a notice: the transaction is aborted; run `ROLLBACK`
  (or `\rollback`) to recover.
- New `ReplCommand` variants `Begin` / `Commit` / `Rollback` for `\begin` /
  `\commit` / `\rollback`, executing the corresponding SQL through the same path
  as typed SQL so state and analytics update uniformly.
- Exit protection: on `Quit` / Ctrl+D / Ctrl+C, if `tx.get() != Idle`, print a
  warning and read a `[y/N]` confirmation. On `y`: `execute("ROLLBACK")` then
  exit. Otherwise: cancel the quit and stay in the loop.

**`handle_sql` (`repl/command_handler.rs`).** Return a `bool` (execution
succeeded) so the loop can drive the state machine. Existing behavior (result
formatting, timing, analytics, DDL schema refresh) is unchanged.

**Help (`repl/ui.rs`).** Add `\begin`, `\commit`, `\rollback` entries to
`REPL_COMMANDS`.

## Error handling

- Statement failure inside a transaction → `Failed` state + a printed notice;
  the REPL does not auto-rollback (psql-style notify-only). The user clears it
  with `\rollback` / `ROLLBACK`.
- `ROLLBACK` issued during exit confirmation: if it errors (e.g. connection
  already dropped), print the error but still exit — the user asked to quit.

## Testing

- **Core (`query/transaction.rs`):** table-driven tests for `tx_effect`
  classification (BEGIN/START TRANSACTION, COMMIT/END, ROLLBACK, SAVEPOINT,
  ROLLBACK TO, RELEASE, non-transaction statements) and for every `next_tx_state`
  transition in the table above.
- **CLI:**
  - `ReplCommand::parse` recognizes `\begin` / `\commit` / `\rollback`.
  - `handle_sql` returns `true` on success and `false` on error.
  - `render_prompt_indicator` returns the correct marker for each `TxState`.
  - Exit-confirmation flow rolls back on `y` and stays on other input
    (tested at whatever seam keeps it deterministic without a live terminal).

## Known limitations (to document)

- `tx_effect` inspects only the first statement of a submission. A single
  multi-statement line via `simple_query` (e.g. `BEGIN; INSERT ...; COMMIT;`) can
  desync the tracked state. Rare for a hand-typed REPL; parallels the existing
  `is_ddl` first-statement limitation.
- Server-side transaction control issued indirectly (a function or `DO` block
  that commits) is invisible to client-side classification.
