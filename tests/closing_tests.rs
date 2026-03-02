#![allow(clippy::inconsistent_digit_grouping)]
use kromia_ledger::{
    AccountType, AuditMeta, Currency, Ledger, LedgerError,
};

/// Helper: sets up a ledger with standard USD accounts.
/// Returns (ledger, cash, equity, retained_earnings, revenue, expense).
fn setup_usd() -> (Ledger, kromia_ledger::AccountId, kromia_ledger::AccountId, kromia_ledger::AccountId, kromia_ledger::AccountId, kromia_ledger::AccountId) {
    let mut ledger = Ledger::new();
    let cash     = ledger.create_account("Cash",              "1000", AccountType::Asset,   Currency::usd()).unwrap();
    let equity   = ledger.create_account("Owner Equity",      "3000", AccountType::Equity,  Currency::usd()).unwrap();
    let retained = ledger.create_account("Retained Earnings", "3100", AccountType::Equity,  Currency::usd()).unwrap();
    let revenue  = ledger.create_account("Sales Revenue",     "4000", AccountType::Revenue, Currency::usd()).unwrap();
    let expense  = ledger.create_account("Rent Expense",      "5000", AccountType::Expense, Currency::usd()).unwrap();
    (ledger, cash, equity, retained, revenue, expense)
}

// ── Basic closing ───────────────────────────────────────────────────

#[test]
fn close_period_zeroes_revenue_and_expense() {
    let (mut ledger, cash, equity, retained, revenue, expense) = setup_usd();

    // Seed owner equity
    ledger.record_transaction_at("Capital", &[(cash, 10_000_00)], &[(equity, 10_000_00)], 100).unwrap();
    // Sales
    ledger.record_transaction_at("Sale 1", &[(cash, 3_000_00)], &[(revenue, 3_000_00)], 200).unwrap();
    ledger.record_transaction_at("Sale 2", &[(cash, 2_000_00)], &[(revenue, 2_000_00)], 300).unwrap();
    // Rent
    ledger.record_transaction_at("Rent", &[(expense, 1_500_00)], &[(cash, 1_500_00)], 400).unwrap();

    let entry_id = ledger.close_period("USD", 500, retained).unwrap();
    assert!(entry_id.is_some());

    // Revenue and Expense zeroed
    assert_eq!(ledger.get_balance(revenue).unwrap(), 0);
    assert_eq!(ledger.get_balance(expense).unwrap(), 0);

    // Net income = 5_000 - 1_500 = 3_500
    assert_eq!(ledger.get_balance(retained).unwrap(), 3_500_00);

    // Asset and Equity untouched
    assert_eq!(ledger.get_balance(cash).unwrap(), 13_500_00);
    assert_eq!(ledger.get_balance(equity).unwrap(), 10_000_00);
}

#[test]
fn close_period_net_loss() {
    let (mut ledger, cash, equity, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Capital", &[(cash, 10_000_00)], &[(equity, 10_000_00)], 100).unwrap();
    ledger.record_transaction_at("Sale", &[(cash, 500_00)], &[(revenue, 500_00)], 200).unwrap();
    ledger.record_transaction_at("Rent", &[(expense, 2_000_00)], &[(cash, 2_000_00)], 300).unwrap();

    let entry_id = ledger.close_period("USD", 400, retained).unwrap();
    assert!(entry_id.is_some());

    assert_eq!(ledger.get_balance(revenue).unwrap(), 0);
    assert_eq!(ledger.get_balance(expense).unwrap(), 0);

    // Net loss = 500 - 2000 = -1500 (signed_balance perspective)
    // Revenue had credit-normal balance 500 → net income side = 500 (debit in closing)
    // Expense had debit-normal balance 2000 → net expense side = 2000 (credit in closing)
    // total_debits (from revenue) = 500, total_credits (from expense) = 2000
    // net_income = 500 - 2000 = -1500
    // Since total_credits > total_debits → debit RE for difference (loss)
    // RE signed_balance for Equity (credit-normal): debit decreases → -1500
    assert_eq!(ledger.get_balance(retained).unwrap(), -1_500_00);
}

// ── Sealed period ───────────────────────────────────────────────────

#[test]
fn sealed_period_blocks_backdated_entries() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.close_period("USD", 200, retained).unwrap();

    // Attempt to record at timestamp 150 (within sealed period)
    let err = ledger.record_transaction_at("Late sale", &[(cash, 500_00)], &[(revenue, 500_00)], 150);
    assert!(err.is_err());
    match err.unwrap_err() {
        LedgerError::PeriodClosed { currency, closed_at } => {
            assert_eq!(currency, "USD");
            assert_eq!(closed_at, 200);
        }
        other => panic!("expected PeriodClosed, got: {other}"),
    }
}

