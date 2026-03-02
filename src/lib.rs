//! # Kromia Ledger
//!
//! A deterministic, immutable, and cryptographically chained financial ledger engine.
#![allow(clippy::inconsistent_digit_grouping)]
//!
//! ## Design Principles
//! - **Fixed-point arithmetic**: All monetary values use `i128` with 2-decimal precision
//! - **Double-entry bookkeeping**: Every transaction must satisfy Σ Debit = Σ Credit
//! - **Cryptographic chaining**: SHA-256 hash chain ensures tamper-evident history
//! - **Atomic transactions**: All-or-nothing mutation — no partial state corruption
//! - **Zero floating-point**: Deterministic across native and WASM targets
//!
//! ## Module Structure
//! - [`account`] — Account, AccountId, AccountType, Currency, ExchangeRate, balance logic
//! - [`audit`] — Audit trail metadata (actor, source, notes)
//! - [`transaction`] — Transaction, TransactionLine, recording methods
//! - [`entry`] — LedgerEntry, hash computation, timestamps
//! - [`exchange`] — Cross-currency exchange recording
//! - [`persistence`] — JSON serialization / deserialization
//! - [`queries`] — Read-only queries and integrity checks
//! - [`types`] — Re-export hub for all core types
//! - [`validation`] — Error types
//! - [`chain`] — SHA-256 hash chain
//! - [`mod@reconcile`] — Dataset reconciliation
//! - [`mod@format`] — Balance formatting and parsing/// - [`report`] — Financial reports (Trial Balance, Balance Sheet, Income Statement, General Ledger)
pub mod account;
pub mod audit;
pub mod transaction;
pub mod entry;
pub mod exchange;
pub mod persistence;
pub mod queries;
pub mod types;
pub mod validation;
pub mod chain;
pub mod reconcile;
pub mod format;
pub mod report;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use types::{AccountId, AccountType, Balance, Currency, ExchangeRate, LedgerEntry, Transaction, Account, RATE_SCALE};
pub use validation::LedgerError;
pub use chain::HashChain;
pub use audit::AuditMeta;
pub use reconcile::{ReconcileRecord, ReconcileResult, ReconcileStatus, reconcile};
pub use format::{format_balance, format_balance_with_currency, parse_balance, format_amount, format_amount_with_currency, parse_amount};
pub use report::{TrialBalanceReport, BalanceSheet, IncomeStatement, GeneralLedgerReport, GeneralLedgerLine, ReportRow};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// The core ledger engine.
///
/// All mutations are atomic — if any validation fails, the ledger state
/// remains unchanged. The ledger can be serialized to JSON for persistence
/// and restored with full hash-chain verification.
///
/// Methods are organized across modules:
/// - **Account management**: [`create_account`](Self::create_account), [`deactivate_account`](Self::deactivate_account), [`get_account`](Self::get_account), [`account_by_code`](Self::account_by_code), [`get_balance`](Self::get_balance), [`accounts`](Self::accounts) — in [`account`]
/// - **Transaction recording**: [`record_transaction`](Self::record_transaction), [`record_transaction_at`](Self::record_transaction_at), [`record_transaction_full`](Self::record_transaction_full) — in [`transaction`]
/// - **Currency exchange**: [`record_exchange`](Self::record_exchange), [`record_exchange_at`](Self::record_exchange_at), [`record_exchange_full`](Self::record_exchange_full) — in [`exchange`]
/// - **Queries**: [`entries`](Self::entries), [`find_entry`](Self::find_entry), [`entries_for_account`](Self::entries_for_account), [`entries_in_range`](Self::entries_in_range), [`entries_by_actor`](Self::entries_by_actor), [`verify_chain`](Self::verify_chain), [`trial_balance`](Self::trial_balance), [`trial_balance_by_currency`](Self::trial_balance_by_currency) — in [`queries`]
/// - **Audit trail**: [`record_transaction_audited`](Self::record_transaction_audited), [`record_exchange_audited`](Self::record_exchange_audited) — in [`transaction`], [`exchange`]
/// - **Reports**: [`trial_balance_report`](Self::trial_balance_report), [`balance_sheet`](Self::balance_sheet), [`income_statement`](Self::income_statement), [`general_ledger`](Self::general_ledger) — in [`report`]
/// - **Persistence**: [`save_json`](Self::save_json), [`load_json`](Self::load_json) — in [`persistence`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub(crate) accounts: BTreeMap<AccountId, Account>,
    pub(crate) entries: Vec<LedgerEntry>,
    pub(crate) chain: HashChain,
    pub(crate) idempotency_keys: HashSet<String>,
    pub(crate) next_account_id: u64,
    pub(crate) next_entry_id: u64,
}

impl Ledger {
    /// Create an empty ledger with a fresh hash chain.
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::Ledger;
    ///
    /// let ledger = Ledger::new();
    /// assert!(ledger.verify_chain());
    /// assert_eq!(ledger.trial_balance(), 0);
    /// ```
    pub fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
            entries: Vec::new(),
            chain: HashChain::new(),
            idempotency_keys: HashSet::new(),
            next_account_id: 1,
            next_entry_id: 1,
        }
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}
