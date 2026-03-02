//! Financial reporting — structured output from ledger data.
//!
//! This module generates four standard accounting reports:
//!
//! - [`TrialBalanceReport`] — all accounts with debit/credit columns
//! - [`BalanceSheet`] — Assets = Liabilities + Equity (point-in-time)
//! - [`IncomeStatement`] — Revenue − Expenses = Net Income (date range)
//! - [`GeneralLedgerReport`] — per-account transaction history with running balance
//!
//! All reports are read-only aggregations over existing ledger data.
//! They implement [`Serialize`] for JSON export.
//!
//! # Example
//!
//! ```
//! use kromia_ledger::{Ledger, AccountType, Currency};
//!
//! let mut ledger = Ledger::new();
//! let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
//! let rev  = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
//! ledger.record_transaction_at("Sale", &[(cash, 500_00)], &[(rev, 500_00)], 100).unwrap();
//!
//! let tb = ledger.trial_balance_report("USD");
//! assert_eq!(tb.total_debit, tb.total_credit);
//! assert_eq!(tb.rows.len(), 2);
//! ```

use serde::Serialize;

use crate::account::{AccountId, AccountType, Balance, Currency};
use crate::Ledger;

// ── Report Structures ───────────────────────────────────────────────

/// A single row in a trial balance or financial statement.
#[derive(Debug, Clone, Serialize)]
pub struct ReportRow {
    pub account_id: AccountId,
    pub account_code: String,
    pub account_name: String,
    pub account_type: AccountType,
    pub currency: Currency,
    /// Positive balance for debit-normal accounts (Asset, Expense).
    pub debit: Balance,
    /// Positive balance for credit-normal accounts (Liability, Equity, Revenue).
    pub credit: Balance,
}

/// Trial balance: all accounts with debit/credit columns.
///
/// In a balanced single-currency ledger, `total_debit == total_credit`.
#[derive(Debug, Clone, Serialize)]
pub struct TrialBalanceReport {
    pub currency_filter: String,
    pub rows: Vec<ReportRow>,
    pub total_debit: Balance,
    pub total_credit: Balance,
}

/// Balance sheet: Assets = Liabilities + Equity (point-in-time snapshot).
#[derive(Debug, Clone, Serialize)]
pub struct BalanceSheet {
    pub currency: String,
    pub as_of: u64,
    pub assets: Vec<ReportRow>,
    pub liabilities: Vec<ReportRow>,
    pub equity: Vec<ReportRow>,
    pub total_assets: Balance,
    pub total_liabilities: Balance,
    pub total_equity: Balance,
    /// `total_liabilities + total_equity` — should equal `total_assets`.
    pub total_liabilities_equity: Balance,
}

/// Income statement: Revenue − Expenses = Net Income (date-range period).
#[derive(Debug, Clone, Serialize)]
pub struct IncomeStatement {
    pub currency: String,
    pub from_ts: u64,
    pub to_ts: u64,
    pub revenue: Vec<ReportRow>,
    pub expenses: Vec<ReportRow>,
    pub total_revenue: Balance,
    pub total_expenses: Balance,
    pub net_income: Balance,
}

/// A single line in a general ledger detail report.
#[derive(Debug, Clone, Serialize)]
pub struct GeneralLedgerLine {
    pub entry_id: u64,
    pub timestamp: u64,
    pub description: String,
    pub debit: Balance,
    pub credit: Balance,
    pub running_balance: Balance,
    pub audit_actor: Option<String>,
}

/// General ledger detail: per-account transaction history with running balance.
#[derive(Debug, Clone, Serialize)]
pub struct GeneralLedgerReport {
    pub account_id: AccountId,
    pub account_code: String,
    pub account_name: String,
    pub account_type: AccountType,
    pub currency: Currency,
    pub lines: Vec<GeneralLedgerLine>,
    pub opening_balance: Balance,
    pub closing_balance: Balance,
    pub from_ts: u64,
    pub to_ts: u64,
}

// ── Helper ──────────────────────────────────────────────────────────

