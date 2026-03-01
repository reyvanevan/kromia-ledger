//! WebAssembly bindings for the Kromia Ledger engine.
//!
//! This module provides a [`WasmLedger`] wrapper that exposes the core ledger
//! functionality to JavaScript/TypeScript via `wasm-bindgen`. Account types
//! are passed as integers (0–4) and transactions are submitted as JSON strings.
//!
//! This module is only compiled when targeting `wasm32`.

use wasm_bindgen::prelude::*;

use crate::types::{AccountType, Balance, Currency};
use crate::Ledger;

#[wasm_bindgen]
pub struct WasmLedger {
    inner: Ledger,
}

#[wasm_bindgen]
impl WasmLedger {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { inner: Ledger::new() }
    }

    /// Create an account.
    /// `account_type`: 0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense.
    /// `currency_code`: ISO 4217 code (e.g. "USD", "IDR").
    /// `currency_precision`: decimal places (e.g. 2 for USD, 0 for IDR).
    pub fn create_account(
        &mut self,
        name: &str,
        code: &str,
        account_type: u8,
        currency_code: &str,
        currency_precision: u8,
    ) -> Result<u64, JsValue> {
        let at = match account_type {
            0 => AccountType::Asset,
            1 => AccountType::Liability,
            2 => AccountType::Equity,
            3 => AccountType::Revenue,
            4 => AccountType::Expense,
            _ => return Err(JsValue::from_str("invalid account type (0-4)")),
        };
        let currency = Currency::new(currency_code, currency_precision);
        self.inner.create_account(name, code, at, currency)
            .map(|id| id.0)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn deactivate_account(&mut self, id: u64) -> Result<(), JsValue> {
        self.inner.deactivate_account(crate::types::AccountId(id))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Record a transaction via JSON.
    /// JSON format: { "description": "...", "debits": [[id, amount], ...],
    ///   "credits": [[id, amount], ...], "timestamp": 123 (optional),
    ///   "idempotency_key": "ORDER-001" (optional),
    ///   "audit": { "actor": "...", "source": "..." (optional), "notes": "..." (optional) } (optional) }
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
        }.map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Record a cross-currency exchange via JSON.
    /// JSON format: { "description": "...", "from_account": id, "from_amount": amount,
    ///   "to_account": id, "to_amount": amount, "exchange_rate": rate,
    ///   "timestamp": 123 (optional), "idempotency_key": "XCH-001" (optional),
    ///   "audit": { "actor": "...", ... } (optional) }
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
        }.map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn verify_chain(&self) -> bool { self.inner.verify_chain() }

    /// Returns the trial balance as a raw integer in the smallest currency unit.
    ///
    /// For single-currency ledgers this is always 0 when all transactions balance.
    /// For multi-currency ledgers, use per-currency reporting instead.
    pub fn trial_balance(&self) -> i64 { self.inner.trial_balance() as i64 }

    pub fn entry_count(&self) -> usize { self.inner.entries().len() }

    /// Returns the raw balance in the smallest currency unit (e.g. cents for USD).
    ///
    /// JavaScript can safely handle integers up to 2^53, which covers
    /// values up to ~90 trillion — far beyond practical ledger needs.
    pub fn get_balance(&self, id: u64) -> Result<i64, JsValue> {
        self.inner.get_balance(crate::types::AccountId(id))
            .map(|b| b as i64)
            .ok_or_else(|| JsValue::from_str("account not found"))
    }

    /// Returns the balance as a human-readable formatted string.
    ///
    /// Uses the account's currency precision for formatting.
    /// Example: `"1,234.56"` for USD, `"1,000,000"` for IDR.
    pub fn get_balance_formatted(&self, id: u64) -> Result<String, JsValue> {
        let acc = self.inner.get_account(crate::types::AccountId(id))
            .ok_or_else(|| JsValue::from_str("account not found"))?;
        Ok(crate::format::format_amount(acc.balance, acc.currency.precision))
    }

    pub fn save_json(&self) -> Result<String, JsValue> {
        self.inner.save_json().map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn load_json(json: &str) -> Result<WasmLedger, JsValue> {
        let inner = Ledger::load_json(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self { inner })
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

impl Default for WasmLedger {
    fn default() -> Self { Self::new() }
}
