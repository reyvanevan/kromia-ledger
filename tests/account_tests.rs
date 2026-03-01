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
fn balanced_transaction_succeeds() {
    let (mut ledger, cash, revenue) = setup_ledger();
    let result = ledger.record_transaction(
        "Payment received",
        &[(cash, 100_00)],
        &[(revenue, 100_00)],
    );
    assert!(result.is_ok());
    assert!(ledger.verify_chain());
    assert_eq!(ledger.trial_balance(), 0);
}

#[test]
fn unbalanced_transaction_fails() {
    let (mut ledger, cash, revenue) = setup_ledger();
    let result = ledger.record_transaction(
        "Bad transaction",
        &[(cash, 100_00)],
        &[(revenue, 50_00)],
    );
    assert!(result.is_err());
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn atomic_transaction_no_partial_state() {
    let mut ledger = Ledger::new();
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
    let fake_id = AccountId(999);

    let result = ledger.record_transaction(
        "Should fail",
        &[(cash, 100_00)],
        &[(fake_id, 100_00)],
    );
    assert!(result.is_err());
    assert_eq!(ledger.get_balance(cash).unwrap(), 0);
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn inactive_account_rejected() {
    let (mut ledger, cash, revenue) = setup_ledger();
    ledger.deactivate_account(cash).unwrap();

    let result = ledger.record_transaction(
        "Should fail",
        &[(cash, 100_00)],
        &[(revenue, 100_00)],
    );
    assert!(matches!(result, Err(LedgerError::InactiveAccount(_))));
}

#[test]
fn duplicate_account_code_rejected() {
    let mut ledger = Ledger::new();
    ledger.create_account("Cash", "1000", AccountType::Asset, usd()).unwrap();
    let dup = ledger.create_account("Cash 2", "1000", AccountType::Asset, usd());
    assert!(matches!(dup, Err(LedgerError::DuplicateAccountCode(_))));
}

#[test]
fn account_by_code_lookup() {
    let (ledger, _, _) = setup_ledger();
    let acc = ledger.account_by_code("1000").unwrap();
    assert_eq!(acc.name, "Cash");
    assert!(ledger.account_by_code("9999").is_none());
}

#[test]
fn account_stores_currency() {
    let mut ledger = Ledger::new();
    let id = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::new("JPY", 0)).unwrap();
    let acc = ledger.get_account(id).unwrap();
    assert_eq!(acc.currency.code, "JPY");
    assert_eq!(acc.currency.precision, 0);
}

#[test]
fn currency_mismatch_rejected() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1100", AccountType::Asset, usd()).unwrap();
    let revenue_idr = ledger.create_account("Revenue IDR", "4100", AccountType::Revenue, Currency::idr()).unwrap();

    let result = ledger.record_transaction(
        "Cross-currency should fail",
        &[(cash_usd, 100_00)],
        &[(revenue_idr, 100_00)],
    );
    assert!(matches!(result, Err(LedgerError::CurrencyMismatch { .. })));
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn same_currency_transaction_succeeds() {
    let mut ledger = Ledger::new();
    let kas = ledger.create_account("Kas", "1100", AccountType::Asset, Currency::idr()).unwrap();
    let pendapatan = ledger.create_account("Pendapatan", "4100", AccountType::Revenue, Currency::idr()).unwrap();

    let result = ledger.record_transaction(
        "Penjualan",
        &[(kas, 1_000_000)],
        &[(pendapatan, 1_000_000)],
    );
    assert!(result.is_ok());
    assert_eq!(ledger.get_balance(kas).unwrap(), 1_000_000);
}
