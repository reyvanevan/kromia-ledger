//! WebAssembly bindings for the Kromia Ledger engine.
//!
//! This module provides a [`WasmLedger`] wrapper that exposes the core ledger
//! functionality to JavaScript/TypeScript via `wasm-bindgen`. Account types
//! are passed as integers (0–4) and transactions are submitted as JSON strings.
//! All complex return types are serialized to JSON strings for JS consumption.
//!
//! This module is only compiled when targeting `wasm32`.
//!
//! ## Return Conventions
//!
//! | Rust type | JS return | Notes |
//! |---|---|---|
//! | `u64` / `i64` | `number` | Entry IDs, balances |
//! | `bool` | `boolean` | `verify_chain()` |
//! | `usize` | `number` | `entry_count()` |
//! | Struct (Serialize) | `string` (JSON) | Parse with `JSON.parse()` in JS |
//! | `Result<T, E>` | throws on error | Error message as string |

use wasm_bindgen::prelude::*;

use crate::types::{AccountType, Balance, Currency};
use crate::Ledger;

// ── Helpers ─────────────────────────────────────────────────────────

/// Serialize any `Serialize` value to a JSON string, mapping errors to `JsValue`.
fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, JsValue> {
    serde_json::to_string(value).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Map a `LedgerError` to a `JsValue` error string.
fn ledger_err(e: crate::validation::LedgerError) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn not_found(msg: &str) -> JsValue {
    JsValue::from_str(msg)
}

fn parse_account_type(n: u8) -> Result<AccountType, JsValue> {
    match n {
        0 => Ok(AccountType::Asset),
        1 => Ok(AccountType::Liability),
        2 => Ok(AccountType::Equity),
        3 => Ok(AccountType::Revenue),
        4 => Ok(AccountType::Expense),
        _ => Err(JsValue::from_str("invalid account type (0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense)")),
    }
}

fn parse_lines(value: &serde_json::Value) -> Result<Vec<(crate::types::AccountId, Balance)>, String> {
    let arr = value.as_array().ok_or("expected array")?;
    let mut lines = Vec::with_capacity(arr.len());
    for item in arr {
        let pair = item.as_array().ok_or("expected [id, amount]")?;
        if pair.len() != 2 {
            return Err("each line must be [id, amount]".to_string());
        }
        let id = pair[0].as_u64().ok_or("id must be u64")?;
        let amount = pair[1].as_i64().ok_or("amount must be i64")? as Balance;
        lines.push((crate::types::AccountId(id), amount));
    }
    Ok(lines)
}

fn parse_audit(value: &serde_json::Value) -> Option<crate::audit::AuditMeta> {
    let actor = value.get("actor")?.as_str()?;
    let mut audit = crate::audit::AuditMeta::new(actor);
    if let Some(source) = value.get("source").and_then(|v| v.as_str()) {
        audit = audit.with_source(source);
    }
    if let Some(notes) = value.get("notes").and_then(|v| v.as_str()) {
        audit = audit.with_notes(notes);
    }
    Some(audit)
}

// ── WasmLedger ──────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct WasmLedger {
    inner: Ledger,
}

