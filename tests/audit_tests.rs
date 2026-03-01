#![allow(clippy::inconsistent_digit_grouping)]

use kromia_ledger::{
    AccountId, AccountType, AuditMeta, Balance, Currency, Ledger, RATE_SCALE,
};

fn usd() -> Currency { Currency::usd() }
fn idr() -> Currency { Currency::idr() }
fn rate_usd_idr() -> Balance { 157 * RATE_SCALE }

fn setup_ledger() -> (Ledger, AccountId, AccountId) {
    let mut ledger = Ledger::new();
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
    let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();
    (ledger, cash, revenue)
}

#[test]
fn transaction_with_audit_metadata() {
    let (mut ledger, cash, revenue) = setup_ledger();
    let audit = AuditMeta::new("admin@kromia.io")
        .with_source("192.168.1.1")
        .with_notes("Monthly reconciliation");

    let entry_id = ledger.record_transaction_audited(
        "Payment", &[(cash, 500_00)], &[(revenue, 500_00)],
        1_000_000, None, audit,
    ).unwrap();

    let entry = ledger.find_entry(entry_id).unwrap();
    let meta = entry.audit.as_ref().unwrap();
    assert_eq!(meta.actor, "admin@kromia.io");
    assert_eq!(meta.source.as_deref(), Some("192.168.1.1"));
    assert_eq!(meta.notes.as_deref(), Some("Monthly reconciliation"));
    assert!(ledger.verify_chain());
}

#[test]
fn audit_included_in_hash() {
    let (mut l1, c1, r1) = setup_ledger();
    let (mut l2, c2, r2) = setup_ledger();

    l1.record_transaction_audited(
        "TX", &[(c1, 100_00)], &[(r1, 100_00)], 1_000_000, None,
        AuditMeta::new("alice"),
    ).unwrap();

    l2.record_transaction_audited(
        "TX", &[(c2, 100_00)], &[(r2, 100_00)], 1_000_000, None,
        AuditMeta::new("bob"),
    ).unwrap();

    // Same transaction with different actors → different hashes
    assert_ne!(l1.entries()[0].hash, l2.entries()[0].hash);
}

#[test]
fn no_audit_backward_compatible() {
    let (mut l1, c1, r1) = setup_ledger();
    let (mut l2, c2, r2) = setup_ledger();

    l1.record_transaction_full("TX", &[(c1, 100_00)], &[(r1, 100_00)], 1_000_000, None).unwrap();
    l2.record_transaction_full("TX", &[(c2, 100_00)], &[(r2, 100_00)], 1_000_000, None).unwrap();

    // Without audit, hashes match (backward compatible)
    assert_eq!(l1.entries()[0].hash, l2.entries()[0].hash);
    assert!(l1.entries()[0].audit.is_none());
}

#[test]
fn entries_by_actor() {
    let (mut ledger, cash, revenue) = setup_ledger();

    ledger.record_transaction_audited(
        "TX1", &[(cash, 100_00)], &[(revenue, 100_00)], 100, None,
        AuditMeta::new("alice"),
    ).unwrap();
    ledger.record_transaction_audited(
        "TX2", &[(cash, 200_00)], &[(revenue, 200_00)], 200, None,
        AuditMeta::new("bob"),
    ).unwrap();
    ledger.record_transaction_audited(
        "TX3", &[(cash, 300_00)], &[(revenue, 300_00)], 300, None,
        AuditMeta::new("alice"),
    ).unwrap();
    // Entry without audit metadata
    ledger.record_transaction_full(
        "TX4", &[(cash, 50_00)], &[(revenue, 50_00)], 400, None,
    ).unwrap();

    assert_eq!(ledger.entries_by_actor("alice").len(), 2);
    assert_eq!(ledger.entries_by_actor("bob").len(), 1);
    assert_eq!(ledger.entries_by_actor("charlie").len(), 0);
}

#[test]
fn audit_survives_persistence() {
    let (mut ledger, cash, revenue) = setup_ledger();
    let audit = AuditMeta::new("admin")
        .with_source("api/v1")
        .with_notes("Bulk import");

    ledger.record_transaction_audited(
        "TX", &[(cash, 100_00)], &[(revenue, 100_00)], 100, Some("KEY-1"), audit,
    ).unwrap();

    let json = ledger.save_json().unwrap();
    let restored = Ledger::load_json(&json).unwrap();

    assert!(restored.verify_chain());
    let entry = &restored.entries()[0];
    let meta = entry.audit.as_ref().unwrap();
    assert_eq!(meta.actor, "admin");
    assert_eq!(meta.source.as_deref(), Some("api/v1"));
    assert_eq!(meta.notes.as_deref(), Some("Bulk import"));
}

#[test]
fn exchange_with_audit() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

    let audit = AuditMeta::new("treasury-bot").with_source("fx-service");

    let entry_id = ledger.record_exchange_audited(
        "USD→IDR", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100, None, audit,
    ).unwrap();

    let entry = ledger.find_entry(entry_id).unwrap();
    assert_eq!(entry.audit.as_ref().unwrap().actor, "treasury-bot");
    assert!(ledger.verify_chain());
    assert_eq!(ledger.get_balance(cash_usd).unwrap(), -500);
    assert_eq!(ledger.get_balance(cash_idr).unwrap(), 78_500);
}

#[test]
fn audit_actor_only_minimal() {
    let (mut ledger, cash, revenue) = setup_ledger();
    let audit = AuditMeta::new("system");

    ledger.record_transaction_audited(
        "TX", &[(cash, 100_00)], &[(revenue, 100_00)], 100, None, audit,
    ).unwrap();

    let entry = &ledger.entries()[0];
    let meta = entry.audit.as_ref().unwrap();
    assert_eq!(meta.actor, "system");
    assert!(meta.source.is_none());
    assert!(meta.notes.is_none());
    assert!(ledger.verify_chain());
}
