use sha2::{Digest, Sha256};

use crate::types::LedgerEntry;

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Manages the SHA-256 hash chain for ledger entries.
pub struct HashChain {
    hashes: Vec<String>,
}

impl HashChain {
    pub fn new() -> Self {
        Self {
            hashes: vec![GENESIS_HASH.to_string()],
        }
    }

    /// Returns the most recent hash in the chain.
    pub fn last_hash(&self) -> String {
        self.hashes.last().cloned().unwrap_or_else(|| GENESIS_HASH.to_string())
    }

    /// Appends a new entry's hash to the chain.
    pub fn append(&mut self, entry: &LedgerEntry) {
        self.hashes.push(entry.hash.clone());
    }

    /// Verifies the entire chain against the provided entries.
    /// Each entry's `prev_hash` must match the preceding hash in the chain,
    /// and each entry's stored hash must match its recomputed hash.
    pub fn verify(&self, entries: &[LedgerEntry]) -> bool {
        if entries.is_empty() {
            return true;
        }

        // Chain must have genesis + one hash per entry
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

    /// Computes a standalone SHA-256 hash of arbitrary data.
    pub fn sha256(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    pub fn len(&self) -> usize {
        self.hashes.len().saturating_sub(1)
    }

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
        LedgerEntry::new(id, tx, prev_hash)
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

        // Tamper with the entry
        e2.transaction.description = "TAMPERED".to_string();

        assert!(!chain.verify(&[e1, e2]));
    }
}