fn account_to_row(acc: &crate::account::Account) -> ReportRow {
    let is_debit_normal = matches!(acc.account_type, AccountType::Asset | AccountType::Expense);
    let (debit, credit) = if is_debit_normal {
        // Debit-normal: positive → debit column, negative → credit column
        if acc.balance >= 0 { (acc.balance, 0) } else { (0, -acc.balance) }
    } else {
        // Credit-normal: positive → credit column, negative → debit column
        if acc.balance >= 0 { (0, acc.balance) } else { (-acc.balance, 0) }
    };
    ReportRow {
        account_id: acc.id,
        account_code: acc.code.clone(),
        account_name: acc.name.clone(),
        account_type: acc.account_type,
        currency: acc.currency.clone(),
        debit,
        credit,
    }
}

// ── Ledger report methods ───────────────────────────────────────────

impl Ledger {
    /// Generate a trial balance report for a specific currency.
    ///
    /// Lists all accounts of the given currency with their debit/credit balances.
    /// In a balanced ledger, `total_debit == total_credit`.
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::{Ledger, AccountType, Currency};
    ///
    /// let mut ledger = Ledger::new();
    /// let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    /// let rev  = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
    /// ledger.record_transaction_at("Sale", &[(cash, 100_00)], &[(rev, 100_00)], 100).unwrap();
    ///
    /// let tb = ledger.trial_balance_report("USD");
    /// assert_eq!(tb.total_debit, 100_00);
    /// assert_eq!(tb.total_credit, 100_00);
    /// ```
    pub fn trial_balance_report(&self, currency: &str) -> TrialBalanceReport {
        let rows: Vec<ReportRow> = self.accounts.values()
            .filter(|a| a.currency.code == currency)
            .map(account_to_row)
            .collect();

        let total_debit: Balance = rows.iter().map(|r| r.debit).sum();
        let total_credit: Balance = rows.iter().map(|r| r.credit).sum();

        TrialBalanceReport {
            currency_filter: currency.to_string(),
            rows,
            total_debit,
            total_credit,
        }
    }

    /// Generate a balance sheet for a specific currency at a point in time.
    ///
    /// Groups accounts into Assets, Liabilities, and Equity.
    /// The accounting equation must hold: **Assets = Liabilities + Equity**.
    ///
    /// Note: `as_of` is stored for metadata but current balances are used
    /// (the ledger doesn't yet support historical point-in-time snapshots).
    pub fn balance_sheet(&self, currency: &str, as_of: u64) -> BalanceSheet {
        let mut assets = Vec::new();
        let mut liabilities = Vec::new();
        let mut equity = Vec::new();

        for acc in self.accounts.values().filter(|a| a.currency.code == currency) {
            let row = account_to_row(acc);
            match acc.account_type {
                AccountType::Asset => assets.push(row),
                AccountType::Liability => liabilities.push(row),
                AccountType::Equity => equity.push(row),
                _ => {} // Revenue/Expense not on balance sheet
            }
        }

        let total_assets: Balance = assets.iter().map(|r| r.debit).sum();
        let total_liabilities: Balance = liabilities.iter().map(|r| r.credit).sum();
        let total_equity: Balance = equity.iter().map(|r| r.credit).sum();

        BalanceSheet {
            currency: currency.to_string(),
            as_of,
            assets,
            liabilities,
            equity,
            total_assets,
            total_liabilities,
            total_equity,
            total_liabilities_equity: total_liabilities + total_equity,
        }
    }

