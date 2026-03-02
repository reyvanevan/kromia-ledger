//! Period closing ("tutup buku") — zeroes Revenue/Expense into Retained Earnings.
//!
//! At the end of an accounting period (month, quarter, year), Revenue and Expense
//! accounts are "closed" — their balances are transferred to a Retained Earnings
//! (Equity) account via an automatically generated closing entry. After closing,
//! the period is sealed: no transactions with timestamps ≤ the closing timestamp
//! can be recorded.
//!
//! # Accounting Background
//!
//! Revenue and Expense are *temporary* accounts — they accumulate during a period
//! and reset to zero at the end. Asset, Liability, and Equity are *permanent*
//! accounts — their balances carry forward.
//!
//! The closing entry transfers net income (Revenue − Expenses) into Retained
//! Earnings, an Equity account. After closing:
//!
//! - All Revenue account balances = 0
//! - All Expense account balances = 0
//! - Retained Earnings increases by net income (or decreases by net loss)
//! - The balance sheet equation holds: Assets = Liabilities + Equity
//!
//! # Example
//!
//! ```
//! use kromia_ledger::{Ledger, AccountType, Currency};
//!
//! let mut ledger = Ledger::new();
//! let cash    = ledger.create_account("Cash",              "1000", AccountType::Asset,   Currency::usd()).unwrap();
//! let equity  = ledger.create_account("Owner Equity",      "3000", AccountType::Equity,  Currency::usd()).unwrap();
//! let ret_earn = ledger.create_account("Retained Earnings","3100", AccountType::Equity,  Currency::usd()).unwrap();
//! let revenue = ledger.create_account("Sales",             "4000", AccountType::Revenue, Currency::usd()).unwrap();
//! let expense = ledger.create_account("Rent",              "5000", AccountType::Expense, Currency::usd()).unwrap();
//!
//! // Record some activity
//! ledger.record_transaction_at("Capital",   &[(cash, 10_000_00)], &[(equity, 10_000_00)], 100).unwrap();
//! ledger.record_transaction_at("Sale",      &[(cash, 3_000_00)],  &[(revenue, 3_000_00)], 200).unwrap();
//! ledger.record_transaction_at("Rent paid", &[(expense, 500_00)], &[(cash, 500_00)],      300).unwrap();
//!
//! // Close the period — net income $2,500 goes to Retained Earnings
//! let entry_id = ledger.close_period("USD", 400, ret_earn).unwrap();
//! assert!(entry_id.is_some()); // closing entry was created
//!
//! // Revenue and Expense are now zero
//! assert_eq!(ledger.get_balance(revenue).unwrap(), 0);
//! assert_eq!(ledger.get_balance(expense).unwrap(), 0);
//!
//! // Retained Earnings = net income
//! assert_eq!(ledger.get_balance(ret_earn).unwrap(), 3_000_00 - 500_00); // $2,500.00
//!
//! // Period is sealed — cannot record before the closing timestamp
//! let err = ledger.record_transaction_at("Late entry", &[(cash, 100)], &[(revenue, 100)], 350);
//! assert!(err.is_err()); // PeriodClosed
//! ```

use serde::{Deserialize, Serialize};

use crate::account::{AccountId, AccountType, Balance};
use crate::audit::AuditMeta;
use crate::validation::LedgerError;
use crate::Ledger;

/// Record of a closed accounting period.
///
/// Stored in [`Ledger::closed_periods`] to enforce that no new transactions
/// can be recorded with a timestamp at or before the closing timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedPeriod {
    /// The currency that was closed.
    pub currency: String,
    /// The closing timestamp — entries at or before this time are sealed.
    pub closed_at: u64,
    /// The entry ID of the closing journal entry.
    pub closing_entry_id: u64,
    /// Net income transferred to Retained Earnings.
    /// Positive = profit, negative = loss.
    pub net_income: Balance,
    /// The Retained Earnings account that received the net income.
    pub retained_earnings_id: AccountId,
}

