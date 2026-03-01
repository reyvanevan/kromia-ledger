//! Ledger query methods.
//!
//! This module extends [`Ledger`] with read-only query methods
//! for inspecting entries and verifying integrity.

use crate::account::{AccountId, Balance};
use crate::entry::LedgerEntry;
use crate::Ledger;

impl Ledger {
    // ── Queries ─────────────────────────────────────────────────────

    /// Returns a slice of all ledger entries in chronological order.
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Find a single entry by its numeric ID. Returns `None` if not found.
    ///
    /// This is an O(1) lookup when entry IDs are sequential (the default).
    /// Falls back to O(n) scan if the index doesn't match.
    pub fn find_entry(&self, id: u64) -> Option<&LedgerEntry> {
        if id == 0 {
            return None;
        }
        // O(1) fast path: entry IDs are sequential starting from 1
        if let Some(entry) = self.entries.get((id - 1) as usize)
            && entry.id == id
        {
            return Some(entry);
        }
        // Fallback: linear scan (handles non-sequential IDs from external data)
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
    /// this value may be non-zero — use [`trial_balance_by_currency`](Self::trial_balance_by_currency) instead.
    pub fn trial_balance(&self) -> Balance {
        self.accounts.values().map(|a| a.signed_balance()).sum()
    }

    /// Compute the trial balance grouped by currency.
    ///
    /// Returns a map from currency code to the signed sum of all accounts
    /// in that currency. Each currency should independently sum to `0`
    /// if all transactions are balanced.
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::{Ledger, AccountType, Currency};
    ///
    /// let mut ledger = Ledger::new();
    /// let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    /// let rev  = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
    /// ledger.record_transaction("Sale", &[(cash, 500)], &[(rev, 500)]).unwrap();
    ///
    /// let tb = ledger.trial_balance_by_currency();
    /// assert_eq!(tb["USD"], 0);
    /// ```
    pub fn trial_balance_by_currency(&self) -> std::collections::BTreeMap<String, Balance> {
        let mut map = std::collections::BTreeMap::new();
        for account in self.accounts.values() {
            *map.entry(account.currency.code.clone()).or_insert(0) += account.signed_balance();
        }
        map
    }
}
