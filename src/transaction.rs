//! Transaction types and constructors.
//!
//! A [`Transaction`] groups one or more [`TransactionLine`]s that must balance
//! (total debits == total credits). Constructors validate this invariant.

use serde::{Deserialize, Serialize};

use crate::account::{AccountId, Balance, ExchangeRate, RATE_SCALE};
use crate::validation::LedgerError;

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

// ── Ledger transaction recording ─────────────────────────────────────

use crate::entry::{self, LedgerEntry};
use crate::Ledger;

impl Ledger {
    /// Record a transaction using the system clock as timestamp.
    ///
    /// This is the simplest way to record a transaction. For deterministic
    /// timestamps or idempotency keys, use [`record_transaction_at`](Self::record_transaction_at)
    /// or [`record_transaction_full`](Self::record_transaction_full).
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::Unbalanced`] if debits ≠ credits,
    /// [`LedgerError::AccountNotFound`] if any account ID is invalid,
    /// [`LedgerError::InactiveAccount`] if any account is deactivated, or
    /// [`LedgerError::CurrencyMismatch`] if accounts have different currencies.
    pub fn record_transaction(
        &mut self,
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
    ) -> Result<u64, LedgerError> {
        self.record_transaction_full(description, debits, credits, entry::current_timestamp(), None)
    }

    /// Record a transaction with an explicit timestamp (deterministic).
    ///
    /// Use this for reproducible tests or when the timestamp comes from an external source.
    /// Two ledgers that record the same data at the same timestamp produce identical hashes.
    ///
    /// # Errors
    ///
    /// Same as [`record_transaction`](Self::record_transaction).
    pub fn record_transaction_at(
        &mut self,
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
        timestamp: u64,
    ) -> Result<u64, LedgerError> {
        self.record_transaction_full(description, debits, credits, timestamp, None)
    }

    /// Record a transaction with explicit timestamp and optional idempotency key.
    ///
    /// This is the full-control method. It is **atomic**: if any validation
    /// fails (unbalanced amounts, missing accounts, inactive accounts,
    /// currency mismatch, duplicate idempotency key), the ledger state
    /// remains completely unchanged.
    ///
    /// # Arguments
    ///
    /// * `description` — Human-readable description of the transaction
    /// * `debits` — Slice of `(AccountId, amount)` pairs for the debit side
    /// * `credits` — Slice of `(AccountId, amount)` pairs for the credit side
    /// * `timestamp` — UNIX timestamp in seconds
    /// * `idempotency_key` — Optional external key to prevent double-processing
    ///
    /// # Errors
    ///
    /// | Error | Cause |
    /// |---|---|
    /// | [`LedgerError::DuplicateIdempotencyKey`] | Key was already used |
    /// | [`LedgerError::Unbalanced`] | Total debits ≠ total credits |
    /// | [`LedgerError::EmptyTransaction`] | No debit or credit lines |
    /// | [`LedgerError::InvalidAmount`] | Any amount ≤ 0 |
    /// | [`LedgerError::AccountNotFound`] | Unknown account ID |
    /// | [`LedgerError::InactiveAccount`] | Deactivated account |
    /// | [`LedgerError::CurrencyMismatch`] | Accounts use different currencies |
    pub fn record_transaction_full(
        &mut self,
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
        timestamp: u64,
        idempotency_key: Option<&str>,
    ) -> Result<u64, LedgerError> {
        // Phase 0: Check idempotency key uniqueness
        if let Some(key) = idempotency_key
            && self.idempotency_keys.contains(key)
        {
            return Err(LedgerError::DuplicateIdempotencyKey(key.to_string()));
        }

        // Phase 1: Validate transaction balance
        let transaction = Transaction::new_with_key(description, debits, credits, idempotency_key)?;

        // Phase 2: Validate ALL accounts exist, are active, and share the same currency
        let mut tx_currency: Option<&crate::account::Currency> = None;
        for &(account_id, _) in debits.iter().chain(credits.iter()) {
            let account = self.accounts.get(&account_id)
                .ok_or(LedgerError::AccountNotFound(account_id.0))?;
            if !account.active {
                return Err(LedgerError::InactiveAccount(account_id.0));
            }
            match tx_currency {
                None => tx_currency = Some(&account.currency),
                Some(expected) => {
                    if account.currency != *expected {
                        return Err(LedgerError::CurrencyMismatch {
                            expected: expected.code.clone(),
                            found: account.currency.code.clone(),
                            account_id: account_id.0,
                        });
                    }
                }
            }
        }

        // Phase 3: All checks passed — mutate state (cannot fail from here)
        if let Some(key) = idempotency_key {
            self.idempotency_keys.insert(key.to_string());
        }

        let prev_hash = self.chain.last_hash();
        let entry_id = self.next_entry_id;
        self.next_entry_id += 1;

        let entry = LedgerEntry::new(entry_id, transaction, &prev_hash, timestamp);
        self.chain.append(&entry);

        for &(account_id, amount) in debits {
            self.accounts.get_mut(&account_id).unwrap().apply_debit(amount);
        }
        for &(account_id, amount) in credits {
            self.accounts.get_mut(&account_id).unwrap().apply_credit(amount);
        }

        self.entries.push(entry);
        Ok(entry_id)
    }
}
