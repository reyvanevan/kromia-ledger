//! Error types for the Kromia Ledger engine.
//!
//! All validation failures are expressed through [`LedgerError`], which implements
//! both [`std::error::Error`] and [`std::fmt::Display`] via the `thiserror` crate.
//! Every error variant is designed to be actionable — the caller can match on it
//! and present a meaningful message to the end user.

use thiserror::Error;

use crate::types::Balance;

/// All errors that can occur when interacting with the ledger.
///
/// These errors are returned from account creation, transaction recording,
/// chain verification, and persistence operations. They are non-exhaustive
/// to allow future extension.
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

    #[error("exchange rate mismatch: expected to_amount={expected}, got {actual}")]
    ExchangeRateMismatch { expected: Balance, actual: Balance },

    #[error("invalid exchange rate: {0} (must be positive)")]
    InvalidExchangeRate(Balance),

    #[error("chain integrity violation at entry {0}")]
    ChainBroken(u64),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("storage error: {0}")]
    Storage(String),
}
