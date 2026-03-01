//! # Kromia Ledger
//!
//! A deterministic, immutable, and cryptographically chained financial ledger engine.
//!
//! ## Design Principles
//! - **Fixed-point arithmetic**: All monetary values use `i128` with 2-decimal precision
//! - **Double-entry bookkeeping**: Every transaction must satisfy Σ Debit = Σ Credit
//! - **Cryptographic chaining**: SHA-256 hash chain ensures tamper-evident history
//! - **Atomic transactions**: All-or-nothing mutation — no partial state corruption
//! - **Zero floating-point**: Deterministic across native and WASM targets

pub mod types;
pub mod validation;
pub mod chain;
pub mod reconcile;
pub mod format;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use types::{AccountId, AccountType, Balance, Currency, LedgerEntry, Transaction, Account};
pub use validation::LedgerError;
pub use chain::HashChain;
pub use reconcile::{ReconcileRecord, ReconcileResult, ReconcileStatus, reconcile};
pub use format::{format_balance, format_balance_with_currency, parse_balance};

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The core ledger engine.
///
/// All mutations are atomic — if any validation fails, the ledger state
/// remains unchanged. The ledger can be serialized to JSON for persistence
/// and restored with full hash-chain verification.
#[derive(Debug, Serialize, Deserialize)]
pub struct Ledger {
    accounts: HashMap<AccountId, Account>,
    entries: Vec<LedgerEntry>,
    chain: HashChain,
    idempotency_keys: HashSet<String>,
    next_account_id: u64,
    next_entry_id: u64,
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            entries: Vec::new(),
            chain: HashChain::new(),
            idempotency_keys: HashSet::new(),
            next_account_id: 1,
            next_entry_id: 1,
        }
    }

    // ── Account Management ──────────────────────────────────────────

    /// Create a new account with a unique code and an assigned currency.
    pub fn create_account(
        &mut self,
        name: &str,
        code: &str,
        account_type: AccountType,
        currency: Currency,
    ) -> Result<AccountId, LedgerError> {
        if self.accounts.values().any(|a| a.code == code) {
            return Err(LedgerError::DuplicateAccountCode(code.to_string()));
        }
        let id = AccountId(self.next_account_id);
        self.next_account_id += 1;
        self.accounts.insert(id, Account {
            id,
            name: name.to_string(),
            code: code.to_string(),
            account_type,
            currency,
            balance: 0,
            active: true,
        });
        Ok(id)
    }

    /// Soft-deactivate an account. Inactive accounts cannot participate in new transactions.
    pub fn deactivate_account(&mut self, id: AccountId) -> Result<(), LedgerError> {
        let account = self.accounts.get_mut(&id)
            .ok_or(LedgerError::AccountNotFound(id.0))?;
        account.active = false;
        Ok(())
    }

    pub fn get_account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }

    pub fn account_by_code(&self, code: &str) -> Option<&Account> {
        self.accounts.values().find(|a| a.code == code)
    }

    pub fn get_balance(&self, id: AccountId) -> Option<Balance> {
        self.accounts.get(&id).map(|a| a.balance)
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    // ── Transaction Recording (Atomic) ──────────────────────────────

    /// Record a transaction using the system clock as timestamp.
    pub fn record_transaction(
        &mut self,
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
    ) -> Result<u64, LedgerError> {
        self.record_transaction_full(description, debits, credits, types::current_timestamp(), None)
    }

    /// Record a transaction with an explicit timestamp (deterministic).
    pub fn record_transaction_at(
        &mut self,
        description: &str,
        debits: &[(AccountId, Balance)],
        credits: &[(AccountId, Balance)],
        timestamp: u64,
    ) -> Result<u64, LedgerError> {
        self.record_transaction_full(description, debits, credits, timestamp, None)
    }

    /// Record a transaction with explicit timestamp and idempotency key.
    ///
    /// This method is **atomic**: if any validation fails (unbalanced amounts,
    /// missing accounts, inactive accounts, currency mismatch, duplicate key),
    /// the ledger state is unchanged.
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
        let mut tx_currency: Option<&Currency> = None;
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

    // ── Queries ─────────────────────────────────────────────────────

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn find_entry(&self, id: u64) -> Option<&LedgerEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Returns all entries that involve the given account (debit or credit side).
    pub fn entries_for_account(&self, account_id: AccountId) -> Vec<&LedgerEntry> {
        self.entries.iter()
            .filter(|e| e.transaction.lines.iter().any(|l| l.account_id == account_id))
            .collect()
    }

    /// Returns entries within a timestamp range (inclusive).
    pub fn entries_in_range(&self, from_ts: u64, to_ts: u64) -> Vec<&LedgerEntry> {
        self.entries.iter()
            .filter(|e| e.timestamp >= from_ts && e.timestamp <= to_ts)
            .collect()
    }

    // ── Integrity ───────────────────────────────────────────────────

    pub fn verify_chain(&self) -> bool {
        self.chain.verify(&self.entries)
    }

    /// Trial balance: must be 0 if all transactions are balanced.
    pub fn trial_balance(&self) -> Balance {
        self.accounts.values().map(|a| a.signed_balance()).sum()
    }

    // ── Persistence ─────────────────────────────────────────────────

    /// Serialize the entire ledger to a JSON string.
    pub fn save_json(&self) -> Result<String, LedgerError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| LedgerError::Serialization(e.to_string()))
    }

    /// Restore a ledger from a JSON string and verify chain integrity.
    pub fn load_json(json: &str) -> Result<Self, LedgerError> {
        let mut ledger: Self = serde_json::from_str(json)
            .map_err(|e| LedgerError::Serialization(e.to_string()))?;
        if !ledger.verify_chain() {
            return Err(LedgerError::ChainBroken(0));
        }
        // Rebuild idempotency key index from loaded entries
        ledger.idempotency_keys = ledger.entries.iter()
            .filter_map(|e| e.transaction.idempotency_key.clone())
            .collect();
        Ok(ledger)
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

    fn usd() -> Currency { Currency::usd() }
    fn idr() -> Currency { Currency::idr() }

    fn setup_ledger() -> (Ledger, AccountId, AccountId) {
        let mut ledger = Ledger::new();
        let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
        let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();
        (ledger, cash, revenue)
    }

    #[test]
    fn balanced_transaction_succeeds() {
        let (mut ledger, cash, revenue) = setup_ledger();
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
        let (mut ledger, cash, revenue) = setup_ledger();
        let result = ledger.record_transaction(
            "Bad transaction",
            &[(cash, 100_00)],
            &[(revenue, 50_00)],
        );
        assert!(result.is_err());
        assert_eq!(ledger.entries().len(), 0);
    }

    #[test]
    fn atomic_transaction_no_partial_state() {
        let mut ledger = Ledger::new();
        let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
        let fake_id = AccountId(999);

        let result = ledger.record_transaction(
            "Should fail",
            &[(cash, 100_00)],
            &[(fake_id, 100_00)],
        );
        assert!(result.is_err());
        assert_eq!(ledger.get_balance(cash).unwrap(), 0);
        assert_eq!(ledger.entries().len(), 0);
    }

    #[test]
    fn inactive_account_rejected() {
        let (mut ledger, cash, revenue) = setup_ledger();
        ledger.deactivate_account(cash).unwrap();

        let result = ledger.record_transaction(
            "Should fail",
            &[(cash, 100_00)],
            &[(revenue, 100_00)],
        );
        assert!(matches!(result, Err(LedgerError::InactiveAccount(_))));
    }

    #[test]
    fn duplicate_account_code_rejected() {
        let mut ledger = Ledger::new();
        ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
        let dup = ledger.create_account("Cash 2", "1000", AccountType::Asset, usd());
        assert!(matches!(dup, Err(LedgerError::DuplicateAccountCode(_))));
    }

    #[test]
    fn deterministic_transaction_at() {
        let (mut l1, c1, r1) = setup_ledger();
        let (mut l2, c2, r2) = setup_ledger();

        l1.record_transaction_at("TX", &[(c1, 500_00)], &[(r1, 500_00)], 1_000_000).unwrap();
        l2.record_transaction_at("TX", &[(c2, 500_00)], &[(r2, 500_00)], 1_000_000).unwrap();

        assert_eq!(l1.entries()[0].hash, l2.entries()[0].hash);
    }

    #[test]
    fn chain_integrity_holds() {
        let (mut ledger, cash, revenue) = setup_ledger();
        for i in 0..10 {
            ledger.record_transaction_at(
                &format!("Entry {i}"),
                &[(cash, 10_00)],
                &[(revenue, 10_00)],
                1_000_000 + i,
            ).unwrap();
        }
        assert!(ledger.verify_chain());
        assert_eq!(ledger.entries().len(), 10);
    }

    #[test]
    fn query_entries_for_account() {
        let mut ledger = Ledger::new();
        let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
        let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();
        let expense = ledger.create_account("Rent", "5000", AccountType::Expense, usd()).unwrap();

        ledger.record_transaction("Sale", &[(cash, 100_00)], &[(revenue, 100_00)]).unwrap();
        ledger.record_transaction("Rent", &[(expense, 30_00)], &[(cash, 30_00)]).unwrap();

        assert_eq!(ledger.entries_for_account(cash).len(), 2);
        assert_eq!(ledger.entries_for_account(revenue).len(), 1);
        assert_eq!(ledger.entries_for_account(expense).len(), 1);
    }

    #[test]
    fn account_by_code_lookup() {
        let (ledger, _, _) = setup_ledger();
        let acc = ledger.account_by_code("1000").unwrap();
        assert_eq!(acc.name, "Cash");
        assert!(ledger.account_by_code("9999").is_none());
    }

    #[test]
    fn persistence_roundtrip() {
        let (mut ledger, cash, revenue) = setup_ledger();
        ledger.record_transaction_at("TX-1", &[(cash, 500_00)], &[(revenue, 500_00)], 100).unwrap();
        ledger.record_transaction_at("TX-2", &[(cash, 250_00)], &[(revenue, 250_00)], 200).unwrap();

        let json = ledger.save_json().unwrap();
        let restored = Ledger::load_json(&json).unwrap();

        assert!(restored.verify_chain());
        assert_eq!(restored.entries().len(), 2);
        assert_eq!(restored.trial_balance(), 0);
        assert_eq!(restored.get_balance(cash).unwrap(), 750_00);
    }

    #[test]
    fn tampered_json_detected() {
        let (mut ledger, cash, revenue) = setup_ledger();
        ledger.record_transaction_at("TX", &[(cash, 100_00)], &[(revenue, 100_00)], 100).unwrap();

        let json = ledger.save_json().unwrap();
        let tampered = json.replace("\"TX\"", "\"HACKED\"");
        let result = Ledger::load_json(&tampered);
        assert!(result.is_err());
    }

    // ── Currency Validation Tests ───────────────────────────────────

    #[test]
    fn currency_mismatch_rejected() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let revenue_idr = ledger.create_account("Revenue IDR", "4100", AccountType::Revenue, idr()).unwrap();

        let result = ledger.record_transaction(
            "Cross-currency should fail",
            &[(cash_usd, 100_00)],
            &[(revenue_idr, 100_00)],
        );
        assert!(matches!(result, Err(LedgerError::CurrencyMismatch { .. })));
        assert_eq!(ledger.entries().len(), 0);
    }

    #[test]
    fn same_currency_transaction_succeeds() {
        let mut ledger = Ledger::new();
        let kas = ledger.create_account("Kas", "1100", AccountType::Asset, idr()).unwrap();
        let pendapatan = ledger.create_account("Pendapatan", "4100", AccountType::Revenue, idr()).unwrap();

        let result = ledger.record_transaction(
            "Penjualan",
            &[(kas, 1_000_000)],
            &[(pendapatan, 1_000_000)],
        );
        assert!(result.is_ok());
        assert_eq!(ledger.get_balance(kas).unwrap(), 1_000_000);
    }

    #[test]
    fn account_stores_currency() {
        let mut ledger = Ledger::new();
        let id = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::new("JPY", 0)).unwrap();
        let acc = ledger.get_account(id).unwrap();
        assert_eq!(acc.currency.code, "JPY");
        assert_eq!(acc.currency.precision, 0);
    }

    // ── Idempotency Key Tests ───────────────────────────────────────

    #[test]
    fn idempotency_key_prevents_duplicate() {
        let (mut ledger, cash, revenue) = setup_ledger();

        let r1 = ledger.record_transaction_full(
            "Order #1",
            &[(cash, 100_00)],
            &[(revenue, 100_00)],
            1_000_000,
            Some("ORDER-001"),
        );
        assert!(r1.is_ok());

        // Same key again — must be rejected
        let r2 = ledger.record_transaction_full(
            "Order #1 retry",
            &[(cash, 100_00)],
            &[(revenue, 100_00)],
            1_000_001,
            Some("ORDER-001"),
        );
        assert!(matches!(r2, Err(LedgerError::DuplicateIdempotencyKey(_))));
        assert_eq!(ledger.entries().len(), 1); // only first one recorded
    }

    #[test]
    fn different_idempotency_keys_both_succeed() {
        let (mut ledger, cash, revenue) = setup_ledger();

        ledger.record_transaction_full(
            "Order A", &[(cash, 50_00)], &[(revenue, 50_00)], 100, Some("KEY-A"),
        ).unwrap();
        ledger.record_transaction_full(
            "Order B", &[(cash, 50_00)], &[(revenue, 50_00)], 200, Some("KEY-B"),
        ).unwrap();

        assert_eq!(ledger.entries().len(), 2);
    }

    #[test]
    fn no_idempotency_key_allows_duplicates() {
        let (mut ledger, cash, revenue) = setup_ledger();

        // Without idempotency key, identical transactions are allowed
        ledger.record_transaction("TX", &[(cash, 100_00)], &[(revenue, 100_00)]).unwrap();
        ledger.record_transaction("TX", &[(cash, 100_00)], &[(revenue, 100_00)]).unwrap();
        assert_eq!(ledger.entries().len(), 2);
    }

    #[test]
    fn idempotency_key_survives_persistence() {
        let (mut ledger, cash, revenue) = setup_ledger();
        ledger.record_transaction_full(
            "TX", &[(cash, 100_00)], &[(revenue, 100_00)], 100, Some("PERSIST-KEY"),
        ).unwrap();

        let json = ledger.save_json().unwrap();
        let mut restored = Ledger::load_json(&json).unwrap();

        // The key must still be tracked after load
        let dup = restored.record_transaction_full(
            "TX again", &[(cash, 100_00)], &[(revenue, 100_00)], 200, Some("PERSIST-KEY"),
        );
        assert!(matches!(dup, Err(LedgerError::DuplicateIdempotencyKey(_))));
    }
}
