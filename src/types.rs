use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::validation::LedgerError;

/// Fixed-point monetary value. 1.00 = 100 internal units.
pub type Balance = i128;

/// Unique identifier for an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub u64);

/// Classification of an account within the chart of accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

/// A named account with a running balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub account_type: AccountType,
    pub balance: Balance,
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
}

impl Transaction {
    pub fn new(
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
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
    pub fn new(id: u64, transaction: Transaction, prev_hash: &str) -> Self {
        let timestamp = current_timestamp();
        let hash = Self::compute_hash(id, &transaction, prev_hash, timestamp);
        Self {
            id,
            transaction,
            prev_hash: prev_hash.to_string(),
            hash,
            timestamp,
        }
    }

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
        hasher.update(timestamp.to_le_bytes());
        hex::encode(hasher.finalize())
    }

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

fn current_timestamp() -> u64 {
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
