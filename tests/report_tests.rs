//! Integration tests for financial reporting (Phase 7).
#![allow(clippy::inconsistent_digit_grouping)]

use kromia_ledger::{
    AccountType, AuditMeta, Currency, Ledger,
};

/// Helper: create a ledger with typical chart of accounts + transactions.
fn sample_ledger() -> Ledger {
    let mut l = Ledger::new();

    let cash     = l.create_account("Cash",     "1000", AccountType::Asset,     Currency::usd()).unwrap();
    let ar       = l.create_account("AR",       "1200", AccountType::Asset,     Currency::usd()).unwrap();
    let ap       = l.create_account("AP",       "2000", AccountType::Liability, Currency::usd()).unwrap();
    let equity   = l.create_account("Equity",   "3000", AccountType::Equity,    Currency::usd()).unwrap();
    let revenue  = l.create_account("Revenue",  "4000", AccountType::Revenue,   Currency::usd()).unwrap();
    let expense  = l.create_account("Expenses", "5000", AccountType::Expense,   Currency::usd()).unwrap();

    // t=100: owner invests $10,000
    l.record_transaction_at("Owner investment", &[(cash, 1_000_000)], &[(equity, 1_000_000)], 100).unwrap();

    // t=200: sell service on credit $3,000
    l.record_transaction_at("Service sale", &[(ar, 300_000)], &[(revenue, 300_000)], 200).unwrap();

    // t=300: pay expenses $1,200
    l.record_transaction_at("Office rent", &[(expense, 120_000)], &[(cash, 120_000)], 300).unwrap();

    // t=400: collect AR $3,000
    l.record_transaction_at("AR collection", &[(cash, 300_000)], &[(ar, 300_000)], 400).unwrap();

    // t=500: incur payable $800
    l.record_transaction_at("Supplier bill", &[(expense, 80_000)], &[(ap, 80_000)], 500).unwrap();

    let _ = (cash, ar, ap, equity, revenue, expense); // suppress unused-variable warnings
    l
}

// ── Trial Balance ───────────────────────────────────────────────────

#[test]
fn trial_balance_report_balances() {
    let l = sample_ledger();
    let tb = l.trial_balance_report("USD");

    assert_eq!(tb.total_debit, tb.total_credit, "trial balance must balance");
    assert_eq!(tb.rows.len(), 6, "6 USD accounts");
    assert_eq!(tb.currency_filter, "USD");
}

#[test]
fn trial_balance_report_ignores_other_currencies() {
    let mut l = sample_ledger();
    let eur_cash = l.create_account("EUR Cash", "1001", AccountType::Asset, Currency::new("EUR", 2)).unwrap();
    let eur_eq   = l.create_account("EUR Equity", "3001", AccountType::Equity, Currency::new("EUR", 2)).unwrap();
    l.record_transaction_at("EUR injection", &[(eur_cash, 500_00)], &[(eur_eq, 500_00)], 600).unwrap();

    let tb_usd = l.trial_balance_report("USD");
    assert_eq!(tb_usd.rows.len(), 6, "still 6 USD accounts");

    let tb_eur = l.trial_balance_report("EUR");
    assert_eq!(tb_eur.rows.len(), 2);
    assert_eq!(tb_eur.total_debit, tb_eur.total_credit);
}

// ── Balance Sheet ───────────────────────────────────────────────────

#[test]
fn balance_sheet_equation() {
    let l = sample_ledger();
    let bs = l.balance_sheet("USD", 999);

    // Before closing entries, the accounting equation is:
    //   Assets = Liabilities + Equity + (Revenue - Expenses)
    // The balance sheet only shows A, L, E — the gap is net income.
    let is = l.income_statement("USD", 0, 999);
    assert_eq!(
        bs.total_assets,
        bs.total_liabilities_equity + is.net_income,
        "A = L + E + Net Income (pre-closing)"
    );
    assert_eq!(bs.currency, "USD");
    assert_eq!(bs.as_of, 999);
}

#[test]
fn balance_sheet_excludes_revenue_expense() {
    let l = sample_ledger();
    let bs = l.balance_sheet("USD", 999);

    let all_codes: Vec<&str> = bs.assets.iter()
        .chain(&bs.liabilities)
        .chain(&bs.equity)
        .map(|r| r.account_code.as_str())
        .collect();

    assert!(!all_codes.contains(&"4000"), "revenue not on balance sheet");
    assert!(!all_codes.contains(&"5000"), "expense not on balance sheet");
}

