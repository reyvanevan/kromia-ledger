//! Storage trait and backends for ledger persistence.
//!
//! This module provides a [`LedgerStore`] trait that abstracts where
//! ledger data is persisted. Two built-in backends are included:
//!
//! - [`MemoryStore`] — in-memory storage (tests, WASM, ephemeral use)
//! - [`JsonFileStore`] — file-based JSON persistence (demo, small ledgers)
//!
//! The core [`Ledger`] engine remains storage-agnostic.
//! Users interact with stores directly:
//!
//! ```
//! use kromia_ledger::{Ledger, AccountType, Currency};
//! use kromia_ledger::store::{LedgerStore, MemoryStore};
//!
//! let mut ledger = Ledger::new();
//! let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
//! let eq   = ledger.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();
//! ledger.record_transaction("Invest", &[(cash, 500_00)], &[(eq, 500_00)]).unwrap();
//!
//! // Save to a memory store
//! let mut store = MemoryStore::new();
//! store.save(&ledger).unwrap();
//!
//! // Load back — chain is automatically verified
//! let restored = store.load().unwrap();
//! assert!(restored.verify_chain());
//! assert_eq!(restored.trial_balance(), 0);
//! ```
//!
//! ## Implementing a custom backend
//!
//! Implement [`LedgerStore`] for any storage target (SQLite, PostgreSQL,
//! S3, Redis, etc.). The trait is intentionally minimal — two methods.
//!
//! ```rust,ignore
//! use kromia_ledger::store::LedgerStore;
//! use kromia_ledger::{Ledger, LedgerError};
//!
//! struct PostgresStore { /* pool, config */ }
//!
//! impl LedgerStore for PostgresStore {
//!     fn save(&mut self, ledger: &Ledger) -> Result<(), LedgerError> {
//!         // serialize + INSERT/UPSERT
//!         todo!()
//!     }
//!     fn load(&self) -> Result<Ledger, LedgerError> {
//!         // SELECT + deserialize + verify chain
//!         todo!()
//!     }
//! }
//! ```

use crate::validation::LedgerError;
use crate::Ledger;

// ── Trait ────────────────────────────────────────────────────────────

/// A backend for persisting and restoring ledger state.
///
/// Implementations handle I/O mechanics (file, database, network).
/// The serialization format is an implementation detail — [`MemoryStore`]
/// and [`JsonFileStore`] both use JSON internally, but a PostgreSQL
/// backend could store entries row-by-row.
///
/// # Object Safety
///
/// This trait is object-safe: you can use `Box<dyn LedgerStore>` or
/// `&mut dyn LedgerStore` for runtime polymorphism.
pub trait LedgerStore {
    /// Persist the entire ledger state to this store.
    ///
    /// Overwrites any previously stored state.
    fn save(&mut self, ledger: &Ledger) -> Result<(), LedgerError>;

    /// Restore a ledger from this store.
    ///
    /// Implementations **must** verify chain integrity after loading.
    /// Return [`LedgerError::ChainBroken`] if verification fails.
    fn load(&self) -> Result<Ledger, LedgerError>;

    /// Check whether this store contains a previously saved ledger.
    fn has_data(&self) -> bool;
}

// ── MemoryStore ─────────────────────────────────────────────────────

/// In-memory store backed by a JSON string.
///
/// Useful for tests, WASM targets, and ephemeral ledgers.
/// Data is lost when the store is dropped.
///
/// # Examples
///
/// ```
/// use kromia_ledger::Ledger;
/// use kromia_ledger::store::{LedgerStore, MemoryStore};
///
/// let mut store = MemoryStore::new();
/// assert!(!store.has_data());
///
/// let ledger = Ledger::new();
/// store.save(&ledger).unwrap();
/// assert!(store.has_data());
///
/// let restored = store.load().unwrap();
/// assert!(restored.verify_chain());
/// ```
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    data: Option<String>,
}

impl MemoryStore {
    /// Create an empty memory store.
    pub fn new() -> Self {
        Self { data: None }
    }

    /// Create a memory store pre-loaded with a JSON string.
    ///
    /// The JSON is **not** validated until [`load`](LedgerStore::load) is called.
    pub fn from_json(json: String) -> Self {
        Self { data: Some(json) }
    }

    /// Get the stored JSON string, if any.
    pub fn as_json(&self) -> Option<&str> {
        self.data.as_deref()
    }
}

impl LedgerStore for MemoryStore {
    fn save(&mut self, ledger: &Ledger) -> Result<(), LedgerError> {
        self.data = Some(ledger.save_json()?);
        Ok(())
    }

    fn load(&self) -> Result<Ledger, LedgerError> {
        let json = self.data.as_deref()
            .ok_or_else(|| LedgerError::Storage("memory store is empty".into()))?;
        Ledger::load_json(json)
    }

    fn has_data(&self) -> bool {
        self.data.is_some()
    }
}

// ── JsonFileStore ───────────────────────────────────────────────────

/// File-based store using pretty-printed JSON.
///
/// Reads and writes the entire ledger as a single JSON file.
/// Suitable for demos, small ledgers, and local development.
///
/// # Examples
///
/// ```no_run
/// use kromia_ledger::Ledger;
/// use kromia_ledger::store::{LedgerStore, JsonFileStore};
///
/// let mut store = JsonFileStore::new("ledger.json");
///
/// let ledger = Ledger::new();
/// store.save(&ledger).unwrap();
///
/// let restored = store.load().unwrap();
/// assert!(restored.verify_chain());
/// ```
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct JsonFileStore {
    path: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl JsonFileStore {
    /// Create a store that reads/writes to the given file path.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Get the file path this store is configured to use.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl LedgerStore for JsonFileStore {
    fn save(&mut self, ledger: &Ledger) -> Result<(), LedgerError> {
        let json = ledger.save_json()?;
        std::fs::write(&self.path, json)
            .map_err(|e| LedgerError::Storage(format!("{}: {}", self.path.display(), e)))
    }

    fn load(&self) -> Result<Ledger, LedgerError> {
        let json = std::fs::read_to_string(&self.path)
            .map_err(|e| LedgerError::Storage(format!("{}: {}", self.path.display(), e)))?;
        Ledger::load_json(&json)
    }

    fn has_data(&self) -> bool {
        self.path.exists()
    }
}
