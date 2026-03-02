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

// ── Regression: 3+ revenue/expense accounts (dedup bug) ────────────

#[test]
fn income_statement_many_revenue_accounts_no_duplicates() {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let rev_a = l.create_account("Sales", "4000", AccountType::Revenue, Currency::usd()).unwrap();
    let rev_b = l.create_account("Interest", "4100", AccountType::Revenue, Currency::usd()).unwrap();
    let rev_c = l.create_account("Fees", "4200", AccountType::Revenue, Currency::usd()).unwrap();
    let exp_a = l.create_account("Rent", "5000", AccountType::Expense, Currency::usd()).unwrap();
    let exp_b = l.create_account("Salaries", "5100", AccountType::Expense, Currency::usd()).unwrap();
    let exp_c = l.create_account("Utilities", "5200", AccountType::Expense, Currency::usd()).unwrap();

    l.record_transaction_at("Sales", &[(cash, 100_00)], &[(rev_a, 100_00)], 100).unwrap();
    l.record_transaction_at("Interest", &[(cash, 20_00)], &[(rev_b, 20_00)], 200).unwrap();
    l.record_transaction_at("Fee income", &[(cash, 5_00)], &[(rev_c, 5_00)], 300).unwrap();
    l.record_transaction_at("Rent", &[(exp_a, 40_00)], &[(cash, 40_00)], 400).unwrap();
    l.record_transaction_at("Salaries", &[(exp_b, 30_00)], &[(cash, 30_00)], 500).unwrap();
    l.record_transaction_at("Utilities", &[(exp_c, 10_00)], &[(cash, 10_00)], 600).unwrap();

    let is = l.income_statement("USD", 0, 999);

    // Must have exactly 3 revenue rows, not 6 (dedup regression)
    assert_eq!(is.revenue.len(), 3, "exactly 3 revenue accounts, no duplicates");
    assert_eq!(is.expenses.len(), 3, "exactly 3 expense accounts, no duplicates");

    assert_eq!(is.total_revenue, 125_00);  // 100 + 20 + 5
    assert_eq!(is.total_expenses, 80_00);  // 40 + 30 + 10
    assert_eq!(is.net_income, 45_00);       // 125 - 80
}

// ── Negative balance (contra account / overdraft) ───────────────────

#[test]
fn trial_balance_negative_asset_flips_to_credit() {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let bank = l.create_account("Bank", "1100", AccountType::Asset, Currency::usd()).unwrap();
    let eq   = l.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();

    // Invest $500 into cash
    l.record_transaction_at("Invest", &[(cash, 500_00)], &[(eq, 500_00)], 100).unwrap();
    // Transfer $800 from cash to bank → cash goes to -$300
    l.record_transaction_at("Transfer", &[(bank, 800_00)], &[(cash, 800_00)], 200).unwrap();

    let tb = l.trial_balance_report("USD");

    // Cash balance = -300_00 → should appear in CREDIT column, not negative debit
    let cash_row = tb.rows.iter().find(|r| r.account_code == "1000").unwrap();
    assert_eq!(cash_row.debit, 0, "negative asset should NOT be in debit column");
    assert_eq!(cash_row.credit, 300_00, "negative asset should flip to credit column");

    // Bank balance = 800_00 → normal debit
    let bank_row = tb.rows.iter().find(|r| r.account_code == "1100").unwrap();
    assert_eq!(bank_row.debit, 800_00);
    assert_eq!(bank_row.credit, 0);

    // Trial balance still balances
    assert_eq!(tb.total_debit, tb.total_credit, "TB must balance");
}

#[test]
fn balance_sheet_negative_liability_flips_to_debit() {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let ap   = l.create_account("AP", "2000", AccountType::Liability, Currency::usd()).unwrap();
    let eq   = l.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();

    // Invest
    l.record_transaction_at("Invest", &[(cash, 1000_00)], &[(eq, 1000_00)], 100).unwrap();
    // Incur payable $200
    l.record_transaction_at("Bill", &[(cash, 200_00)], &[(ap, 200_00)], 200).unwrap();
    // Overpay $500 → AP goes to -$300
    l.record_transaction_at("Overpay", &[(ap, 500_00)], &[(cash, 500_00)], 300).unwrap();

    let bs = l.balance_sheet("USD", 999);

    // AP balance = -300 → negative liability should flip to debit side
    let ap_row = bs.liabilities.iter().find(|r| r.account_code == "2000").unwrap();
    assert_eq!(ap_row.credit, 0, "negative liability should NOT be in credit column");
    assert_eq!(ap_row.debit, 300_00, "negative liability should flip to debit column");
}

// ── Zero-balance accounts ───────────────────────────────────────────

#[test]
fn trial_balance_includes_zero_balance_accounts() {
    let mut l = Ledger::new();
    let _cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let _eq   = l.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();
    // No transactions — both accounts have zero balance

    let tb = l.trial_balance_report("USD");
    assert_eq!(tb.rows.len(), 2, "zero-balance accounts still appear");
    assert_eq!(tb.total_debit, 0);
    assert_eq!(tb.total_credit, 0);
}

// ── General ledger for credit-normal account ────────────────────────

#[test]
fn general_ledger_credit_normal_account() {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let rev  = l.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

    l.record_transaction_at("Sale 1", &[(cash, 100_00)], &[(rev, 100_00)], 100).unwrap();
    l.record_transaction_at("Refund", &[(rev, 30_00)], &[(cash, 30_00)], 200).unwrap();
    l.record_transaction_at("Sale 2", &[(cash, 50_00)], &[(rev, 50_00)], 300).unwrap();

    let gl = l.general_ledger(rev, 0, 999).unwrap();
    assert_eq!(gl.lines.len(), 3);
    // Revenue is credit-normal: credit increases, debit decreases
    assert_eq!(gl.lines[0].running_balance, 100_00);  // +100 credit
    assert_eq!(gl.lines[1].running_balance, 70_00);    // -30 debit (refund)
    assert_eq!(gl.lines[2].running_balance, 120_00);  // +50 credit
    assert_eq!(gl.closing_balance, 120_00);
}

// ── Deactivated accounts still appear in reports ────────────────────

#[test]
fn deactivated_accounts_appear_in_reports() {
    let mut l = Ledger::new();
    let cash = l.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let eq   = l.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();

    l.record_transaction_at("Invest", &[(cash, 500_00)], &[(eq, 500_00)], 100).unwrap();
    l.deactivate_account(cash).unwrap();

    // Deactivated accounts must still appear — they have balances
    let tb = l.trial_balance_report("USD");
    assert_eq!(tb.rows.len(), 2);
    assert_eq!(tb.total_debit, tb.total_credit);

    let bs = l.balance_sheet("USD", 999);
    assert_eq!(bs.assets.len(), 1);
    assert_eq!(bs.total_assets, 500_00);
}
