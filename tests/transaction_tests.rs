#![allow(clippy::inconsistent_digit_grouping)]

use kromia_ledger::{Ledger, AccountType, Currency, AccountId, LedgerError};

fn usd() -> Currency { Currency::usd() }

fn setup_ledger() -> (Ledger, AccountId, AccountId) {
    let mut ledger = Ledger::new();
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
    let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();
    (ledger, cash, revenue)
}

#[test]
fn deterministic_transaction_at() {
    let (mut l1, c1, r1) = setup_ledger();
    let (mut l2, c2, r2) = setup_ledger();

    l1.record_transaction_at("TX", &[(c1, 500_00)], &[(r1, 500_00)], 1_000_000).unwrap();
    l2.record_transaction_at("TX", &[(c2, 500_00)], &[(r2, 500_00)], 1_000_000).unwrap();

    assert_eq!(l1.entries()[0].hash, l2.entries()[0].hash);
}

#[test]
fn chain_integrity_holds() {
    let (mut ledger, cash, revenue) = setup_ledger();
    for i in 0..10 {
        ledger.record_transaction_at(
            &format!("Entry {i}"),
            &[(cash, 10_00)],
            &[(revenue, 10_00)],
            1_000_000 + i,
        ).unwrap();
    }
    assert!(ledger.verify_chain());
    assert_eq!(ledger.entries().len(), 10);
}

#[test]
fn query_entries_for_account() {
    let mut ledger = Ledger::new();
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
    let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();
    let expense = ledger.create_account("Rent", "5000", AccountType::Expense, usd()).unwrap();

    ledger.record_transaction("Sale", &[(cash, 100_00)], &[(revenue, 100_00)]).unwrap();
    ledger.record_transaction("Rent", &[(expense, 30_00)], &[(cash, 30_00)]).unwrap();

    assert_eq!(ledger.entries_for_account(cash).len(), 2);
    assert_eq!(ledger.entries_for_account(revenue).len(), 1);
    assert_eq!(ledger.entries_for_account(expense).len(), 1);
}

#[test]
fn idempotency_key_prevents_duplicate() {
    let (mut ledger, cash, revenue) = setup_ledger();

    let r1 = ledger.record_transaction_full(
        "Order #1",
        &[(cash, 100_00)],
        &[(revenue, 100_00)],
        1_000_000,
        Some("ORDER-001"),
    );
    assert!(r1.is_ok());

    // Same key again — must be rejected
    let r2 = ledger.record_transaction_full(
        "Order #1 retry",
        &[(cash, 100_00)],
        &[(revenue, 100_00)],
        1_000_001,
        Some("ORDER-001"),
    );
    assert!(matches!(r2, Err(LedgerError::DuplicateIdempotencyKey(_))));
    assert_eq!(ledger.entries().len(), 1);
}

#[test]
fn different_idempotency_keys_both_succeed() {
    let (mut ledger, cash, revenue) = setup_ledger();

    ledger.record_transaction_full(
        "Order A", &[(cash, 50_00)], &[(revenue, 50_00)], 100, Some("KEY-A"),
    ).unwrap();
    ledger.record_transaction_full(
        "Order B", &[(cash, 50_00)], &[(revenue, 50_00)], 200, Some("KEY-B"),
    ).unwrap();

    assert_eq!(ledger.entries().len(), 2);
}

#[test]
fn no_idempotency_key_allows_duplicates() {
    let (mut ledger, cash, revenue) = setup_ledger();

    // Without idempotency key, identical transactions are allowed
    ledger.record_transaction("TX", &[(cash, 100_00)], &[(revenue, 100_00)]).unwrap();
    ledger.record_transaction("TX", &[(cash, 100_00)], &[(revenue, 100_00)]).unwrap();
    assert_eq!(ledger.entries().len(), 2);
}
