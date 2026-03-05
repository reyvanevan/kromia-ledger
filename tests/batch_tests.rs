//! Integration tests for the batch transaction recording API.
//!
//! These tests exercise the same JSON parsing and sequential recording logic
//! used by `WasmLedger::record_transactions_batch`, but via the native
//! `Ledger` API (since `#[wasm_bindgen]` methods cannot be called in native tests).

use kromia_ledger::*;

fn setup_ledger() -> (Ledger, AccountId, AccountId) {
    let mut ledger = Ledger::new();
    let cash = ledger
        .create_account("Cash", "1000", AccountType::Asset, Currency::usd())
        .unwrap();
    let revenue = ledger
        .create_account("Revenue", "4000", AccountType::Revenue, Currency::usd())
        .unwrap();
    (ledger, cash, revenue)
}

// ── Happy Path ──────────────────────────────────────────────────────

#[test]
fn batch_record_multiple_transactions() {
    let (mut ledger, cash, revenue) = setup_ledger();

    // Simulate what WasmLedger::record_transactions_batch does internally
    let json = format!(
        r#"[
            {{ "description": "Sale 1", "debits": [[{}, 10000]], "credits": [[{}, 10000]], "timestamp": 100 }},
            {{ "description": "Sale 2", "debits": [[{}, 20000]], "credits": [[{}, 20000]], "timestamp": 101 }},
            {{ "description": "Sale 3", "debits": [[{}, 30000]], "credits": [[{}, 30000]], "timestamp": 102 }}
        ]"#,
        cash.0, revenue.0, cash.0, revenue.0, cash.0, revenue.0
    );

    let items: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    let mut entry_ids = Vec::new();

    for v in &items {
        let desc = v["description"].as_str().unwrap();
        let debits = parse_json_lines(&v["debits"]);
        let credits = parse_json_lines(&v["credits"]);
        let ts = v["timestamp"].as_u64().unwrap();
        let id = ledger
            .record_transaction_at(desc, &debits, &credits, ts)
            .unwrap();
        entry_ids.push(id);
    }

    assert_eq!(entry_ids.len(), 3);
    // IDs are sequential
    assert_eq!(entry_ids[1], entry_ids[0] + 1);
    assert_eq!(entry_ids[2], entry_ids[1] + 1);
    // Balances correct: 10000 + 20000 + 30000 = 60000
    assert_eq!(ledger.get_balance(cash).unwrap(), 60000);
    // Revenue is credit-normal → negative balance
    assert_eq!(ledger.get_balance(revenue).unwrap(), 60000);
    assert!(ledger.verify_chain());
}

#[test]
fn batch_empty_array() {
    let (ledger, _cash, _revenue) = setup_ledger();

    let items: Vec<serde_json::Value> = serde_json::from_str("[]").unwrap();
    assert!(items.is_empty());
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn batch_single_transaction() {
    let (mut ledger, cash, revenue) = setup_ledger();

    let json = format!(
        r#"[{{ "description": "Solo", "debits": [[{}, 5000]], "credits": [[{}, 5000]], "timestamp": 1 }}]"#,
        cash.0, revenue.0
    );
    let items: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();

    for v in &items {
        let debits = parse_json_lines(&v["debits"]);
        let credits = parse_json_lines(&v["credits"]);
        ledger
            .record_transaction_at(
                v["description"].as_str().unwrap(),
                &debits,
                &credits,
                v["timestamp"].as_u64().unwrap(),
            )
            .unwrap();
    }

    assert_eq!(ledger.entries().len(), 1);
    assert_eq!(ledger.get_balance(cash).unwrap(), 5000);
}

// ── Fail-Fast Behavior ─────────────────────────────────────────────

#[test]
fn batch_fail_fast_on_unbalanced() {
    let (mut ledger, cash, revenue) = setup_ledger();

    // First tx: valid. Second tx: unbalanced (debit != credit).
    let json = format!(
        r#"[
            {{ "description": "OK", "debits": [[{}, 10000]], "credits": [[{}, 10000]], "timestamp": 100 }},
            {{ "description": "BAD", "debits": [[{}, 99999]], "credits": [[{}, 11111]], "timestamp": 101 }}
        ]"#,
        cash.0, revenue.0, cash.0, revenue.0
    );

    let items: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    let mut entry_ids = Vec::new();
    let mut fail_index = None;

    for (i, v) in items.iter().enumerate() {
        let debits = parse_json_lines(&v["debits"]);
        let credits = parse_json_lines(&v["credits"]);
        let result = ledger.record_transaction_at(
            v["description"].as_str().unwrap(),
            &debits,
            &credits,
            v["timestamp"].as_u64().unwrap(),
        );
        match result {
            Ok(id) => entry_ids.push(id),
            Err(_) => {
                fail_index = Some(i);
                break;
            }
        }
    }

    // First tx committed, second failed at index 1
    assert_eq!(entry_ids.len(), 1);
    assert_eq!(fail_index, Some(1));
    // First tx's effect is permanent (append-only ledger)
    assert_eq!(ledger.get_balance(cash).unwrap(), 10000);
    assert!(ledger.verify_chain());
}