    /// Generate an income statement for a specific currency over a date range.
    ///
    /// Computes revenue and expenses from entries within `[from_ts, to_ts]`,
    /// then calculates **Net Income = Revenue − Expenses**.
    ///
    /// This scans entries in the given range and sums debit/credit per account,
    /// rather than using the running balance (which may include transactions
    /// outside the period).
    pub fn income_statement(&self, currency: &str, from_ts: u64, to_ts: u64) -> IncomeStatement {
        // Accumulate per-account totals from entries in the date range
        let mut account_debits: std::collections::BTreeMap<AccountId, Balance> = std::collections::BTreeMap::new();
        let mut account_credits: std::collections::BTreeMap<AccountId, Balance> = std::collections::BTreeMap::new();

        for entry in self.entries.iter().filter(|e| e.timestamp >= from_ts && e.timestamp <= to_ts) {
            for line in &entry.transaction.lines {
                // Only include accounts matching the currency
                if let Some(acc) = self.accounts.get(&line.account_id) {
                    if acc.currency.code != currency {
                        continue;
                    }
                    match acc.account_type {
                        AccountType::Revenue | AccountType::Expense => {
                            *account_debits.entry(line.account_id).or_insert(0) += line.debit;
                            *account_credits.entry(line.account_id).or_insert(0) += line.credit;
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut revenue = Vec::new();
        let mut expenses = Vec::new();

        // Collect unique account IDs (avoids duplicate iteration from chaining two maps)
        let all_ids: std::collections::BTreeSet<AccountId> =
            account_debits.keys().chain(account_credits.keys()).copied().collect();

        for acc_id in all_ids {
            let acc = match self.accounts.get(&acc_id) {
                Some(a) => a,
                None => continue,
            };
            let d = account_debits.get(&acc_id).copied().unwrap_or(0);
            let c = account_credits.get(&acc_id).copied().unwrap_or(0);
            // Net amount for this account in the period
            let net = match acc.account_type {
                AccountType::Revenue => c - d,  // credit-normal
                AccountType::Expense => d - c,  // debit-normal
                _ => continue,
            };

            let row = ReportRow {
                account_id: acc_id,
                account_code: acc.code.clone(),
                account_name: acc.name.clone(),
                account_type: acc.account_type,
                currency: acc.currency.clone(),
                debit: if acc.account_type == AccountType::Expense { net } else { 0 },
                credit: if acc.account_type == AccountType::Revenue { net } else { 0 },
            };

            match acc.account_type {
                AccountType::Revenue => revenue.push(row),
                AccountType::Expense => expenses.push(row),
                _ => {}
            }
        }

        let total_revenue: Balance = revenue.iter().map(|r| r.credit).sum();
        let total_expenses: Balance = expenses.iter().map(|r| r.debit).sum();

        IncomeStatement {
            currency: currency.to_string(),
            from_ts,
            to_ts,
            revenue,
            expenses,
            total_revenue,
            total_expenses,
            net_income: total_revenue - total_expenses,
        }
    }

    /// Generate a general ledger detail report for a single account over a date range.
    ///
    /// Shows every transaction that touched this account, with a running balance
    /// computed from the opening balance at `from_ts`.
    ///
    /// # Returns
    ///
    /// `None` if the account does not exist.
    pub fn general_ledger(
        &self,
        account_id: AccountId,
        from_ts: u64,
        to_ts: u64,
    ) -> Option<GeneralLedgerReport> {
        let acc = self.accounts.get(&account_id)?;

        // Compute opening balance: sum all transactions before from_ts
        let mut opening = 0_i128;
        for entry in self.entries.iter().filter(|e| e.timestamp < from_ts) {
            for line in &entry.transaction.lines {
                if line.account_id == account_id {
                    opening += line_effect(acc, line);
                }
            }
        }

        let mut running = opening;
        let mut lines = Vec::new();

        for entry in self.entries.iter().filter(|e| e.timestamp >= from_ts && e.timestamp <= to_ts) {
            for line in &entry.transaction.lines {
                if line.account_id == account_id {
                    let effect = line_effect(acc, line);
                    running += effect;

                    lines.push(GeneralLedgerLine {
                        entry_id: entry.id,
                        timestamp: entry.timestamp,
                        description: entry.transaction.description.clone(),
                        debit: line.debit,
                        credit: line.credit,
                        running_balance: running,
                        audit_actor: entry.audit.as_ref().map(|a| a.actor.clone()),
                    });
                }
            }
        }

        Some(GeneralLedgerReport {
            account_id,
            account_code: acc.code.clone(),
            account_name: acc.name.clone(),
            account_type: acc.account_type,
            currency: acc.currency.clone(),
            lines,
            opening_balance: opening,
            closing_balance: running,
            from_ts,
            to_ts,
        })
    }

}

/// Compute the balance effect of a single transaction line on an account.
fn line_effect(acc: &crate::account::Account, line: &crate::transaction::TransactionLine) -> Balance {
    match acc.account_type {
        // Debit-normal: debit increases, credit decreases
        AccountType::Asset | AccountType::Expense => line.debit - line.credit,
        // Credit-normal: credit increases, debit decreases
        _ => line.credit - line.debit,
    }
}