#[test]
fn sealed_period_blocks_at_exact_timestamp() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.close_period("USD", 200, retained).unwrap();

    // Attempt at exact closing timestamp
    let err = ledger.record_transaction_at("Exact", &[(cash, 100)], &[(revenue, 100)], 200);
    assert!(err.is_err());
}

#[test]
fn sealed_period_allows_future_entries() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.close_period("USD", 200, retained).unwrap();

    // Timestamp 201 is after the closed period — should succeed
    let result = ledger.record_transaction_at("New sale", &[(cash, 500_00)], &[(revenue, 500_00)], 201);
    assert!(result.is_ok());
    assert_eq!(ledger.get_balance(revenue).unwrap(), 500_00);
}

// ── Nothing to close ────────────────────────────────────────────────

#[test]
fn close_period_returns_none_when_nothing_to_close() {
    let (mut ledger, _, _, retained, _, _) = setup_usd();

    // No Revenue/Expense activity — nothing to close
    let result = ledger.close_period("USD", 100, retained).unwrap();
    assert!(result.is_none());
}

#[test]
fn close_period_returns_none_after_already_zeroed() {
    let (mut ledger, cash, _, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.record_transaction_at("Rent", &[(expense, 1_000_00)], &[(cash, 1_000_00)], 200).unwrap();

    // First close
    ledger.close_period("USD", 300, retained).unwrap();

    // Second close at a later timestamp — nothing to close
    let result = ledger.close_period("USD", 400, retained).unwrap();
    assert!(result.is_none());
}

// ── Invalid retained earnings ───────────────────────────────────────

#[test]
fn close_period_rejects_non_equity_retained_earnings() {
    let (mut ledger, cash, _, _, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();

    // cash is Asset, not Equity
    let err = ledger.close_period("USD", 200, cash).unwrap_err();
    match err {
        LedgerError::InvalidRetainedEarnings { account_id, reason } => {
            assert_eq!(account_id, cash.0);
            assert!(reason.contains("Equity"), "reason: {reason}");
        }
        other => panic!("expected InvalidRetainedEarnings, got: {other}"),
    }
}

#[test]
fn close_period_rejects_wrong_currency_retained_earnings() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD", "1000", AccountType::Asset,   Currency::usd()).unwrap();
    let re_eur   = ledger.create_account("RE EUR",   "3100", AccountType::Equity,  Currency::eur()).unwrap();
    let revenue  = ledger.create_account("Revenue",  "4000", AccountType::Revenue, Currency::usd()).unwrap();

    ledger.record_transaction_at("Sale", &[(cash_usd, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();

    let err = ledger.close_period("USD", 200, re_eur).unwrap_err();
    match err {
        LedgerError::InvalidRetainedEarnings { reason, .. } => {
            assert!(reason.contains("EUR"), "reason: {reason}");
        }
        other => panic!("expected InvalidRetainedEarnings, got: {other}"),
    }
}

#[test]
fn close_period_rejects_nonexistent_account() {
    let (mut ledger, cash, _, _, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();

    let fake_id = kromia_ledger::AccountId(9999);
    let err = ledger.close_period("USD", 200, fake_id).unwrap_err();
    match err {
        LedgerError::AccountNotFound(id) => assert_eq!(id, 9999),
        other => panic!("expected AccountNotFound, got: {other}"),
    }
}

// ── Double-close same timestamp ─────────────────────────────────────

#[test]
fn close_period_rejects_duplicate_close() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.close_period("USD", 200, retained).unwrap();

    // Record more activity after the closed period
    ledger.record_transaction_at("Sale 2", &[(cash, 500_00)], &[(revenue, 500_00)], 300).unwrap();

    // Try to close at the same timestamp again
    let err = ledger.close_period("USD", 200, retained).unwrap_err();
    match err {
        LedgerError::PeriodClosed { closed_at, .. } => assert_eq!(closed_at, 200),
        other => panic!("expected PeriodClosed, got: {other}"),
    }
}

#[test]
fn close_period_rejects_earlier_timestamp() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.close_period("USD", 200, retained).unwrap();

    // Try to close at timestamp 150 (earlier than the already closed 200)
    let err = ledger.close_period("USD", 150, retained).unwrap_err();
    assert!(matches!(err, LedgerError::PeriodClosed { .. }));
}

// ── Multi-currency ──────────────────────────────────────────────────

#[test]
fn close_period_per_currency_independence() {
    let mut ledger = Ledger::new();
    let cash_usd = ledger.create_account("Cash USD",    "1000", AccountType::Asset,   Currency::usd()).unwrap();
    let re_usd   = ledger.create_account("RE USD",      "3100", AccountType::Equity,  Currency::usd()).unwrap();
    let rev_usd  = ledger.create_account("Revenue USD", "4000", AccountType::Revenue, Currency::usd()).unwrap();

    let cash_eur = ledger.create_account("Cash EUR",    "1001", AccountType::Asset,   Currency::eur()).unwrap();
    let re_eur   = ledger.create_account("RE EUR",      "3101", AccountType::Equity,  Currency::eur()).unwrap();
    let rev_eur  = ledger.create_account("Revenue EUR", "4001", AccountType::Revenue, Currency::eur()).unwrap();

    // USD activity
    ledger.record_transaction_at("USD Sale", &[(cash_usd, 3_000_00)], &[(rev_usd, 3_000_00)], 100).unwrap();
    // EUR activity
    ledger.record_transaction_at("EUR Sale", &[(cash_eur, 2_000_00)], &[(rev_eur, 2_000_00)], 100).unwrap();

    // Close USD only
    ledger.close_period("USD", 200, re_usd).unwrap();

    // USD closed
    assert_eq!(ledger.get_balance(rev_usd).unwrap(), 0);
    assert_eq!(ledger.get_balance(re_usd).unwrap(), 3_000_00);

    // EUR not yet closed — revenue still there
    assert_eq!(ledger.get_balance(rev_eur).unwrap(), 2_000_00);

    // EUR entries at timestamp 150 still allowed (only USD is sealed)
    ledger.record_transaction_at("EUR Sale 2", &[(cash_eur, 500_00)], &[(rev_eur, 500_00)], 150).unwrap();

    // USD entries at timestamp 150 blocked
    let err = ledger.record_transaction_at("Late USD", &[(cash_usd, 100)], &[(rev_usd, 100)], 150);
    assert!(matches!(err.unwrap_err(), LedgerError::PeriodClosed { .. }));

    // Now close EUR
    ledger.close_period("EUR", 300, re_eur).unwrap();
    assert_eq!(ledger.get_balance(rev_eur).unwrap(), 0);
    assert_eq!(ledger.get_balance(re_eur).unwrap(), 2_500_00);
}

// ── Audit trail ─────────────────────────────────────────────────────

#[test]
fn close_period_audited_attaches_metadata() {
    let (mut ledger, cash, _, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 2_000_00)], &[(revenue, 2_000_00)], 100).unwrap();
    ledger.record_transaction_at("Rent", &[(expense, 800_00)], &[(cash, 800_00)], 200).unwrap();

    let audit = AuditMeta::new("reyvan")
        .with_source("monthly-close-script")
        .with_notes("Closing January 2026");

    let entry_id = ledger.close_period_audited("USD", 300, retained, audit).unwrap().unwrap();

    // Find the closing entry and verify audit
    let entry = ledger.find_entry(entry_id).unwrap();
    let meta = entry.audit.as_ref().expect("closing entry should have audit");
    assert_eq!(meta.actor, "reyvan");
    assert_eq!(meta.source.as_deref(), Some("monthly-close-script"));
    assert_eq!(meta.notes.as_deref(), Some("Closing January 2026"));
}

// ── closed_periods accessor ─────────────────────────────────────────

#[test]
fn closed_periods_returns_history() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    assert!(ledger.closed_periods().is_empty());

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    ledger.close_period("USD", 200, retained).unwrap();

    assert_eq!(ledger.closed_periods().len(), 1);

    let cp = &ledger.closed_periods()[0];
    assert_eq!(cp.currency, "USD");
    assert_eq!(cp.closed_at, 200);
    assert_eq!(cp.net_income, 1_000_00);
    assert_eq!(cp.retained_earnings_id, retained);
}

