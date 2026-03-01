#![allow(clippy::inconsistent_digit_grouping)]

use kromia_ledger::{Ledger, AccountType, Currency, AccountId, Balance, LedgerError, RATE_SCALE};

fn usd() -> Currency { Currency::usd() }
fn idr() -> Currency { Currency::idr() }

// 1 USD cent = 157 IDR → 1 USD = 15,700 IDR
fn rate_usd_idr() -> Balance { 157 * RATE_SCALE }

#[test]
fn exchange_basic_succeeds() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();
    let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, usd()).unwrap();

    // Seed $10.00 into Cash USD
    ledger.record_transaction_at("Deposit", &[(cash_usd, 1000)], &[(revenue, 1000)], 100).unwrap();

    // Exchange $5.00 (500 cents) → Rp 78,500
    // 500 × 157_000_000 / 1_000_000 = 78,500 ✓
    let result = ledger.record_exchange_at(
        "USD to IDR", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 200,
    );
    assert!(result.is_ok());
    assert_eq!(ledger.get_balance(cash_usd).unwrap(), 500);  // 1000 - 500
    assert_eq!(ledger.get_balance(cash_idr).unwrap(), 78_500);
    assert!(ledger.verify_chain());
    assert_eq!(ledger.entries().len(), 2);
}

#[test]
fn exchange_rate_mismatch_rejected() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

    // to_amount doesn't match: 500 × 157M / 1M = 78,500, not 99,999
    let result = ledger.record_exchange_at(
        "Bad exchange", cash_usd, 500, cash_idr, 99_999, rate_usd_idr(), 100,
    );
    assert!(matches!(result, Err(LedgerError::ExchangeRateMismatch { .. })));
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn exchange_invalid_rate_rejected() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

    let result = ledger.record_exchange_at(
        "Zero rate", cash_usd, 500, cash_idr, 78_500, 0, 100,
    );
    assert!(matches!(result, Err(LedgerError::InvalidExchangeRate(_))));
}

#[test]
fn exchange_rounding_tolerance() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

    // Exact: 500 × 157_000_000 / 1_000_000 = 78,500
    // Off by 1 (within tolerance): 78,501
    let result = ledger.record_exchange_at(
        "Rounded", cash_usd, 500, cash_idr, 78_501, rate_usd_idr(), 100,
    );
    assert!(result.is_ok()); // ±1 tolerance

    // Off by 2 (beyond tolerance): 78,502
    let mut ledger2 = Ledger::new();
    let cu = ledger2.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let ci = ledger2.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();
    let result2 = ledger2.record_exchange_at(
        "Too far off", cu, 500, ci, 78_502, rate_usd_idr(), 100,
    );
    assert!(matches!(result2, Err(LedgerError::ExchangeRateMismatch { .. })));
}

#[test]
fn exchange_deterministic_hash() {
    let mut l1 = Ledger::new();
    let u1 = l1.create_account("USD", "1100", AccountType::Asset, usd()).unwrap();
    let i1 = l1.create_account("IDR", "1200", AccountType::Asset, idr()).unwrap();

    let mut l2 = Ledger::new();
    let u2 = l2.create_account("USD", "1100", AccountType::Asset, usd()).unwrap();
    let i2 = l2.create_account("IDR", "1200", AccountType::Asset, idr()).unwrap();

    l1.record_exchange_at("XCH", u1, 500, i1, 78_500, rate_usd_idr(), 1_000_000).unwrap();
    l2.record_exchange_at("XCH", u2, 500, i2, 78_500, rate_usd_idr(), 1_000_000).unwrap();

    assert_eq!(l1.entries()[0].hash, l2.entries()[0].hash);
}

#[test]
fn exchange_atomicity() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let fake_id = AccountId(999);

    // to_account doesn't exist — must fail atomically
    let result = ledger.record_exchange_at(
        "Fail", cash_usd, 500, fake_id, 78_500, rate_usd_idr(), 100,
    );
    assert!(result.is_err());
    assert_eq!(ledger.get_balance(cash_usd).unwrap(), 0);
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn exchange_inactive_account_rejected() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();
    ledger.deactivate_account(cash_usd).unwrap();

    let result = ledger.record_exchange_at(
        "Fail", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100,
    );
    assert!(matches!(result, Err(LedgerError::InactiveAccount(_))));
}

#[test]
fn exchange_idempotency_key() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let cash_idr = ledger.create_account("Cash IDR", "1200", AccountType::Asset, idr()).unwrap();

    ledger.record_exchange_full(
        "XCH", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 100, Some("XCH-001"),
    ).unwrap();

    // Same key — must be rejected
    let dup = ledger.record_exchange_full(
        "XCH retry", cash_usd, 500, cash_idr, 78_500, rate_usd_idr(), 200, Some("XCH-001"),
    );
    assert!(matches!(dup, Err(LedgerError::DuplicateIdempotencyKey(_))));
    assert_eq!(ledger.entries().len(), 1);
}
