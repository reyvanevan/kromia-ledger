//! Cross-currency exchange recording.
//!
//! This module extends [`Ledger`] with methods for recording
//! multi-currency exchange transactions using scaled integer arithmetic.

use crate::account::{AccountId, Balance, ExchangeRate};
use crate::audit::AuditMeta;
use crate::entry::{self, LedgerEntry};
use crate::transaction::Transaction;
use crate::validation::LedgerError;
use crate::Ledger;

impl Ledger {
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
            exchange_rate, entry::current_timestamp(), None,
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
    /// * `exchange_rate` — Scaled rate (see [`RATE_SCALE`](crate::RATE_SCALE))
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
        self.record_exchange_impl(
            description, from_account, from_amount, to_account, to_amount,
            exchange_rate, timestamp, idempotency_key, None,
        )
    }

    /// Record a cross-currency exchange with audit trail metadata.
    ///
    /// This extends [`record_exchange_full`](Self::record_exchange_full) with
    /// an [`AuditMeta`] recording who performed the exchange and from where.
    /// The audit metadata is included in the SHA-256 hash, making it tamper-evident.
    ///
    /// # Errors
    ///
    /// Same as [`record_exchange_full`](Self::record_exchange_full).
    #[allow(clippy::too_many_arguments)]
    pub fn record_exchange_audited(
        &mut self,
        description: &str,
        from_account: AccountId,
        from_amount: Balance,
        to_account: AccountId,
        to_amount: Balance,
        exchange_rate: Balance,
        timestamp: u64,
        idempotency_key: Option<&str>,
        audit: AuditMeta,
    ) -> Result<u64, LedgerError> {
        self.record_exchange_impl(
            description, from_account, from_amount, to_account, to_amount,
            exchange_rate, timestamp, idempotency_key, Some(audit),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn record_exchange_impl(
        &mut self,
        description: &str,
        from_account: AccountId,
        from_amount: Balance,
        to_account: AccountId,
        to_amount: Balance,
        exchange_rate: Balance,
        timestamp: u64,
        idempotency_key: Option<&str>,
        audit: Option<AuditMeta>,
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
        let rate_meta = ExchangeRate {
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

        let entry = LedgerEntry::new(entry_id, transaction, &prev_hash, timestamp, audit);
        self.chain.append(&entry);

        self.accounts.get_mut(&from_account).unwrap().apply_credit(from_amount);
        self.accounts.get_mut(&to_account).unwrap().apply_debit(to_amount);

        self.entries.push(entry);
        Ok(entry_id)
    }
}
