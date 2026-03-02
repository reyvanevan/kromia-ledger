//! Integration tests for storage backends (Phase 8).
#![allow(clippy::inconsistent_digit_grouping)]

use kromia_ledger::{AccountType, AuditMeta, Currency, Ledger, LedgerError};
use kromia_ledger::store::{LedgerStore, MemoryStore};

#[cfg(not(target_arch = "wasm32"))]
use kromia_ledger::store::JsonFileStore;

/// Helper: create a ledger with a few transactions.
fn sample_ledger() -> Ledger {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let eq   = l.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();
    let rev  = l.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

    l.record_transaction_at("Invest", &[(cash, 1_000_000)], &[(eq, 1_000_000)], 100).unwrap();
    l.record_transaction_at("Sale", &[(cash, 50_000)], &[(rev, 50_000)], 200).unwrap();
    l
}

// ── MemoryStore ─────────────────────────────────────────────────────

#[test]
fn memory_store_roundtrip() {
    let ledger = sample_ledger();
    let mut store = MemoryStore::new();

    assert!(!store.has_data());
    store.save(&ledger).unwrap();
    assert!(store.has_data());

    let restored = store.load().unwrap();
    assert!(restored.verify_chain());
    assert_eq!(restored.trial_balance(), 0);
    assert_eq!(restored.entries().len(), 2);
}

#[test]
fn memory_store_load_empty_returns_error() {
    let store = MemoryStore::new();
    let err = store.load().unwrap_err();
    match err {
        LedgerError::Storage(msg) => assert!(msg.contains("empty"), "got: {msg}"),
        other => panic!("expected Storage error, got: {other:?}"),
    }
}

#[test]
fn memory_store_from_json() {
    let ledger = sample_ledger();
    let json = ledger.save_json().unwrap();

    let store = MemoryStore::from_json(json.clone());
    assert!(store.has_data());
    assert_eq!(store.as_json().unwrap(), json);

    let restored = store.load().unwrap();
    assert!(restored.verify_chain());
}

#[test]
fn memory_store_overwrite() {
    let mut ledger = sample_ledger();
    let mut store = MemoryStore::new();

    store.save(&ledger).unwrap();
    let json_v1 = store.as_json().unwrap().len();

    // Add more transactions, save again
    let cash = ledger.account_by_code("1000").unwrap().id;
    let rev  = ledger.account_by_code("4000").unwrap().id;
    ledger.record_transaction_at("Sale 2", &[(cash, 10_000)], &[(rev, 10_000)], 300).unwrap();

    store.save(&ledger).unwrap();
    let json_v2 = store.as_json().unwrap().len();

    assert!(json_v2 > json_v1, "second save should be larger");
    let restored = store.load().unwrap();
    assert_eq!(restored.entries().len(), 3);
}

#[test]
fn memory_store_tampered_json_detected() {
    let ledger = sample_ledger();
    let mut json = ledger.save_json().unwrap();

    // Tamper: change a hash character
    json = json.replace("\"hash\":", "\"hash\":\"TAMPERED\",\"_old_hash\":");
    // This will fail at load_json's chain verification
    let store = MemoryStore::from_json(json);
    assert!(store.load().is_err());
}

#[test]
fn memory_store_preserves_audit() {
    let mut ledger = Ledger::new();
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let rev  = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

    let audit = AuditMeta::new("reyvan").with_source("test").with_notes("store test");
    ledger.record_transaction_audited("Audited tx", &[(cash, 100_00)], &[(rev, 100_00)], 100, None, audit).unwrap();

    let mut store = MemoryStore::new();
    store.save(&ledger).unwrap();

    let restored = store.load().unwrap();
    let entry = &restored.entries()[0];
    let audit = entry.audit.as_ref().unwrap();
    assert_eq!(audit.actor, "reyvan");
    assert_eq!(audit.source.as_deref(), Some("test"));
    assert_eq!(audit.notes.as_deref(), Some("store test"));
}

#[test]
fn memory_store_preserves_idempotency_keys() {
    let mut ledger = Ledger::new();
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let eq   = ledger.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();

    ledger.record_transaction_full("Invest", &[(cash, 100_00)], &[(eq, 100_00)], 100, Some("KEY-001")).unwrap();

    let mut store = MemoryStore::new();
    store.save(&ledger).unwrap();
    let restored = store.load().unwrap();

    // Idempotency key should be rebuilt on load
    let err = restored.clone().record_transaction_full("Dup", &[(cash, 100_00)], &[(eq, 100_00)], 200, Some("KEY-001")).unwrap_err();
    match err {
        LedgerError::DuplicateIdempotencyKey(key) => assert_eq!(key, "KEY-001"),
        other => panic!("expected DuplicateIdempotencyKey, got: {other:?}"),
    }
}

// ── Trait object safety ─────────────────────────────────────────────

#[test]
fn store_is_object_safe() {
    let ledger = sample_ledger();

    // Prove we can use Box<dyn LedgerStore>
    let mut store: Box<dyn LedgerStore> = Box::new(MemoryStore::new());
    store.save(&ledger).unwrap();
    let restored = store.load().unwrap();
    assert!(restored.verify_chain());
}

// ── JsonFileStore ───────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod json_file {
    use super::*;
    use std::fs;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("kromia-ledger-tests");
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn json_file_store_roundtrip() {
        let path = temp_path("roundtrip.json");
        let ledger = sample_ledger();

        let mut store = JsonFileStore::new(&path);
        store.save(&ledger).unwrap();
        assert!(store.has_data());
        assert!(path.exists());

        let restored = store.load().unwrap();
        assert!(restored.verify_chain());
        assert_eq!(restored.entries().len(), 2);
        assert_eq!(restored.trial_balance(), 0);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn json_file_store_load_nonexistent_returns_error() {
        let store = JsonFileStore::new("/tmp/kromia-nonexistent-12345.json");
        assert!(!store.has_data());
        let err = store.load().unwrap_err();
        match err {
            LedgerError::Storage(msg) => assert!(msg.contains("No such file") || msg.contains("not found"), "got: {msg}"),
            other => panic!("expected Storage error, got: {other:?}"),
        }
    }

    #[test]
    fn json_file_store_overwrite() {
        let path = temp_path("overwrite.json");
        let mut ledger = sample_ledger();
        let mut store = JsonFileStore::new(&path);

        store.save(&ledger).unwrap();
        let size_v1 = fs::metadata(&path).unwrap().len();

        let cash = ledger.account_by_code("1000").unwrap().id;
        let rev  = ledger.account_by_code("4000").unwrap().id;
        ledger.record_transaction_at("Extra", &[(cash, 10_000)], &[(rev, 10_000)], 300).unwrap();

        store.save(&ledger).unwrap();
        let size_v2 = fs::metadata(&path).unwrap().len();

        assert!(size_v2 > size_v1);
        let restored = store.load().unwrap();
        assert_eq!(restored.entries().len(), 3);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn json_file_store_tampered_file_detected() {
        let path = temp_path("tampered.json");
        let ledger = sample_ledger();

        let mut store = JsonFileStore::new(&path);
        store.save(&ledger).unwrap();

        // Tamper: change a transaction description (part of the hash)
        let mut content = fs::read_to_string(&path).unwrap();
        content = content.replacen("\"Invest\"", "\"HACKED\"", 1);
        fs::write(&path, content).unwrap();

        let err = store.load().unwrap_err();
        assert!(matches!(err, LedgerError::ChainBroken(_)));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn json_file_store_path_accessor() {
        let store = JsonFileStore::new("/tmp/test.json");
        assert_eq!(store.path().to_str().unwrap(), "/tmp/test.json");
    }
}
