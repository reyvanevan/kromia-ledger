//! Account types and balance logic.
//!
//! This module defines the chart-of-accounts structures:
//!
//! - [`Balance`] — fixed-point i128 monetary value (no floating point)
//! - [`RATE_SCALE`] — scale factor for exchange rate integer arithmetic
//! - [`AccountId`] — unique numeric identifier for an account
//! - [`AccountType`] — classification (Asset, Liability, Equity, Revenue, Expense)
//! - [`Currency`] — ISO 4217 currency metadata with decimal precision
//! - [`ExchangeRate`] — cross-currency exchange rate metadata
//! - [`Account`] — a named account with a running balance

use serde::{Deserialize, Serialize};
use std::fmt;

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