impl Ledger {
    /// Close an accounting period for a specific currency.
    ///
    /// This generates a closing journal entry that:
    /// 1. Zeroes all Revenue accounts (debits them for their balance)
    /// 2. Zeroes all Expense accounts (credits them for their balance)
    /// 3. Transfers the net income to the specified Retained Earnings account
    ///
    /// After closing, the period is sealed — [`record_transaction_at`](Self::record_transaction_at)
    /// and other recording methods will reject timestamps ≤ `end_timestamp` for
    /// accounts in this currency.
    ///
    /// Returns `Ok(Some(entry_id))` if a closing entry was created, or
    /// `Ok(None)` if all Revenue/Expense accounts for this currency already
    /// have zero balances (nothing to close).
    ///
    /// # Arguments
    ///
    /// * `currency` — ISO 4217 currency code to close (e.g. `"USD"`)
    /// * `end_timestamp` — The cutoff timestamp. All entries at or before this time are sealed.
    /// * `retained_earnings_id` — An Equity account to receive net income/loss
    ///
    /// # Errors
    ///
    /// | Error | Cause |
    /// |---|---|
    /// | [`LedgerError::AccountNotFound`] | Retained earnings account doesn't exist |
    /// | [`LedgerError::InvalidRetainedEarnings`] | Account is not Equity type or wrong currency |
    /// | [`LedgerError::PeriodClosed`] | A period at or after this timestamp was already closed for this currency |
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::{Ledger, AccountType, Currency};
    ///
    /// let mut ledger = Ledger::new();
    /// let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    /// let re   = ledger.create_account("Retained Earnings", "3100", AccountType::Equity, Currency::usd()).unwrap();
    /// let rev  = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
    ///
    /// ledger.record_transaction_at("Sale", &[(cash, 1000)], &[(rev, 1000)], 100).unwrap();
    /// let result = ledger.close_period("USD", 200, re).unwrap();
    /// assert!(result.is_some());
    /// assert_eq!(ledger.get_balance(rev).unwrap(), 0);
    /// assert_eq!(ledger.get_balance(re).unwrap(), 1000);
    /// ```
    pub fn close_period(
        &mut self,
        currency: &str,
        end_timestamp: u64,
        retained_earnings_id: AccountId,
    ) -> Result<Option<u64>, LedgerError> {
        self.close_period_impl(currency, end_timestamp, retained_earnings_id, None)
    }

    /// Close an accounting period with audit trail metadata.
    ///
    /// Same as [`close_period`](Self::close_period) but attaches an [`AuditMeta`]
    /// to the closing entry, recording who performed the closing and why.
    ///
    /// # Examples
    ///
    /// ```
    /// use kromia_ledger::{Ledger, AccountType, Currency, AuditMeta};
    ///
    /// let mut ledger = Ledger::new();
    /// let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    /// let re   = ledger.create_account("Retained Earnings", "3100", AccountType::Equity, Currency::usd()).unwrap();
    /// let rev  = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
    ///
    /// ledger.record_transaction_at("Sale", &[(cash, 1000)], &[(rev, 1000)], 100).unwrap();
    ///
    /// let audit = AuditMeta::new("reyvan").with_notes("Monthly closing — January 2026");
    /// ledger.close_period_audited("USD", 200, re, audit).unwrap();
    /// assert_eq!(ledger.get_balance(rev).unwrap(), 0);
    /// ```
    pub fn close_period_audited(
        &mut self,
        currency: &str,
        end_timestamp: u64,
        retained_earnings_id: AccountId,
        audit: AuditMeta,
    ) -> Result<Option<u64>, LedgerError> {
        self.close_period_impl(currency, end_timestamp, retained_earnings_id, Some(audit))
    }