#[test]
fn balance_sheet_amounts() {
    let l = sample_ledger();
    let bs = l.balance_sheet("USD", 999);

    // Cash = 10,000 - 1,200 + 3,000 = 11,800 → 1_180_000
    // AR   = 3,000 - 3,000 = 0
    // Total Assets = 1_180_000
    assert_eq!(bs.total_assets, 1_180_000);

    // AP = 800 → 80_000
    assert_eq!(bs.total_liabilities, 80_000);

    // Equity = 10,000 → 1_000_000
    assert_eq!(bs.total_equity, 1_000_000);

    // But wait: Revenue(3000) - Expense(2000) = 1000 net income
    // is NOT closed into equity yet → A = 1_180_000, L+E = 1_080_000
    // The difference is the undistributed net income ($1,800)
    // This is correct for a pre-closing trial balance.
    // Let's recalculate:
    // Assets: Cash 1_180_000 + AR 0 = 1_180_000
    // Liabilities: AP 80_000
    // Equity: 1_000_000
    // L+E: 1_080_000
    // Difference = 100_000 (the net income not yet closed)
    // This is by design — balance sheet won't balance until closing entries.
}

// ── Income Statement ────────────────────────────────────────────────

#[test]
fn income_statement_full_range() {
    let l = sample_ledger();
    let is = l.income_statement("USD", 0, 999);

    // Revenue = $3,000 → 300_000
    assert_eq!(is.total_revenue, 300_000, "total revenue");

    // Expenses = $1,200 + $800 = $2,000 → 200_000
    assert_eq!(is.total_expenses, 200_000, "total expenses");

    // Net income = Revenue - Expenses = $1,000 → 100_000
    assert_eq!(is.net_income, 100_000, "net income");
    assert_eq!(is.currency, "USD");
}

#[test]
fn income_statement_partial_range() {
    let l = sample_ledger();
    // Only t=200 (service sale) in range
    let is = l.income_statement("USD", 150, 250);

    assert_eq!(is.total_revenue, 300_000);
    assert_eq!(is.total_expenses, 0);
    assert_eq!(is.net_income, 300_000);
    assert_eq!(is.revenue.len(), 1);
    assert_eq!(is.expenses.len(), 0);
}

#[test]
fn income_statement_empty_range() {
    let l = sample_ledger();
    let is = l.income_statement("USD", 9000, 9999);

    assert_eq!(is.total_revenue, 0);
    assert_eq!(is.total_expenses, 0);
    assert_eq!(is.net_income, 0);
}

// ── General Ledger ──────────────────────────────────────────────────

#[test]
fn general_ledger_cash_account() {
    let l = sample_ledger();
    let cash_id = l.account_by_code("1000").unwrap().id;
    let gl = l.general_ledger(cash_id, 0, 999).unwrap();

    assert_eq!(gl.account_code, "1000");
    assert_eq!(gl.lines.len(), 3, "3 entries touch cash");
    assert_eq!(gl.opening_balance, 0, "no entries before t=0");

    // Cash closing = 10,000 - 1,200 + 3,000 = 11,800 → 1_180_000
    assert_eq!(gl.closing_balance, 1_180_000);

    // Running balances
    assert_eq!(gl.lines[0].running_balance, 1_000_000); // +10,000
    assert_eq!(gl.lines[1].running_balance, 880_000);    // -1,200
    assert_eq!(gl.lines[2].running_balance, 1_180_000);  // +3,000
}

#[test]
fn general_ledger_with_opening_balance() {
    let l = sample_ledger();
    let cash_id = l.account_by_code("1000").unwrap().id;

    // Range starts at 250 → opening = transactions at t=100 only
    let gl = l.general_ledger(cash_id, 250, 999).unwrap();

    assert_eq!(gl.opening_balance, 1_000_000, "opening = $10,000 from t=100");
    assert_eq!(gl.lines.len(), 2, "entries at t=300 and t=400");
    assert_eq!(gl.closing_balance, 1_180_000);
}

#[test]
fn general_ledger_nonexistent_account() {
    let l = sample_ledger();
    let fake_id = kromia_ledger::AccountId(999);
    assert!(l.general_ledger(fake_id, 0, 999).is_none());
}

#[test]
fn general_ledger_audit_actor_propagated() {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let rev  = l.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

    let audit = AuditMeta::new("reyvan").with_source("test");
    l.record_transaction_audited("Audited sale", &[(cash, 100_00)], &[(rev, 100_00)], 100, None, audit).unwrap();

    let gl = l.general_ledger(cash, 0, 999).unwrap();
    assert_eq!(gl.lines.len(), 1);
    assert_eq!(gl.lines[0].audit_actor.as_deref(), Some("reyvan"));
}

// ── Serialization ───────────────────────────────────────────────────

#[test]
fn reports_serialize_to_json() {
    let l = sample_ledger();

    let tb_json = serde_json::to_string(&l.trial_balance_report("USD")).unwrap();
    assert!(tb_json.contains("total_debit"));

    let bs_json = serde_json::to_string(&l.balance_sheet("USD", 999)).unwrap();
    assert!(bs_json.contains("total_assets"));

    let is_json = serde_json::to_string(&l.income_statement("USD", 0, 999)).unwrap();
    assert!(is_json.contains("net_income"));

    let cash_id = l.account_by_code("1000").unwrap().id;
    let gl_json = serde_json::to_string(&l.general_ledger(cash_id, 0, 999).unwrap()).unwrap();
    assert!(gl_json.contains("running_balance"));
}
