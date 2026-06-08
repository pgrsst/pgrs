# Require an explicit transaction for DML

## Problem

In the `shell` REPL, `INSERT`/`UPDATE`/`DELETE` run in autocommit by default —
a single mistyped statement permanently mutates data with no chance to roll
back. We want a guardrail: row-mutating statements must be issued inside an
explicit transaction, so the user always has a `ROLLBACK` escape hatch.

## Decision

- **Always on.** No toggle, no CLI flag. DML outside a transaction is always
  rejected.
- **DML only.** `INSERT`, `UPDATE`, `DELETE`, and DML wrapped in a CTE
  (`WITH ... INSERT ... RETURNING ...`). DDL (CREATE/DROP/ALTER/TRUNCATE) and
  everything else are unaffected. This reuses the existing `is_dml`.
- **REPL only.** `connect` hands off to `psql` and is outside our control. Only
  the built-in `shell` REPL enforces this.

## Behaviour

In `repl/mod.rs`, the `ReplCommand::Sql(sql)` arm gains a pre-execution check:

```
if tx == TxState::Idle && is_dml(sql) {
    print error
    // do NOT execute, do NOT record analytics, do NOT change tx state
    continue
}
```

- **When blocked:** only when the session is `TxState::Idle` (autocommit). Inside
  `InTransaction` the statement runs normally. In `Failed`, the statement is
  passed through to the server as today (the server rejects it with 25P02); we do
  not special-case it.
- **Error message:**
  ```
  error: INSERT/UPDATE/DELETE requires an explicit transaction. Run BEGIN (or \begin) first.
  ```
- **Side effects of a blocked statement:** none. It is not executed, not written
  to query history / analytics, and `TxState` stays `Idle`.

## Where the logic lives

A small, unit-testable helper in `repl/mod.rs`:

```rust
/// True if `sql` is a row-mutating statement that must not run outside a
/// transaction given the current state.
fn dml_requires_tx(state: TxState, sql: &str) -> bool {
    state == TxState::Idle && is_dml(sql)
}
```

This mirrors the file's existing "dumb parser + separately-tested logic"
pattern (`ReplCommand::parse`, `is_yes`). The `Sql` arm calls it before
`handler.handle_sql(...)`.

No core changes: `is_dml`, `TxState`, `tx_effect`, `next_tx_state` are already
exported across the API boundary.

## Testing

Unit tests for `dml_requires_tx`:

- `INSERT` + `Idle` → true (blocked)
- `UPDATE` + `Idle` → true
- `DELETE` + `Idle` → true
- CTE-wrapped `INSERT` + `Idle` → true
- `INSERT` + `InTransaction` → false (allowed)
- `SELECT` + `Idle` → false
- `CREATE TABLE` + `Idle` → false (DDL out of scope)

Plus a behavioural check that the error message is printed and the statement is
not executed when blocked.

## Docs

- One line in `\help` (`ui.rs`) noting the rule.
- One line in `CLAUDE.md` describing the guardrail.
