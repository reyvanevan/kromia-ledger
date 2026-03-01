//! # Kromia Ledger
//!
//! A deterministic, immutable, and cryptographically chained financial ledger engine.
//!
//! ## Design Principles
//! - **Fixed-point arithmetic**: All monetary values use `i128` with 2-decimal precision
//! - **Double-entry bookkeeping**: Every transaction must satisfy Σ Debit = Σ Credit
//! - **Cryptographic chaining**: SHA-256 hash chain ensures tamper-evident history
//! - **Zero floating-point**: Deterministic across native and WASM targets

pub mod types;
pub mod validation;
pub mod chain;
pub mod reconcile;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use types::{AccountId, AccountType, Balance, LedgerEntry, Transaction, Account};
pub use validation::LedgerError;
pub use chain::HashChain;
pub use reconcile::{ReconcileResult, ReconcileStatus};

use std::collections::HashMap;

pub struct Ledger {
    accounts: HashMap<AccountId, Account>,
    entries: Vec<LedgerEntry>,
    chain: HashChain,
    next_account_id: u64,
    next_entry_id: u64,
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            entries: Vec::new(),
            chain: HashChain::new(),
            next_account_id: 1,
            next_entry_id: 1,
        }
    }

    pub fn create_account(&mut self, name: &str, account_type: AccountType) -> AccountId {
        let id = AccountId(self.next_account_id);
        self.next_account_id += 1;
        self.accounts.insert(id, Account {
            id,
            name: name.to_string(),
            account_type,
            balance: 0,
        });
        id
    }

    pub fn get_account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }

    pub fn record_transaction(
        &mut self,
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
    ) -> Result<u64, LedgerError> {
        let transaction = Transaction::new(description, debits, credits)?;
        let prev_hash = self.chain.last_hash();
        let entry_id = self.next_entry_id;
        self.next_entry_id += 1;

        let entry = LedgerEntry::new(entry_id, transaction, &prev_hash);
        self.chain.append(&entry);

        for &(account_id, amount) in debits {
            let account = self.accounts.get_mut(&account_id)
                .ok_or(LedgerError::AccountNotFound(account_id.0))?;
            account.apply_debit(amount);
        }

        for &(account_id, amount) in credits {
            let account = self.accounts.get_mut(&account_id)
                .ok_or(LedgerError::AccountNotFound(account_id.0))?;
            account.apply_credit(amount);
        }

        self.entries.push(entry);
        Ok(entry_id)
    }

    pub fn verify_chain(&self) -> bool {
        self.chain.verify(&self.entries)
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    pub fn trial_balance(&self) -> Balance {
        self.accounts.values().map(|a| a.signed_balance()).sum()
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_transaction_succeeds() {
        let mut ledger = Ledger::new();
        let cash = ledger.create_account("Cash", AccountType::Asset);
        let revenue = ledger.create_account("Revenue", AccountType::Revenue);

        let result = ledger.record_transaction(
            "Payment received",
            &[(cash, 100_00)],
            &[(revenue, 100_00)],
        );
        assert!(result.is_ok());
        assert!(ledger.verify_chain());
        assert_eq!(ledger.trial_balance(), 0);
    }

    #[test]
    fn unbalanced_transaction_fails() {
        let mut ledger = Ledger::new();
        let cash = ledger.create_account("Cash", AccountType::Asset);
        let revenue = ledger.create_account("Revenue", AccountType::Revenue);

        let result = ledger.record_transaction(
            "Bad transaction",
            &[(cash, 100_00)],
            &[(revenue, 50_00)],
        );
        assert!(result.is_err());
    }

    #[test]
    fn chain_integrity_holds() {
        let mut ledger = Ledger::new();
        let cash = ledger.create_account("Cash", AccountType::Asset);
        let equity = ledger.create_account("Equity", AccountType::Equity);

        for i in 0..10 {
            ledger.record_transaction(
                &format!("Entry {i}"),
                &[(cash, 10_00)],
                &[(equity, 10_00)],
            ).unwrap();
        }

        assert!(ledger.verify_chain());
        assert_eq!(ledger.entries().len(), 10);
    }
}
