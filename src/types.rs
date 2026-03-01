//! Core types for the Kromia Ledger engine.
//!
//! This module defines the fundamental data structures:
//!
//! - [`Balance`] — fixed-point i128 monetary value (no floating point)
//! - [`RATE_SCALE`] — scale factor for exchange rate integer arithmetic
//! - [`AccountId`] — unique account identifier
//! - [`Currency`] — ISO 4217 currency metadata with decimal precision
//! - [`ExchangeRate`] — cross-currency exchange rate metadata
//! - [`AccountType`] — chart-of-accounts classification
//! - [`Account`] — a named account with a running balance
//! - [`Transaction`] — a balanced set of debit/credit lines
//! - [`LedgerEntry`] — an immutable, hash-chained record in the ledger

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

use crate::validation::LedgerError;

/// Fixed-point monetary value. Stored in the smallest unit of the currency.
/// For USD (precision=2): 1.00 = 100. For IDR (precision=0): 1000 = 1000.
pub type Balance = i128;

/// Scale factor for exchange rates (10⁶ = 6 decimal places of precision).
///
/// Exchange rates are stored as `rate × RATE_SCALE` to avoid floating-point math.
///
/// # Formula
///
/// `to_amount = from_amount × exchange_rate / RATE_SCALE`
///
/// # Example
///
/// 1 USD = 15,700 IDR. In smallest units: 1 cent = 157 IDR.
/// Rate = `157 × 1_000_000 = 157_000_000`.
pub const RATE_SCALE: i128 = 1_000_000;

/// Unique identifier for an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub u64);

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ACC-{:04}", self.0)
    }
}

/// Currency metadata. All accounts sharing a currency must use the same precision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Currency {
    /// ISO 4217 code (e.g. "USD", "IDR", "EUR").
    pub code: String,
    /// Number of decimal places. USD=2, IDR=0, BTC=8.
    pub precision: u8,
}

impl Currency {
    /// Create a new currency with an ISO 4217 code and decimal precision.
    ///
    /// The code is automatically uppercased.
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::Currency;
    ///
    /// let btc = Currency::new("BTC", 8);
    /// assert_eq!(btc.code, "BTC");
    /// assert_eq!(btc.precision, 8);
    /// ```
    pub fn new(code: &str, precision: u8) -> Self {
        Self { code: code.to_uppercase(), precision }
    }

    /// US Dollar (precision = 2).
    pub fn usd() -> Self { Self::new("USD", 2) }
    /// Indonesian Rupiah (precision = 0).
    pub fn idr() -> Self { Self::new("IDR", 0) }
    /// Euro (precision = 2).
    pub fn eur() -> Self { Self::new("EUR", 2) }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)
    }
}

/// Exchange rate metadata for cross-currency transactions.
///
/// Stored in the hash chain as part of the tamper-evident record.
/// The `rate` field uses scaled integer arithmetic — see [`RATE_SCALE`].
///
/// # Formula
///
/// `to_amount = from_amount × rate / RATE_SCALE`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// The exchange rate, scaled by [`RATE_SCALE`].
    ///
    /// Example: 1 USD cent = 157 IDR → `rate = 157 × RATE_SCALE = 157_000_000`.
    pub rate: Balance,
    /// Source currency code (e.g. "USD").
    pub from_currency: String,
    /// Target currency code (e.g. "IDR").
    pub to_currency: String,
}

/// Classification of an account within the chart of accounts.
///
/// The account type determines debit/credit behavior:
/// - **Debit-normal** (increase on debit): [`Asset`](Self::Asset), [`Expense`](Self::Expense)
/// - **Credit-normal** (increase on credit): [`Liability`](Self::Liability), [`Equity`](Self::Equity), [`Revenue`](Self::Revenue)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    /// Debit-normal. Resources owned (cash, receivables, equipment).
    Asset,
    /// Credit-normal. Obligations owed (payables, loans).
    Liability,
    /// Credit-normal. Owner's residual interest (capital, retained earnings).
    Equity,
    /// Credit-normal. Income earned (sales, interest income).
    Revenue,
    /// Debit-normal. Costs incurred (rent, salaries, utilities).
    Expense,
}

