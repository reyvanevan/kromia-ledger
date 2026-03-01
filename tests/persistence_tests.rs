#![allow(clippy::inconsistent_digit_grouping)]

use kromia_ledger::{Ledger, AccountType, Currency, AccountId, Balance, LedgerError, RATE_SCALE};

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
fn persistence_roundtrip() {
    let (mut ledger, cash, revenue) = setup_ledger();
    ledger.record_transaction_at("TX-1", &[(cash, 500_00)], &[(revenue, 500_00)], 100).unwrap();
    ledger.record_transaction_at("TX-2", &[(cash, 250_00)], &[(revenue, 250_00)], 200).unwrap();

    let json = ledger.save_json().unwrap();
    let restored = Ledger::load_json(&json).unwrap();

    assert!(restored.verify_chain());
    assert_eq!(restored.entries().len(), 2);
    assert_eq!(restored.trial_balance(), 0);
    assert_eq!(restored.get_balance(cash).unwrap(), 750_00);
}

#[test]
fn tampered_json_detected() {
    let (mut ledger, cash, revenue) = setup_ledger();
    ledger.record_transaction_at("TX", &[(cash, 100_00)], &[(revenue, 100_00)], 100).unwrap();

    let json = ledger.save_json().unwrap();
    let tampered = json.replace("\"TX\"", "\"HACKED\"");
    let result = Ledger::load_json(&tampered);
    assert!(result.is_err());
}

#[test]
fn idempotency_key_survives_persistence() {
    let (mut ledger, cash, revenue) = setup_ledger();
    ledger.record_transaction_full(
        "TX", &[(cash, 100_00)], &[(revenue, 100_00)], 100, Some("PERSIST-KEY"),
    ).unwrap();

    let json = ledger.save_json().unwrap();
    let mut restored = Ledger::load_json(&json).unwrap();

    // The key must still be tracked after load
    let dup = restored.record_transaction_full(
        "TX again", &[(cash, 100_00)], &[(revenue, 100_00)], 200, Some("PERSIST-KEY"),
    );
    assert!(matches!(dup, Err(LedgerError::DuplicateIdempotencyKey(_))));
}

#[test]
fn exchange_persistence_roundtrip() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

    ledger.record_exchange_at(
        "USD to IDR", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100,
    ).unwrap();

    let json = ledger.save_json().unwrap();
    let restored = Ledger::load_json(&json).unwrap();

    assert!(restored.verify_chain());
    assert_eq!(restored.entries().len(), 1);
    assert_eq!(restored.get_balance(cash_usd).unwrap(), -500);
    assert_eq!(restored.get_balance(cash_idr).unwrap(), 78_500);

    // Verify exchange rate is preserved
    let entry = &restored.entries()[0];
    let xr = entry.transaction.exchange_rate.as_ref().unwrap();
    assert_eq!(xr.rate, rate_usd_idr());
    assert_eq!(xr.from_currency, "USD");
    assert_eq!(xr.to_currency, "IDR");
}
