//! Audit trail metadata for tamper-evident entry provenance.
//!
//! Every ledger mutation can carry an [`AuditMeta`] recording *who* performed
//! the action and *from where*. This metadata is included in the SHA-256 hash
//! computation, making it part of the tamper-evident chain — once recorded,
//! it cannot be altered without breaking the hash chain.
//!
//! # Example
//!
//! ```
//! use kromia_ledger::AuditMeta;
//!
//! let audit = AuditMeta::new("admin@kromia.io")
//!     .with_source("192.168.1.1")
//!     .with_notes("Monthly closing adjustment");
//!
//! assert_eq!(audit.actor, "admin@kromia.io");
//! assert_eq!(audit.source.as_deref(), Some("192.168.1.1"));
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Metadata capturing who performed a ledger action and from where.
///
/// Used with [`Ledger::record_transaction_audited`](crate::Ledger::record_transaction_audited)
/// and [`Ledger::record_exchange_audited`](crate::Ledger::record_exchange_audited).
/// Included in the SHA-256 hash of the [`LedgerEntry`](crate::LedgerEntry),
/// making it tamper-evident.
///
/// # Builder Pattern
///
/// ```
/// use kromia_ledger::AuditMeta;
///
/// let audit = AuditMeta::new("api-key-abc123")
///     .with_source("POST /api/v1/transactions")
///     .with_notes("Automated nightly sweep");
///
/// assert_eq!(audit.actor, "api-key-abc123");
/// assert_eq!(audit.source.as_deref(), Some("POST /api/v1/transactions"));
/// assert_eq!(audit.notes.as_deref(), Some("Automated nightly sweep"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditMeta {
    /// The actor who initiated the action (user ID, API key, service name, etc.).
    pub actor: String,
    /// The origin of the action (IP address, API endpoint, module path, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Free-form notes or justification for the action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl AuditMeta {
    /// Create audit metadata with just an actor identifier.
    ///
    /// Use the builder methods [`with_source`](Self::with_source) and
    /// [`with_notes`](Self::with_notes) to attach optional context.
    pub fn new(actor: &str) -> Self {
        Self {
            actor: actor.to_string(),
            source: None,
            notes: None,
        }
    }

    /// Attach a source/origin to the audit metadata.
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Attach free-form notes to the audit metadata.
    pub fn with_notes(mut self, notes: &str) -> Self {
        self.notes = Some(notes.to_string());
        self
    }
}

impl fmt::Display for AuditMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "actor={}", self.actor)?;
        if let Some(ref source) = self.source {
            write!(f, ", source={source}")?;
        }
        if let Some(ref notes) = self.notes {
            write!(f, ", notes={notes}")?;
        }
        Ok(())
    }
}
