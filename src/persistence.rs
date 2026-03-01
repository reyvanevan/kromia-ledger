//! Ledger persistence (JSON serialization / deserialization).
//!
//! This module extends [`Ledger`] with methods for saving
//! to and loading from JSON, with automatic hash-chain verification on load.

use crate::validation::LedgerError;
use crate::Ledger;

impl Ledger {
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
        if let Some(broken_id) = ledger.chain.find_first_invalid(&ledger.entries) {
            return Err(LedgerError::ChainBroken(broken_id));
        }
        // Rebuild idempotency key index from loaded entries
        ledger.idempotency_keys = ledger.entries.iter()
            .filter_map(|e| e.transaction.idempotency_key.clone())
            .collect();
        Ok(ledger)
    }
}