impl fmt::Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asset => write!(f, "Asset"),
            Self::Liability => write!(f, "Liability"),
            Self::Equity => write!(f, "Equity"),
            Self::Revenue => write!(f, "Revenue"),
            Self::Expense => write!(f, "Expense"),
        }
    }
}

/// A named account with a running balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub code: String,
    pub account_type: AccountType,
    pub currency: Currency,
    pub balance: Balance,
    pub active: bool,
}

impl Account {
    /// Apply a debit to this account.
    /// Assets and Expenses increase on debit; others decrease.
    pub fn apply_debit(&mut self, amount: Balance) {
        match self.account_type {
            AccountType::Asset | AccountType::Expense => self.balance += amount,
            _ => self.balance -= amount,
        }
    }

    /// Apply a credit to this account.
    /// Liabilities, Equity, and Revenue increase on credit; others decrease.
    pub fn apply_credit(&mut self, amount: Balance) {
        match self.account_type {
            AccountType::Liability | AccountType::Equity | AccountType::Revenue => {
                self.balance += amount;
            }
            _ => self.balance -= amount,
        }
    }

    /// Returns the signed balance for trial balance computation.
    /// Debit-normal accounts return positive; credit-normal return negative.
    pub fn signed_balance(&self) -> Balance {
        match self.account_type {
            AccountType::Asset | AccountType::Expense => self.balance,
            _ => -self.balance,
        }
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.active { "" } else { " [INACTIVE]" };
        write!(f, "[{}] {} — {} ({}){}", self.code, self.name, self.account_type, self.currency, status)
    }
}

/// A single debit or credit line within a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLine {
    pub account_id: AccountId,
    pub debit: Balance,
    pub credit: Balance,
}

/// A balanced set of debit and credit lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub description: String,
    pub lines: Vec<TransactionLine>,
    pub total_debit: Balance,
    pub total_credit: Balance,
    /// External idempotency key to prevent double-processing.
    /// If `Some`, the ledger rejects any transaction with a duplicate key.
    pub idempotency_key: Option<String>,
    /// Exchange rate metadata for cross-currency transactions.
    /// Present only on entries created via [`crate::Ledger::record_exchange_full`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange_rate: Option<ExchangeRate>,
}

impl Transaction {
    /// Create a balanced transaction without an idempotency key.
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::EmptyTransaction`] if both sides are empty,
    /// [`LedgerError::InvalidAmount`] if any amount ≤ 0, or
    /// [`LedgerError::Unbalanced`] if total debits ≠ total credits.
    pub fn new(
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
    ) -> Result<Self, LedgerError> {
        Self::new_with_key(description, debits, credits, None)
    }

    /// Create a balanced transaction with an optional idempotency key.
    ///
    /// The idempotency key is included in the SHA-256 hash computation,
    /// making it part of the tamper-evident record.
    ///
    /// # Errors
    ///
    /// Same as [`Transaction::new`].
    pub fn new_with_key(
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
        idempotency_key: Option<&str>,
    ) -> Result<Self, LedgerError> {
        if debits.is_empty() && credits.is_empty() {
            return Err(LedgerError::EmptyTransaction);
        }

        for &(_, amount) in debits.iter().chain(credits.iter()) {
            if amount <= 0 {
                return Err(LedgerError::InvalidAmount(amount));
            }
        }

        let total_debit: Balance = debits.iter().map(|&(_, a)| a).sum();
        let total_credit: Balance = credits.iter().map(|&(_, a)| a).sum();

        if total_debit != total_credit {
            return Err(LedgerError::Unbalanced {
                debit: total_debit,
                credit: total_credit,
            });
        }

        let mut lines = Vec::with_capacity(debits.len() + credits.len());
        for &(account_id, amount) in debits {
            lines.push(TransactionLine {
                account_id,
                debit: amount,
                credit: 0,
            });
        }
        for &(account_id, amount) in credits {
            lines.push(TransactionLine {
                account_id,
                debit: 0,
                credit: amount,
            });
        }

        Ok(Self {
            description: description.to_string(),
            lines,
            total_debit,
            total_credit,
            idempotency_key: idempotency_key.map(str::to_string),
            exchange_rate: None,
        })
    }

    /// Create a cross-currency exchange transaction.
    ///
    /// Unlike [`Transaction::new`], this does **not** require total debits = total credits
    /// (the amounts are in different currencies). Instead, it validates that
    /// `to_amount ≈ from_amount × rate / RATE_SCALE` within ±1 unit tolerance for rounding.
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::InvalidAmount`] if either amount ≤ 0,
    /// [`LedgerError::InvalidExchangeRate`] if the rate ≤ 0, or
    /// [`LedgerError::ExchangeRateMismatch`] if `to_amount` doesn't match the rate.
    pub fn new_exchange(
        description: &str,
        from_account: AccountId,
        from_amount: Balance,
        to_account: AccountId,
        to_amount: Balance,
        exchange_rate: ExchangeRate,
        idempotency_key: Option<&str>,
    ) -> Result<Self, LedgerError> {
        if from_amount <= 0 {
            return Err(LedgerError::InvalidAmount(from_amount));
        }
        if to_amount <= 0 {
            return Err(LedgerError::InvalidAmount(to_amount));
        }
        if exchange_rate.rate <= 0 {
            return Err(LedgerError::InvalidExchangeRate(exchange_rate.rate));
        }

        // Validate: to_amount ≈ from_amount × rate / RATE_SCALE (±1 tolerance for rounding)
        let expected_to = from_amount * exchange_rate.rate / RATE_SCALE;
        let diff = (to_amount - expected_to).abs();
        if diff > 1 {
            return Err(LedgerError::ExchangeRateMismatch {
                expected: expected_to,
                actual: to_amount,
            });
        }

        let lines = vec![
            TransactionLine {
                account_id: to_account,
                debit: to_amount,
                credit: 0,
            },
            TransactionLine {
                account_id: from_account,
                debit: 0,
                credit: from_amount,
            },
        ];

        Ok(Self {
            description: description.to_string(),
            lines,
            total_debit: to_amount,
            total_credit: from_amount,
            idempotency_key: idempotency_key.map(str::to_string),
            exchange_rate: Some(exchange_rate),
        })
    }
}

