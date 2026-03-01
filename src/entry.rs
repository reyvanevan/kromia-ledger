//! Ledger entry and hash computation.
//!
//! A [`LedgerEntry`] wraps a [`Transaction`] together with its SHA-256 hash
//! and a back-pointer to the previous hash, forming a tamper-evident chain.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::audit::AuditMeta;
use crate::transaction::Transaction;

/// An immutable, hash-chained ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: u64,
    pub transaction: Transaction,
    pub prev_hash: String,
    pub hash: String,
    pub timestamp: u64,
    /// Audit trail metadata (who performed the action and from where).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditMeta>,
}

impl LedgerEntry {
    /// Create a new entry with an explicit timestamp and optional audit metadata.
    pub fn new(id: u64, transaction: Transaction, prev_hash: &str, timestamp: u64, audit: Option<AuditMeta>) -> Self {
        let hash = Self::compute_hash(id, &transaction, prev_hash, timestamp, audit.as_ref());
        Self {
            id,
            transaction,
            prev_hash: prev_hash.to_string(),
            hash,
            timestamp,
            audit,
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
        audit: Option<&AuditMeta>,
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
        if let Some(audit) = audit {
            hasher.update(audit.actor.as_bytes());
            if let Some(ref source) = audit.source {
                hasher.update(source.as_bytes());
            }
            if let Some(ref notes) = audit.notes {
                hasher.update(notes.as_bytes());
            }
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
            self.audit.as_ref(),
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
