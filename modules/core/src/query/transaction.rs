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