    fn close_period_impl(
        &mut self,
        currency: &str,
        end_timestamp: u64,
        retained_earnings_id: AccountId,
        audit: Option<AuditMeta>,
    ) -> Result<Option<u64>, LedgerError> {
        // ── Validate retained earnings account ──────────────────────
        let re_account = self.accounts.get(&retained_earnings_id)
            .ok_or(LedgerError::AccountNotFound(retained_earnings_id.0))?;

        if re_account.account_type != AccountType::Equity {
            return Err(LedgerError::InvalidRetainedEarnings {
                account_id: retained_earnings_id.0,
                reason: format!(
                    "account type is {} but must be Equity",
                    re_account.account_type
                ),
            });
        }

        if re_account.currency.code != currency {
            return Err(LedgerError::InvalidRetainedEarnings {
                account_id: retained_earnings_id.0,
                reason: format!(
                    "account currency is {} but closing currency is {}",
                    re_account.currency.code, currency
                ),
            });
        }

        // ── Check period not already closed ─────────────────────────
        for cp in &self.closed_periods {
            if cp.currency == currency && cp.closed_at >= end_timestamp {
                return Err(LedgerError::PeriodClosed {
                    currency: currency.to_string(),
                    closed_at: cp.closed_at,
                });
            }
        }

        // ── Collect Revenue/Expense balances to close ───────────────
        let mut debits: Vec<(AccountId, Balance)> = Vec::new();
        let mut credits: Vec<(AccountId, Balance)> = Vec::new();

        for acc in self.accounts.values() {
            if acc.currency.code != currency || acc.balance == 0 {
                continue;
            }

            match acc.account_type {
                // Revenue (credit-normal): positive balance → debit to zero
                AccountType::Revenue => {
                    if acc.balance > 0 {
                        debits.push((acc.id, acc.balance));
                    } else {
                        credits.push((acc.id, -acc.balance));
                    }
                }
                // Expense (debit-normal): positive balance → credit to zero
                AccountType::Expense => {
                    if acc.balance > 0 {
                        credits.push((acc.id, acc.balance));
                    } else {
                        debits.push((acc.id, -acc.balance));
                    }
                }
                _ => {} // Skip permanent accounts
            }
        }

        // Nothing to close — all Rev/Exp already zero
        if debits.is_empty() && credits.is_empty() {
            return Ok(None);
        }

        // ── Balance with Retained Earnings ──────────────────────────
        let total_debits: Balance = debits.iter().map(|(_, a)| *a).sum();
        let total_credits: Balance = credits.iter().map(|(_, a)| *a).sum();
        let net_income = total_debits - total_credits;

        if total_debits > total_credits {
            // Profit: credit Retained Earnings
            credits.push((retained_earnings_id, total_debits - total_credits));
        } else if total_credits > total_debits {
            // Loss: debit Retained Earnings
            debits.push((retained_earnings_id, total_credits - total_debits));
        }
        // If equal (rare): no RE entry needed, debits == credits already

        // ── Record closing entry ────────────────────────────────────
        let description = format!("Period closing — {currency} at {end_timestamp}");
        let entry_id = self.record_transaction_impl(
            &description,
            &debits,
            &credits,
            end_timestamp,
            None, // no idempotency key — timestamp + currency is the natural key
            audit,
        )?;

        // ── Seal the period ─────────────────────────────────────────
        self.closed_periods.push(ClosedPeriod {
            currency: currency.to_string(),
            closed_at: end_timestamp,
            closing_entry_id: entry_id,
            net_income,
            retained_earnings_id,
        });

        Ok(Some(entry_id))
    }

    /// Returns all closed periods, most recent first.
    pub fn closed_periods(&self) -> &[ClosedPeriod] {
        &self.closed_periods
    }

    /// Check if a timestamp falls within a closed period for a given currency.
    ///
    /// Used internally by transaction recording methods to reject backdated entries.
    pub(crate) fn is_period_closed(&self, currency: &str, timestamp: u64) -> Option<u64> {
        self.closed_periods.iter()
            .filter(|cp| cp.currency == currency)
            .filter(|cp| timestamp <= cp.closed_at)
            .map(|cp| cp.closed_at)
            .next()
    }
}
