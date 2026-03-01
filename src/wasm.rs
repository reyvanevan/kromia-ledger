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
    ///   "idempotency_key": "ORDER-001" (optional) }
    pub fn record_transaction(&mut self, json: &str) -> Result<u64, JsValue> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let description = v["description"].as_str().unwrap_or("");
        let debits = parse_lines(&v["debits"]).map_err(|e| JsValue::from_str(&e))?;
        let credits = parse_lines(&v["credits"]).map_err(|e| JsValue::from_str(&e))?;
        let timestamp = v["timestamp"].as_u64();
        let idempotency_key = v["idempotency_key"].as_str();

        let ts = timestamp.unwrap_or_else(crate::types::current_timestamp);
        self.inner.record_transaction_full(description, &debits, &credits, ts, idempotency_key)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn verify_chain(&self) -> bool { self.inner.verify_chain() }

    pub fn trial_balance(&self) -> f64 { self.inner.trial_balance() as f64 / 100.0 }

    pub fn entry_count(&self) -> usize { self.inner.entries().len() }

    pub fn get_balance(&self, id: u64) -> Result<f64, JsValue> {
        self.inner.get_balance(crate::types::AccountId(id))
            .map(|b| b as f64 / 100.0)
            .ok_or_else(|| JsValue::from_str("account not found"))
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

impl Default for WasmLedger {
    fn default() -> Self { Self::new() }
}
