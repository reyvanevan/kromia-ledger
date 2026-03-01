use wasm_bindgen::prelude::*;

use crate::types::{AccountType, Balance};
use crate::Ledger;

/// WASM-exposed wrapper for the Kromia Ledger.
#[wasm_bindgen]
pub struct WasmLedger {
    inner: Ledger,
}

#[wasm_bindgen]
impl WasmLedger {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Ledger::new(),
        }
    }

    /// Create an account. `account_type`: 0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense.
    pub fn create_account(&mut self, name: &str, account_type: u8) -> u64 {
        let at = match account_type {
            0 => AccountType::Asset,
            1 => AccountType::Liability,
            2 => AccountType::Equity,
            3 => AccountType::Revenue,
            4 => AccountType::Expense,
            _ => AccountType::Asset,
        };
        self.inner.create_account(name, at).0
    }

    /// Record a transaction via JSON.
    /// Expects JSON: `{ "description": "...", "debits": [[id, amount], ...], "credits": [[id, amount], ...] }`
    pub fn record_transaction(&mut self, json: &str) -> Result<u64, JsValue> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let description = v["description"].as_str().unwrap_or("");

        let debits = parse_lines(&v["debits"])
            .map_err(|e| JsValue::from_str(&e))?;
        let credits = parse_lines(&v["credits"])
            .map_err(|e| JsValue::from_str(&e))?;

        self.inner
            .record_transaction(description, &debits, &credits)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn verify_chain(&self) -> bool {
        self.inner.verify_chain()
    }

    pub fn trial_balance(&self) -> f64 {
        self.inner.trial_balance() as f64 / 100.0
    }

    pub fn entry_count(&self) -> usize {
        self.inner.entries().len()
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
    fn default() -> Self {
        Self::new()
    }
}
