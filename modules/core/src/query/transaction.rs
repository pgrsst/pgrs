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

/// The session's transaction status, tracked client-side and surfaced in the
/// REPL prompt. `Copy` so the REPL can read the current state out of its shared
/// `Mutex` without cloning.
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
}