#[wasm_bindgen]
impl WasmLedger {
    // ── Constructor & Persistence ───────────────────────────────────

    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { inner: Ledger::new() }
    }

    /// Serialize the entire ledger to a JSON string.
    pub fn save_json(&self) -> Result<String, JsValue> {
        self.inner.save_json().map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Restore a ledger from a JSON string (with hash-chain verification).
    pub fn load_json(json: &str) -> Result<WasmLedger, JsValue> {
        let inner = Ledger::load_json(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self { inner })
    }

    // ── Account Management ──────────────────────────────────────────

    /// Create an account.
    /// `account_type`: 0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense.
    /// Returns the new account's numeric ID.
    pub fn create_account(
        &mut self,
        name: &str,
        code: &str,
        account_type: u8,
        currency_code: &str,
        currency_precision: u8,
    ) -> Result<u64, JsValue> {
        let at = parse_account_type(account_type)?;
        let currency = Currency::new(currency_code, currency_precision);
        self.inner.create_account(name, code, at, currency)
            .map(|id| id.0)
            .map_err(ledger_err)
    }

    /// Soft-deactivate an account by ID.
    pub fn deactivate_account(&mut self, id: u64) -> Result<(), JsValue> {
        self.inner.deactivate_account(crate::types::AccountId(id))
            .map_err(ledger_err)
    }

    /// Get a single account by ID. Returns JSON: `{ id, name, code, account_type, currency, balance, active }`.
    pub fn get_account(&self, id: u64) -> Result<String, JsValue> {
        let acc = self.inner.get_account(crate::types::AccountId(id))
            .ok_or_else(|| not_found("account not found"))?;
        to_json(acc)
    }

    /// Get a single account by chart-of-accounts code. Returns JSON or throws if not found.
    pub fn account_by_code(&self, code: &str) -> Result<String, JsValue> {
        let acc = self.inner.account_by_code(code)
            .ok_or_else(|| not_found("account not found"))?;
        to_json(acc)
    }

    /// Returns all accounts as a JSON array.
    pub fn get_accounts(&self) -> Result<String, JsValue> {
        let accounts: Vec<_> = self.inner.accounts().collect();
        to_json(&accounts)
    }

    /// Returns the raw balance in the smallest currency unit (e.g. cents for USD).
    /// JavaScript can safely handle integers up to 2^53 (~90 trillion).
    pub fn get_balance(&self, id: u64) -> Result<i64, JsValue> {
        self.inner.get_balance(crate::types::AccountId(id))
            .map(|b| b as i64)
            .ok_or_else(|| not_found("account not found"))
    }

    /// Returns the balance formatted as a human-readable string.
    /// Example: `"1,234.56"` for USD, `"1,000,000"` for IDR.
    pub fn get_balance_formatted(&self, id: u64) -> Result<String, JsValue> {
        let acc = self.inner.get_account(crate::types::AccountId(id))
            .ok_or_else(|| not_found("account not found"))?;
        Ok(crate::format::format_amount(acc.balance, acc.currency.precision))
    }

    // ── Transaction Recording ───────────────────────────────────────

    /// Record a transaction via JSON. Returns the new entry ID.
    ///
    /// JSON format:
    /// ```json
    /// {
    ///   "description": "Sale",
    ///   "debits": [[1, 100000]],
    ///   "credits": [[3, 100000]],
    ///   "timestamp": 1709337600,
    ///   "idempotency_key": "ORDER-001",
    ///   "audit": { "actor": "reyvan", "source": "web", "notes": "..." }
    /// }
    /// ```
    /// Fields `timestamp`, `idempotency_key`, and `audit` are optional.
    pub fn record_transaction(&mut self, json: &str) -> Result<u64, JsValue> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let description = v["description"].as_str().unwrap_or("");
        let debits = parse_lines(&v["debits"]).map_err(|e| JsValue::from_str(&e))?;
        let credits = parse_lines(&v["credits"]).map_err(|e| JsValue::from_str(&e))?;
        let timestamp = v["timestamp"].as_u64();
        let idempotency_key = v["idempotency_key"].as_str();

        let ts = timestamp.unwrap_or_else(crate::types::current_timestamp);
        match parse_audit(&v["audit"]) {
            Some(audit) => self.inner.record_transaction_audited(
                description, &debits, &credits, ts, idempotency_key, audit,
            ),
            None => self.inner.record_transaction_full(
                description, &debits, &credits, ts, idempotency_key,
            ),
        }.map_err(ledger_err)
    }

    /// Record a cross-currency exchange via JSON. Returns the new entry ID.
    ///
    /// JSON format:
    /// ```json
    /// {
    ///   "description": "USD to IDR",
    ///   "from_account": 1, "from_amount": 50000,
    ///   "to_account": 2, "to_amount": 785025000,
    ///   "exchange_rate": 15700500000,
    ///   "timestamp": 1709337600,
    ///   "idempotency_key": "XCH-001",
    ///   "audit": { "actor": "treasury" }
    /// }
    /// ```
    pub fn record_exchange(&mut self, json: &str) -> Result<u64, JsValue> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let description = v["description"].as_str().unwrap_or("");
        let from_account = v["from_account"].as_u64()
            .ok_or_else(|| JsValue::from_str("from_account must be u64"))?;
        let from_amount = v["from_amount"].as_i64()
            .ok_or_else(|| JsValue::from_str("from_amount must be i64"))? as Balance;
        let to_account = v["to_account"].as_u64()
            .ok_or_else(|| JsValue::from_str("to_account must be u64"))?;
        let to_amount = v["to_amount"].as_i64()
            .ok_or_else(|| JsValue::from_str("to_amount must be i64"))? as Balance;
        let exchange_rate = v["exchange_rate"].as_i64()
            .ok_or_else(|| JsValue::from_str("exchange_rate must be i64"))? as Balance;
        let timestamp = v["timestamp"].as_u64();
        let idempotency_key = v["idempotency_key"].as_str();

        let ts = timestamp.unwrap_or_else(crate::types::current_timestamp);
        match parse_audit(&v["audit"]) {
            Some(audit) => self.inner.record_exchange_audited(
                description,
                crate::types::AccountId(from_account), from_amount,
                crate::types::AccountId(to_account), to_amount,
                exchange_rate, ts, idempotency_key, audit,
            ),
            None => self.inner.record_exchange_full(
                description,
                crate::types::AccountId(from_account), from_amount,
                crate::types::AccountId(to_account), to_amount,
                exchange_rate, ts, idempotency_key,
            ),
        }.map_err(ledger_err)
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Returns all ledger entries as a JSON array.
    pub fn get_entries(&self) -> Result<String, JsValue> {
        to_json(self.inner.entries())
    }

    /// Returns a single entry by ID as JSON, or throws if not found.
    pub fn find_entry(&self, id: u64) -> Result<String, JsValue> {
        let entry = self.inner.find_entry(id)
            .ok_or_else(|| not_found("entry not found"))?;
        to_json(entry)
    }

    /// Returns all entries involving a specific account as a JSON array.
    pub fn entries_for_account(&self, account_id: u64) -> Result<String, JsValue> {
        let entries = self.inner.entries_for_account(crate::types::AccountId(account_id));
        to_json(&entries)
    }

    /// Returns entries within a timestamp range (inclusive) as a JSON array.
    pub fn entries_in_range(&self, from_ts: u64, to_ts: u64) -> Result<String, JsValue> {
        let entries = self.inner.entries_in_range(from_ts, to_ts);
        to_json(&entries)
    }

    /// Returns entries recorded by a specific actor as a JSON array.
    pub fn entries_by_actor(&self, actor: &str) -> Result<String, JsValue> {
        let entries = self.inner.entries_by_actor(actor);
        to_json(&entries)
    }

    /// Total number of ledger entries.
    pub fn entry_count(&self) -> usize { self.inner.entries().len() }

    /// Verify the integrity of the entire hash chain.
    pub fn verify_chain(&self) -> bool { self.inner.verify_chain() }

    /// Returns the trial balance as a raw integer.
    /// For single-currency ledgers, this is always 0 when balanced.
    pub fn trial_balance(&self) -> i64 { self.inner.trial_balance() as i64 }

    /// Returns the trial balance grouped by currency as JSON: `{ "USD": 0, "IDR": 0, ... }`.
    pub fn trial_balance_by_currency(&self) -> Result<String, JsValue> {
        to_json(&self.inner.trial_balance_by_currency())
    }

    // ── Financial Reports ───────────────────────────────────────────

    /// Generate a trial balance report for a currency. Returns JSON.
    ///
    /// Response shape: `{ currency_filter, rows: [...], total_debit, total_credit }`
    pub fn trial_balance_report(&self, currency: &str) -> Result<String, JsValue> {
        to_json(&self.inner.trial_balance_report(currency))
    }

    /// Generate a balance sheet for a currency at a point in time. Returns JSON.
    ///
    /// Response shape: `{ currency, as_of, assets, liabilities, equity, total_assets, ... }`
    pub fn balance_sheet(&self, currency: &str, as_of: u64) -> Result<String, JsValue> {
        to_json(&self.inner.balance_sheet(currency, as_of))
    }

    /// Generate an income statement for a currency over a date range. Returns JSON.
    ///
    /// Response shape: `{ currency, from_ts, to_ts, revenue, expenses, total_revenue, total_expenses, net_income }`
    pub fn income_statement(&self, currency: &str, from_ts: u64, to_ts: u64) -> Result<String, JsValue> {
        to_json(&self.inner.income_statement(currency, from_ts, to_ts))
    }

    /// Generate a general ledger detail report for a single account. Returns JSON or throws if not found.
    ///
    /// Response shape: `{ account_id, lines: [...], opening_balance, closing_balance, ... }`
    pub fn general_ledger(&self, account_id: u64, from_ts: u64, to_ts: u64) -> Result<String, JsValue> {
        self.inner.general_ledger(crate::types::AccountId(account_id), from_ts, to_ts)
            .ok_or_else(|| not_found("account not found"))
            .and_then(|r| to_json(&r))
    }

    // ── Period Closing ──────────────────────────────────────────────

    /// Close an accounting period. Returns the closing entry ID (as JSON number), or `null` if nothing to close.
    ///
    /// Arguments:
    /// - `currency`: ISO 4217 code (e.g. `"USD"`)
    /// - `end_timestamp`: Seal everything at or before this time
    /// - `retained_earnings_id`: Equity account to receive net income
    pub fn close_period(
        &mut self,
        currency: &str,
        end_timestamp: u64,
        retained_earnings_id: u64,
    ) -> Result<JsValue, JsValue> {
        let result = self.inner.close_period(
            currency, end_timestamp, crate::types::AccountId(retained_earnings_id),
        ).map_err(ledger_err)?;
        match result {
            Some(id) => Ok(JsValue::from_f64(id as f64)),
            None => Ok(JsValue::NULL),
        }
    }

    /// Close a period with audit trail. Returns the closing entry ID or `null`.
    ///
    /// `audit_json`: `{ "actor": "reyvan", "source": "dashboard", "notes": "Monthly close" }`
    pub fn close_period_audited(
        &mut self,
        currency: &str,
        end_timestamp: u64,
        retained_earnings_id: u64,
        audit_json: &str,
    ) -> Result<JsValue, JsValue> {
        let v: serde_json::Value = serde_json::from_str(audit_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let audit = parse_audit(&v)
            .ok_or_else(|| JsValue::from_str("audit must contain at least \"actor\""))?;
        let result = self.inner.close_period_audited(
            currency, end_timestamp, crate::types::AccountId(retained_earnings_id), audit,
        ).map_err(ledger_err)?;
        match result {
            Some(id) => Ok(JsValue::from_f64(id as f64)),
            None => Ok(JsValue::NULL),
        }
    }

    /// Returns all closed periods as a JSON array.
    ///
    /// Each element: `{ currency, closed_at, closing_entry_id, net_income, retained_earnings_id }`
    pub fn closed_periods(&self) -> Result<String, JsValue> {
        to_json(self.inner.closed_periods())
    }

    // ── Reconciliation ──────────────────────────────────────────────

    /// Reconcile two datasets. Both arguments are JSON arrays of `{ id, amount, date }`.
    ///
    /// Returns a JSON array of `{ id, status }` where status is one of:
    /// `"Matched"`, `{ "AmountMismatch": {...} }`, `"InternalOnly"`, `"ExternalOnly"`, etc.
    pub fn reconcile(internal_json: &str, external_json: &str) -> Result<String, JsValue> {
        let internal: Vec<crate::reconcile::ReconcileRecord> =
            serde_json::from_str(internal_json)
                .map_err(|e| JsValue::from_str(&format!("invalid internal data: {e}")))?;
        let external: Vec<crate::reconcile::ReconcileRecord> =
            serde_json::from_str(external_json)
                .map_err(|e| JsValue::from_str(&format!("invalid external data: {e}")))?;
        let results = crate::reconcile::reconcile(&internal, &external);
        to_json(&results)
    }
}

impl Default for WasmLedger {
    fn default() -> Self { Self::new() }
}
