//! SHA-256 hash chain for tamper-evident ledger integrity.
//!
//! The [`HashChain`] maintains an ordered list of hashes — one per ledger entry,
//! plus a genesis hash (64 zero bytes). Each entry's hash is computed from its
//! content **and** the previous hash, forming an unbreakable chain. If any
//! historical entry is modified, every subsequent hash becomes invalid.
//!
//! This is the same principle behind blockchain, applied to a financial ledger.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::LedgerEntry;

/// The genesis hash — 64 hex zeros. This anchors the very first entry in the chain.
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Manages the SHA-256 hash chain for ledger entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashChain {
    hashes: Vec<String>,
}

impl HashChain {
    /// Create a new hash chain seeded with the genesis hash.
    pub fn new() -> Self {
        Self {
            hashes: vec![GENESIS_HASH.to_string()],
        }
    }

    /// Returns the most recent hash in the chain (used as `prev_hash` for the next entry).
    pub fn last_hash(&self) -> String {
        self.hashes.last().cloned().unwrap_or_else(|| GENESIS_HASH.to_string())
    }

    /// Append a ledger entry's hash to the chain.
    pub fn append(&mut self, entry: &LedgerEntry) {
        self.hashes.push(entry.hash.clone());
    }

    /// Verifies the entire chain against the provided entries.
    pub fn verify(&self, entries: &[LedgerEntry]) -> bool {
        if entries.is_empty() {
            return true;
        }

        if self.hashes.len() != entries.len() + 1 {
            return false;
        }

        for (i, entry) in entries.iter().enumerate() {
            let expected_prev = &self.hashes[i];
            if entry.prev_hash != *expected_prev {
                return false;
            }
            if !entry.verify() {
                return false;
            }
            if entry.hash != self.hashes[i + 1] {
                return false;
            }
        }

        true
    }

    /// Returns the entry ID of the first invalid entry, or `None` if the chain is intact.
    ///
    /// Unlike [`verify`](Self::verify), which returns a simple boolean, this method
    /// pinpoints exactly which entry broke the chain — useful for error reporting.
    pub fn find_first_invalid(&self, entries: &[LedgerEntry]) -> Option<u64> {
        if entries.is_empty() {
            return None;
        }
        if self.hashes.len() != entries.len() + 1 {
            return entries.first().map(|e| e.id);
        }
        for (i, entry) in entries.iter().enumerate() {
            let expected_prev = &self.hashes[i];
            if entry.prev_hash != *expected_prev
                || !entry.verify()
                || entry.hash != self.hashes[i + 1]
            {
                return Some(entry.id);
            }
        }
        None
    }

    /// Compute a standalone SHA-256 hash of arbitrary data, returned as a hex string.
    pub fn sha256(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Returns the number of entries in the chain (excludes the genesis hash).
    pub fn len(&self) -> usize {
        self.hashes.len().saturating_sub(1)
    }

    /// Returns `true` if no entries have been appended to the chain.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for HashChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AccountId, Transaction, LedgerEntry};

    fn make_entry(id: u64, prev_hash: &str) -> LedgerEntry {
        let tx = Transaction::new(
            &format!("tx-{id}"),
            &[(AccountId(1), 100)],
            &[(AccountId(2), 100)],
        ).unwrap();
        LedgerEntry::new(id, tx, prev_hash, 1_000_000 + id)
    }

    #[test]
    fn genesis_chain_is_valid() {
        let chain = HashChain::new();
        assert!(chain.verify(&[]));
        assert!(chain.is_empty());
    }

    #[test]
    fn chain_of_three_verifies() {
        let mut chain = HashChain::new();

        let e1 = make_entry(1, &chain.last_hash());
        chain.append(&e1);

        let e2 = make_entry(2, &chain.last_hash());
        chain.append(&e2);

        let e3 = make_entry(3, &chain.last_hash());
        chain.append(&e3);

        assert!(chain.verify(&[e1, e2, e3]));
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn tampered_entry_fails_verification() {
        let mut chain = HashChain::new();

        let e1 = make_entry(1, &chain.last_hash());
        chain.append(&e1);

        let mut e2 = make_entry(2, &chain.last_hash());
        chain.append(&e2);

        e2.transaction.description = "TAMPERED".to_string();

        assert!(!chain.verify(&[e1, e2]));
    }

    #[test]
    fn serialization_roundtrip() {
        let mut chain = HashChain::new();
        let e1 = make_entry(1, &chain.last_hash());
        chain.append(&e1);

        let json = serde_json::to_string(&chain).unwrap();
        let restored: HashChain = serde_json::from_str(&json).unwrap();
        assert!(restored.verify(&[e1]));
    }
}
