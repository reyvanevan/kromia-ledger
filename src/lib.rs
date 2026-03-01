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

pub use types::{AccountId, AccountType, Balance, Currency, ExchangeRate, LedgerEntry, Transaction, Account, RATE_SCALE};
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
    ///
    /// # Arguments
    ///
    /// * `name` — Human-readable account name (e.g. "Cash", "Pendapatan")
    /// * `code` — Unique chart-of-accounts code (e.g. "1000", "4100")
    /// * `account_type` — Classification ([`AccountType`])
    /// * `currency` — Currency metadata for this account ([`Currency`])
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::DuplicateAccountCode`] if `code` is already in use.
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::{Ledger, AccountType, Currency};
    ///
    /// let mut ledger = Ledger::new();
    /// let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    /// assert_eq!(ledger.get_account(cash).unwrap().name, "Cash");
    /// ```
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

    /// Look up an account by its [`AccountId`]. Returns `None` if not found.
    pub fn get_account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }

    /// Look up an account by its chart-of-accounts code (e.g. `"1000"`).
    /// Returns `None` if no account has that code.
    pub fn account_by_code(&self, code: &str) -> Option<&Account> {
        self.accounts.values().find(|a| a.code == code)
    }

    /// Get the current balance of an account. Returns `None` if the account doesn't exist.
    ///
    /// The balance is in the smallest currency unit (e.g. cents for USD).
    pub fn get_balance(&self, id: AccountId) -> Option<Balance> {
        self.accounts.get(&id).map(|a| a.balance)
    }

    /// Iterate over all accounts in the ledger.
    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    // ── Transaction Recording (Atomic) ──────────────────────────────

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
        self.record_transaction_full(description, debits, credits, types::current_timestamp(), None)
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
    // ── Currency Exchange (Atomic) ────────────────────────────────────────

    /// Record a cross-currency exchange using the system clock.
    ///
    /// Convenience wrapper around [`record_exchange_full`](Self::record_exchange_full).
    pub fn record_exchange(
        &mut self,
        description: &str,
        from_account: AccountId,
        from_amount: Balance,
        to_account: AccountId,
        to_amount: Balance,
        exchange_rate: Balance,
    ) -> Result<u64, LedgerError> {
        self.record_exchange_full(
            description, from_account, from_amount, to_account, to_amount,
            exchange_rate, types::current_timestamp(), None,
        )
    }

    /// Record a cross-currency exchange with an explicit timestamp (deterministic).
    ///
    /// Convenience wrapper around [`record_exchange_full`](Self::record_exchange_full).
    #[allow(clippy::too_many_arguments)]
    pub fn record_exchange_at(
        &mut self,
        description: &str,
        from_account: AccountId,
        from_amount: Balance,
        to_account: AccountId,
        to_amount: Balance,
        exchange_rate: Balance,
        timestamp: u64,
    ) -> Result<u64, LedgerError> {
        self.record_exchange_full(
            description, from_account, from_amount, to_account, to_amount,
            exchange_rate, timestamp, None,
        )
    }

    /// Record a cross-currency exchange with explicit timestamp and optional idempotency key.
    ///
    /// This method allows two accounts with **different currencies** to transact.
    /// The `exchange_rate` uses scaled integer math:
    /// `to_amount = from_amount × exchange_rate / RATE_SCALE` (±1 tolerance for rounding).
    ///
    /// This method is **atomic**: if any validation fails, the ledger state is unchanged.
    ///
    /// # Arguments
    ///
    /// * `description` — Human-readable description
    /// * `from_account` — Account money leaves (credited in source currency)
    /// * `from_amount` — Amount in the source currency's smallest unit
    /// * `to_account` — Account money enters (debited in target currency)
    /// * `to_amount` — Amount in the target currency's smallest unit
    /// * `exchange_rate` — Scaled rate (see [`RATE_SCALE`])
    /// * `timestamp` — UNIX timestamp in seconds
    /// * `idempotency_key` — Optional external key to prevent double-processing
    ///
    /// # Errors
    ///
    /// | Error | Cause |
    /// |---|---|
    /// | [`LedgerError::InvalidAmount`] | Either amount ≤ 0 |
    /// | [`LedgerError::InvalidExchangeRate`] | Rate ≤ 0 |
    /// | [`LedgerError::ExchangeRateMismatch`] | `to_amount` doesn't match rate math |
    /// | [`LedgerError::AccountNotFound`] | Unknown account ID |
    /// | [`LedgerError::InactiveAccount`] | Deactivated account |
    /// | [`LedgerError::DuplicateIdempotencyKey`] | Key was already used |
    #[allow(clippy::too_many_arguments)]
    pub fn record_exchange_full(
        &mut self,
        description: &str,
        from_account: AccountId,
        from_amount: Balance,
        to_account: AccountId,
        to_amount: Balance,
        exchange_rate: Balance,
        timestamp: u64,
        idempotency_key: Option<&str>,
    ) -> Result<u64, LedgerError> {
        // Phase 0: Idempotency check
        if let Some(key) = idempotency_key
            && self.idempotency_keys.contains(key)
        {
            return Err(LedgerError::DuplicateIdempotencyKey(key.to_string()));
        }

        // Phase 1: Validate both accounts exist and are active
        let from_acc = self.accounts.get(&from_account)
            .ok_or(LedgerError::AccountNotFound(from_account.0))?;
        if !from_acc.active {
            return Err(LedgerError::InactiveAccount(from_account.0));
        }
        let from_currency = from_acc.currency.code.clone();

        let to_acc = self.accounts.get(&to_account)
            .ok_or(LedgerError::AccountNotFound(to_account.0))?;
        if !to_acc.active {
            return Err(LedgerError::InactiveAccount(to_account.0));
        }
        let to_currency = to_acc.currency.code.clone();

        // Phase 2: Build exchange metadata and validate rate math
        let rate_meta = types::ExchangeRate {
            rate: exchange_rate,
            from_currency,
            to_currency,
        };
        let transaction = Transaction::new_exchange(
            description, from_account, from_amount, to_account, to_amount,
            rate_meta, idempotency_key,
        )?;

        // Phase 3: All checks passed — mutate state (cannot fail from here)
        if let Some(key) = idempotency_key {
            self.idempotency_keys.insert(key.to_string());
        }

        let prev_hash = self.chain.last_hash();
        let entry_id = self.next_entry_id;
        self.next_entry_id += 1;

        let entry = LedgerEntry::new(entry_id, transaction, &prev_hash, timestamp);
        self.chain.append(&entry);

        self.accounts.get_mut(&from_account).unwrap().apply_credit(from_amount);
        self.accounts.get_mut(&to_account).unwrap().apply_debit(to_amount);

        self.entries.push(entry);
        Ok(entry_id)
    }
    // ── Queries ─────────────────────────────────────────────────────

    /// Returns a slice of all ledger entries in chronological order.
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Find a single entry by its numeric ID. Returns `None` if not found.
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

    /// Verify the integrity of the entire hash chain.
    ///
    /// Returns `true` if every entry's hash is consistent with its content
    /// and its predecessor. Returns `false` if any entry has been tampered with.
    pub fn verify_chain(&self) -> bool {
        self.chain.verify(&self.entries)
    }

    /// Compute the trial balance across all accounts.
    ///
    /// For single-currency ledgers, this returns exactly `0` if all transactions
    /// are balanced. For multi-currency ledgers (with exchange transactions),
    /// this value may be non-zero — use per-currency reporting instead.
    pub fn trial_balance(&self) -> Balance {
        self.accounts.values().map(|a| a.signed_balance()).sum()
    }

    // ── Persistence ─────────────────────────────────────────────────

    /// Serialize the entire ledger to a pretty-printed JSON string.
    ///
    /// The output includes all accounts, entries, and the hash chain.
    /// Use [`load_json`](Self::load_json) to restore.
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::Serialization`] if serialization fails.
    pub fn save_json(&self) -> Result<String, LedgerError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| LedgerError::Serialization(e.to_string()))
    }

    /// Restore a ledger from a JSON string with automatic chain verification.
    ///
    /// This method verifies the hash chain immediately after deserialization.
    /// If the chain is broken (any entry was tampered with), it returns an error.
    /// Idempotency keys are automatically rebuilt from the loaded entries.
    ///
    /// # Errors
    ///
    /// Returns [`LedgerError::Serialization`] if JSON parsing fails, or
    /// [`LedgerError::ChainBroken`] if the hash chain is invalid.
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

    // ── Currency Exchange Tests ───────────────────────────────────────

    // 1 USD cent = 157 IDR → 1 USD = 15,700 IDR
    fn rate_usd_idr() -> Balance { 157 * RATE_SCALE }

    #[test]
    fn exchange_basic_succeeds() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();
        let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();

        // Seed $10.00 into Cash USD
        ledger.record_transaction_at("Deposit", &[(cash_usd, 1000)], &[(revenue, 1000)], 100).unwrap();

        // Exchange $5.00 (500 cents) → Rp 78,500
        // 500 × 157_000_000 / 1_000_000 = 78,500 ✓
        let result = ledger.record_exchange_at(
            "USD to IDR", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 200,
        );
        assert!(result.is_ok());
        assert_eq!(ledger.get_balance(cash_usd).unwrap(), 500);  // 1000 - 500
        assert_eq!(ledger.get_balance(cash_idr).unwrap(), 78_500);
        assert!(ledger.verify_chain());
        assert_eq!(ledger.entries().len(), 2);
    }

    #[test]
    fn exchange_rate_mismatch_rejected() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

        // to_amount doesn't match: 500 × 157M / 1M = 78,500, not 99,999
        let result = ledger.record_exchange_at(
            "Bad exchange", cash_usd, 500, cash_idr, 99_999, rate_usd_idr(), 100,
        );
        assert!(matches!(result, Err(LedgerError::ExchangeRateMismatch { .. })));
        assert_eq!(ledger.entries().len(), 0);
    }

    #[test]
    fn exchange_invalid_rate_rejected() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

        let result = ledger.record_exchange_at(
            "Zero rate", cash_usd, 500, cash_idr, 78_500, 0, 100,
        );
        assert!(matches!(result, Err(LedgerError::InvalidExchangeRate(_))));
    }

    #[test]
    fn exchange_rounding_tolerance() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

        // Exact: 500 × 157_000_000 / 1_000_000 = 78,500
        // Off by 1 (within tolerance): 78,501
        let result = ledger.record_exchange_at(
            "Rounded", cash_usd, 500, cash_idr, 78_501, rate_usd_idr(), 100,
        );
        assert!(result.is_ok()); // ±1 tolerance

        // Off by 2 (beyond tolerance): 78,502
        let mut ledger2 = Ledger::new();
        let cu = ledger2.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let ci = ledger2.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();
        let result2 = ledger2.record_exchange_at(
            "Too far off", cu, 500, ci, 78_502, rate_usd_idr(), 100,
        );
        assert!(matches!(result2, Err(LedgerError::ExchangeRateMismatch { .. })));
    }

    #[test]
    fn exchange_deterministic_hash() {
        let mut l1 = Ledger::new();
        let u1 = l1.create_account("USD", "1100", AccountType::Asset, usd()).unwrap();
        let i1 = l1.create_account("IDR", "1200", AccountType::Asset, idr()).unwrap();

        let mut l2 = Ledger::new();
        let u2 = l2.create_account("USD", "1100", AccountType::Asset, usd()).unwrap();
        let i2 = l2.create_account("IDR", "1200", AccountType::Asset, idr()).unwrap();

        l1.record_exchange_at("XCH", u1, 500, i1, 78_500, rate_usd_idr(), 1_000_000).unwrap();
        l2.record_exchange_at("XCH", u2, 500, i2, 78_500, rate_usd_idr(), 1_000_000).unwrap();

        assert_eq!(l1.entries()[0].hash, l2.entries()[0].hash);
    }

    #[test]
    fn exchange_atomicity() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let fake_id = AccountId(999);

        // to_account doesn't exist — must fail atomically
        let result = ledger.record_exchange_at(
            "Fail", cash_usd, 500, fake_id, 78_500, rate_usd_idr(), 100,
        );
        assert!(result.is_err());
        assert_eq!(ledger.get_balance(cash_usd).unwrap(), 0);
        assert_eq!(ledger.entries().len(), 0);
    }

    #[test]
    fn exchange_inactive_account_rejected() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();
        ledger.deactivate_account(cash_usd).unwrap();

        let result = ledger.record_exchange_at(
            "Fail", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100,
        );
        assert!(matches!(result, Err(LedgerError::InactiveAccount(_))));
    }

    #[test]
    fn exchange_idempotency_key() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

        ledger.record_exchange_full(
            "XCH", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100, Some("XCH-001"),
        ).unwrap();

        // Same key — must be rejected
        let dup = ledger.record_exchange_full(
            "XCH retry", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 200, Some("XCH-001"),
        );
        assert!(matches!(dup, Err(LedgerError::DuplicateIdempotencyKey(_))));
        assert_eq!(ledger.entries().len(), 1);
    }

    #[test]
    fn exchange_persistence_roundtrip() {
        let mut ledger = Ledger::new();
        let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
        let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

        ledger.record_exchange_at(
            "USD to IDR", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100,
        ).unwrap();

        let json = ledger.save_json().unwrap();
        let restored = Ledger::load_json(&json).unwrap();

        assert!(restored.verify_chain());
        assert_eq!(restored.entries().len(), 1);
        assert_eq!(restored.get_balance(cash_usd).unwrap(), -500); // credited 500 from 0
        assert_eq!(restored.get_balance(cash_idr).unwrap(), 78_500);

        // Verify exchange rate is preserved
        let entry = &restored.entries()[0];
        let xr = entry.transaction.exchange_rate.as_ref().unwrap();
        assert_eq!(xr.rate, rate_usd_idr());
        assert_eq!(xr.from_currency, "USD");
        assert_eq!(xr.to_currency, "IDR");
    }
}