#[test]
fn batch_fail_on_first_transaction() {
    let (mut ledger, cash, revenue) = setup_ledger();

    // First tx itself is invalid (zero amount)
    let json = format!(
        r#"[
            {{ "description": "Zero", "debits": [[{}, 0]], "credits": [[{}, 0]], "timestamp": 100 }}
        ]"#,
        cash.0, revenue.0
    );

    let items: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    let result = ledger.record_transaction_at(
        items[0]["description"].as_str().unwrap(),
        &parse_json_lines(&items[0]["debits"]),
        &parse_json_lines(&items[0]["credits"]),
        100,
    );

    assert!(result.is_err());
    assert_eq!(ledger.entries().len(), 0);
}

// ── With Optional Fields ────────────────────────────────────────────

#[test]
fn batch_with_audit_and_idempotency() {
    let (mut ledger, cash, revenue) = setup_ledger();

    // Record with audit metadata and idempotency key
    let audit = AuditMeta::new("batch-bot").with_source("stress-test");
    let id1 = ledger
        .record_transaction_audited(
            "Audited Sale",
            &[(cash, 10000)],
            &[(revenue, 10000)],
            200,
            Some("BATCH-001"),
            audit,
        )
        .unwrap();

    // Duplicate idempotency key should fail
    let audit2 = AuditMeta::new("batch-bot");
    let dup = ledger.record_transaction_audited(
        "Dup Sale",
        &[(cash, 10000)],
        &[(revenue, 10000)],
        201,
        Some("BATCH-001"),
        audit2,
    );
    assert!(dup.is_err());

    let entry = ledger.find_entry(id1).unwrap();
    let audit_meta = entry.audit.as_ref().unwrap();
    assert_eq!(audit_meta.actor, "batch-bot");
    assert_eq!(audit_meta.source.as_deref(), Some("stress-test"));
}

// ── Batch Scale ─────────────────────────────────────────────────────

#[test]
fn batch_1000_transactions() {
    let (mut ledger, cash, revenue) = setup_ledger();

    for i in 0..1_000u64 {
        ledger
            .record_transaction_at(
                &format!("TX-{i:04}"),
                &[(cash, 100)],
                &[(revenue, 100)],
                1000 + i,
            )
            .unwrap();
    }

    assert_eq!(ledger.entries().len(), 1_000);
    assert_eq!(ledger.get_balance(cash).unwrap(), 100_000);
    assert_eq!(ledger.trial_balance(), 0);
    assert!(ledger.verify_chain());
}

// ── Helper ──────────────────────────────────────────────────────────

/// Parse `[[id, amount], ...]` JSON into `Vec<(AccountId, Balance)>`.
/// Mirrors the private `parse_lines` in wasm.rs.
fn parse_json_lines(value: &serde_json::Value) -> Vec<(AccountId, Balance)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            let arr = pair.as_array().unwrap();
            let id = AccountId(arr[0].as_u64().unwrap());
            let amount = arr[1].as_i64().unwrap() as Balance;
            (id, amount)
        })
        .collect()
}
