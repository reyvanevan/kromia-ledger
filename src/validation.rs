use thiserror::Error;

use crate::types::Balance;

/// All possible errors from ledger operations.
#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("transaction is unbalanced: debit={debit}, credit={credit}")]
    Unbalanced { debit: Balance, credit: Balance },

    #[error("transaction must have at least one line")]
    EmptyTransaction,

    #[error("invalid amount: {0} (must be positive)")]
    InvalidAmount(Balance),

    #[error("account not found: {0}")]
    AccountNotFound(u64),

    #[error("chain integrity violation at entry {0}")]
    ChainBroken(u64),
}