// ── Hash chain integrity after closing ──────────────────────────────

#[test]
fn hash_chain_remains_valid_after_closing() {
    let (mut ledger, cash, equity, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Capital", &[(cash, 10_000_00)], &[(equity, 10_000_00)], 100).unwrap();
    ledger.record_transaction_at("Sale",    &[(cash, 3_000_00)],  &[(revenue, 3_000_00)], 200).unwrap();
    ledger.record_transaction_at("Rent",    &[(expense, 500_00)], &[(cash, 500_00)],      300).unwrap();

    let entries_before = ledger.entries().len();
    ledger.close_period("USD", 400, retained).unwrap();
    assert_eq!(ledger.entries().len(), entries_before + 1);

    // The entire chain (including closing entry) should be valid
    assert!(ledger.verify_chain());
}

// ── Serialization round-trip ────────────────────────────────────────

#[test]
fn serialization_preserves_closed_periods() {
    let (mut ledger, cash, _, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 2_000_00)], &[(revenue, 2_000_00)], 100).unwrap();
    ledger.record_transaction_at("Rent", &[(expense, 500_00)], &[(cash, 500_00)], 200).unwrap();
    ledger.close_period("USD", 300, retained).unwrap();

    // Serialize → deserialize
    let json = ledger.save_json().unwrap();
    let restored = Ledger::load_json(&json).unwrap();

    // Closed periods preserved
    assert_eq!(restored.closed_periods().len(), 1);
    assert_eq!(restored.closed_periods()[0].currency, "USD");
    assert_eq!(restored.closed_periods()[0].closed_at, 300);
    assert_eq!(restored.closed_periods()[0].net_income, 1_500_00);

    // Sealed period still enforced after restore — check balances are preserved
    assert_eq!(restored.get_balance(revenue).unwrap(), 0);
    assert_eq!(restored.get_balance(expense).unwrap(), 0);
    assert_eq!(restored.get_balance(retained).unwrap(), 1_500_00);
    assert!(restored.verify_chain());
}