/// An immutable, hash-chained ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: u64,
    pub transaction: Transaction,
    pub prev_hash: String,
    pub hash: String,
    pub timestamp: u64,
}

impl LedgerEntry {
    /// Create a new entry with an explicit timestamp.
    pub fn new(id: u64, transaction: Transaction, prev_hash: &str, timestamp: u64) -> Self {
        let hash = Self::compute_hash(id, &transaction, prev_hash, timestamp);
        Self {
            id,
            transaction,
            prev_hash: prev_hash.to_string(),
            hash,
            timestamp,
        }
    }

    /// Compute the SHA-256 hash for an entry given its components.
    ///
    /// The hash includes: entry ID, previous hash, description, totals,
    /// all transaction lines, the idempotency key (if present), and the timestamp.
    /// This deterministic computation enables chain verification.
    pub fn compute_hash(
        id: u64,
        transaction: &Transaction,
        prev_hash: &str,
        timestamp: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(id.to_le_bytes());
        hasher.update(prev_hash.as_bytes());
        hasher.update(transaction.description.as_bytes());
        hasher.update(transaction.total_debit.to_le_bytes());
        hasher.update(transaction.total_credit.to_le_bytes());
        for line in &transaction.lines {
            hasher.update(line.account_id.0.to_le_bytes());
            hasher.update(line.debit.to_le_bytes());
            hasher.update(line.credit.to_le_bytes());
        }
        if let Some(ref key) = transaction.idempotency_key {
            hasher.update(key.as_bytes());
        }
        if let Some(ref xr) = transaction.exchange_rate {
            hasher.update(xr.rate.to_le_bytes());
            hasher.update(xr.from_currency.as_bytes());
            hasher.update(xr.to_currency.as_bytes());
        }
        hasher.update(timestamp.to_le_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify that this entry's stored hash matches a fresh computation.
    ///
    /// Returns `false` if any field has been tampered with since creation.
    pub fn verify(&self) -> bool {
        let expected = Self::compute_hash(
            self.id,
            &self.transaction,
            &self.prev_hash,
            self.timestamp,
        );
        self.hash == expected
    }
}

/// Returns the current UNIX timestamp in seconds.
pub(crate) fn current_timestamp() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
}
