use thiserror::Error;

use crate::types::Balance;

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

    #[error("account is inactive: {0}")]
    InactiveAccount(u64),

    #[error("duplicate account code: {0}")]
    DuplicateAccountCode(String),

    #[error("currency mismatch in transaction: expected {expected}, found {found} on account {account_id}")]
    CurrencyMismatch {
        expected: String,
        found: String,
        account_id: u64,
    },

    #[error("duplicate idempotency key: {0}")]
    DuplicateIdempotencyKey(String),

    #[error("chain integrity violation at entry {0}")]
    ChainBroken(u64),

    #[error("serialization error: {0}")]
    Serialization(String),
}