// ── Trial balance after closing ─────────────────────────────────────

#[test]
fn trial_balance_zero_after_closing() {
    let (mut ledger, cash, equity, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Capital", &[(cash, 10_000_00)], &[(equity, 10_000_00)], 100).unwrap();
    ledger.record_transaction_at("Sale",    &[(cash, 3_000_00)],  &[(revenue, 3_000_00)], 200).unwrap();
    ledger.record_transaction_at("Rent",    &[(expense, 500_00)], &[(cash, 500_00)],      300).unwrap();

    // Before close — trial balance should be 0 (it always is in double-entry)
    assert_eq!(ledger.trial_balance(), 0);

    // After close — still 0
    ledger.close_period("USD", 400, retained).unwrap();
    assert_eq!(ledger.trial_balance(), 0);
}

// ── Multiple sequential closes ──────────────────────────────────────

#[test]
fn sequential_closes_accumulate_retained_earnings() {
    let (mut ledger, cash, equity, retained, revenue, expense) = setup_usd();

    ledger.record_transaction_at("Capital", &[(cash, 10_000_00)], &[(equity, 10_000_00)], 100).unwrap();

    // Period 1: Jan
    ledger.record_transaction_at("Jan Sale", &[(cash, 3_000_00)], &[(revenue, 3_000_00)], 200).unwrap();
    ledger.record_transaction_at("Jan Rent", &[(expense, 1_000_00)], &[(cash, 1_000_00)], 250).unwrap();
    ledger.close_period("USD", 300, retained).unwrap();
    assert_eq!(ledger.get_balance(retained).unwrap(), 2_000_00);

    // Period 2: Feb
    ledger.record_transaction_at("Feb Sale", &[(cash, 5_000_00)], &[(revenue, 5_000_00)], 400).unwrap();
    ledger.record_transaction_at("Feb Rent", &[(expense, 1_500_00)], &[(cash, 1_500_00)], 450).unwrap();
    ledger.close_period("USD", 500, retained).unwrap();

    // Retained earnings accumulated: 2000 + 3500 = 5500
    assert_eq!(ledger.get_balance(retained).unwrap(), 5_500_00);

    // All periods sealed
    assert_eq!(ledger.closed_periods().len(), 2);
    assert!(ledger.verify_chain());
}

// ── Closing entry description ───────────────────────────────────────

#[test]
fn closing_entry_has_descriptive_transaction() {
    let (mut ledger, cash, _, retained, revenue, _) = setup_usd();

    ledger.record_transaction_at("Sale", &[(cash, 1_000_00)], &[(revenue, 1_000_00)], 100).unwrap();
    let entry_id = ledger.close_period("USD", 200, retained).unwrap().unwrap();

    let entry = ledger.find_entry(entry_id).unwrap();
    assert!(entry.transaction.description.contains("USD"));
    assert!(entry.transaction.description.contains("200"));
}

// ── Multiple Revenue/Expense accounts ───────────────────────────────

#[test]
fn close_period_handles_multiple_rev_exp_accounts() {
    let mut ledger = Ledger::new();
    let cash     = ledger.create_account("Cash",         "1000", AccountType::Asset,   Currency::usd()).unwrap();
    let retained = ledger.create_account("RE",           "3100", AccountType::Equity,  Currency::usd()).unwrap();
    let rev1     = ledger.create_account("Product Sales","4000", AccountType::Revenue, Currency::usd()).unwrap();
    let rev2     = ledger.create_account("Service Fees", "4100", AccountType::Revenue, Currency::usd()).unwrap();
    let exp1     = ledger.create_account("Salaries",     "5000", AccountType::Expense, Currency::usd()).unwrap();
    let exp2     = ledger.create_account("Utilities",    "5100", AccountType::Expense, Currency::usd()).unwrap();

    ledger.record_transaction_at("Product sale", &[(cash, 5_000_00)], &[(rev1, 5_000_00)], 100).unwrap();
    ledger.record_transaction_at("Service fee",  &[(cash, 2_000_00)], &[(rev2, 2_000_00)], 200).unwrap();
    ledger.record_transaction_at("Salaries",     &[(exp1, 3_000_00)], &[(cash, 3_000_00)], 300).unwrap();
    ledger.record_transaction_at("Utilities",    &[(exp2, 500_00)],   &[(cash, 500_00)],   400).unwrap();

    ledger.close_period("USD", 500, retained).unwrap();

    assert_eq!(ledger.get_balance(rev1).unwrap(), 0);
    assert_eq!(ledger.get_balance(rev2).unwrap(), 0);
    assert_eq!(ledger.get_balance(exp1).unwrap(), 0);
    assert_eq!(ledger.get_balance(exp2).unwrap(), 0);

    // Net income: (5000 + 2000) - (3000 + 500) = 3500
    assert_eq!(ledger.get_balance(retained).unwrap(), 3_500_00);
    assert!(ledger.verify_chain());
}
