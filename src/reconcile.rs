//! O(n + m) dataset reconciliation engine.
//!
//! Compares an internal dataset against an external dataset (e.g. bank statement
//! vs general ledger) by matching on record IDs. The output classifies each
//! record as [`Matched`](ReconcileStatus::Matched),
//! [`AmountMismatch`](ReconcileStatus::AmountMismatch),
//! [`DateMismatch`](ReconcileStatus::DateMismatch),
//! [`InternalOnly`](ReconcileStatus::InternalOnly), or
//! [`ExternalOnly`](ReconcileStatus::ExternalOnly).
//!
//! # Example
//!
//! ```
//! use kromia_ledger::reconcile::{reconcile, ReconcileRecord, ReconcileStatus};
//!
//! let internal = vec![ReconcileRecord { id: "TX001".into(), amount: 10000, date: "2026-01-15".into() }];
//! let external = vec![ReconcileRecord { id: "TX001".into(), amount: 10000, date: "2026-01-15".into() }];
//!
//! let results = reconcile(&internal, &external);
//! assert_eq!(results[0].status, ReconcileStatus::Matched);
//! ```

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::types::Balance;

/// A record from either internal or external data source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileRecord {
    pub id: String,
    pub amount: Balance,
    pub date: String,
}

/// Status of a reconciliation match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconcileStatus {
    /// Records match on all fields.
    Matched,
    /// Amount mismatch between internal and external record.
    AmountMismatch { internal: Balance, external: Balance },
    /// Date mismatch between internal and external record.
    DateMismatch { internal: String, external: String },
    /// Multiple field mismatches.
    MultipleMismatch {
        amount: Option<(Balance, Balance)>,
        date: Option<(String, String)>,
    },
    /// Record exists only in the internal dataset.
    InternalOnly,
    /// Record exists only in the external dataset.
    ExternalOnly,
}

/// Result of reconciling a single record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResult {
    pub id: String,
    pub status: ReconcileStatus,
}

/// Reconciles two datasets (internal vs external) by matching on `id`.
/// Time complexity: O(n + m) where n, m are the sizes of the two datasets.
pub fn reconcile(
    internal: &[ReconcileRecord],
    external: &[ReconcileRecord],
) -> Vec<ReconcileResult> {
    let mut results = Vec::new();

    // Index external records by id for O(1) lookup
    let external_map: HashMap<&str, &ReconcileRecord> = external
        .iter()
        .map(|r| (r.id.as_str(), r))
        .collect();

    let mut matched_external: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Match internal records against external
    for int_rec in internal {
        match external_map.get(int_rec.id.as_str()) {
            Some(ext_rec) => {
                matched_external.insert(ext_rec.id.as_str());
                let amount_match = int_rec.amount == ext_rec.amount;
                let date_match = int_rec.date == ext_rec.date;

                let status = match (amount_match, date_match) {
                    (true, true) => ReconcileStatus::Matched,
                    (false, true) => ReconcileStatus::AmountMismatch {
                        internal: int_rec.amount,
                        external: ext_rec.amount,
                    },
                    (true, false) => ReconcileStatus::DateMismatch {
                        internal: int_rec.date.clone(),
                        external: ext_rec.date.clone(),
                    },
                    (false, false) => ReconcileStatus::MultipleMismatch {
                        amount: Some((int_rec.amount, ext_rec.amount)),
                        date: Some((int_rec.date.clone(), ext_rec.date.clone())),
                    },
                };

                results.push(ReconcileResult {
                    id: int_rec.id.clone(),
                    status,
                });
            }
            None => {
                results.push(ReconcileResult {
                    id: int_rec.id.clone(),
                    status: ReconcileStatus::InternalOnly,
                });
            }
        }
    }

    // Find external-only records
    for ext_rec in external {
        if !matched_external.contains(ext_rec.id.as_str()) {
            results.push(ReconcileResult {
                id: ext_rec.id.clone(),
                status: ReconcileStatus::ExternalOnly,
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, amount: Balance, date: &str) -> ReconcileRecord {
        ReconcileRecord {
            id: id.to_string(),
            amount,
            date: date.to_string(),
        }
    }

    #[test]
    fn perfect_match() {
        let internal = vec![rec("TX001", 100_00, "2026-01-15")];
        let external = vec![rec("TX001", 100_00, "2026-01-15")];

        let results = reconcile(&internal, &external);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, ReconcileStatus::Matched);
    }

    #[test]
    fn amount_mismatch() {
        let internal = vec![rec("TX001", 100_00, "2026-01-15")];
        let external = vec![rec("TX001", 99_00, "2026-01-15")];

        let results = reconcile(&internal, &external);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].status, ReconcileStatus::AmountMismatch { .. }));
    }

    #[test]
    fn internal_only_and_external_only() {
        let internal = vec![rec("TX001", 100_00, "2026-01-15")];
        let external = vec![rec("TX002", 200_00, "2026-01-16")];

        let results = reconcile(&internal, &external);
        assert_eq!(results.len(), 2);

        let internal_only = results.iter().find(|r| r.id == "TX001").unwrap();
        assert_eq!(internal_only.status, ReconcileStatus::InternalOnly);

        let external_only = results.iter().find(|r| r.id == "TX002").unwrap();
        assert_eq!(external_only.status, ReconcileStatus::ExternalOnly);
    }

    #[test]
    fn empty_datasets() {
        let results = reconcile(&[], &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn large_dataset_performance() {
        let n = 10_000;
        let internal: Vec<_> = (0..n).map(|i| rec(&format!("TX{i:06}"), i * 100, "2026-01-01")).collect();
        let external: Vec<_> = (0..n).map(|i| rec(&format!("TX{i:06}"), i * 100, "2026-01-01")).collect();

        let results = reconcile(&internal, &external);
        assert_eq!(results.len(), n as usize);
        assert!(results.iter().all(|r| r.status == ReconcileStatus::Matched));
    }
}
