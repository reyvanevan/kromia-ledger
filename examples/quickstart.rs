//! # Quickstart — Kromia Ledger Demo
//!
//! A small-business bookkeeping simulation showcasing the full Kromia Ledger API:
//!
//! - Chart of Accounts (multi-currency: USD + IDR)
//! - Double-entry transactions (Σ debit = Σ credit)
//! - Cross-currency exchange (USD → IDR, integer-scaled rates)
//! - SHA-256 hash-chain integrity verification
//! - JSON persistence (save → load → verify)
//! - Tamper detection (flip one byte → chain broken)
//! - Reconciliation engine (internal ledger vs bank statement)
//! - Idempotency protection (duplicate key rejection)
//! - Formatted financial report
//!
//! ```sh
//! cargo run --example quickstart
//! ```

#![allow(clippy::inconsistent_digit_grouping, clippy::type_complexity)]

use kromia_ledger::{
    format_balance_with_currency, reconcile, AccountType, Currency, Ledger, ReconcileRecord,
    ReconcileStatus, RATE_SCALE,
};

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║            Kromia Ledger — Quickstart Demo                  ║");
    println!("║     Double-entry · Hash-chained · Multi-currency · WASM     ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let mut ledger = Ledger::new();

    // ── 1. Chart of Accounts ────────────────────────────────────────────
    print_section("1. CHART OF ACCOUNTS");

    let usd = Currency::usd(); // precision = 2 (cents)
    let idr = Currency::idr(); // precision = 0

    let cash = ledger
        .create_account("Cash", "1000", AccountType::Asset, usd.clone())
        .unwrap();
    let bank = ledger
        .create_account("Bank Account", "1100", AccountType::Asset, usd.clone())
        .unwrap();
    let idr_account = ledger
        .create_account("IDR Account", "1200", AccountType::Asset, idr.clone())
        .unwrap();
    let payable = ledger
        .create_account("Accounts Payable", "2000", AccountType::Liability, usd.clone())
        .unwrap();
    let equity = ledger
        .create_account("Owner's Equity", "3000", AccountType::Equity, usd.clone())
        .unwrap();
    let revenue = ledger
        .create_account("Sales Revenue", "4000", AccountType::Revenue, usd.clone())
        .unwrap();
    let salary_exp = ledger
        .create_account("Salary Expense", "5000", AccountType::Expense, usd.clone())
        .unwrap();
    let rent_exp = ledger
        .create_account("Rent Expense", "5100", AccountType::Expense, usd.clone())
        .unwrap();

    for acc in ledger.accounts() {
        println!("  {acc}");
    }
    println!("  Total accounts: {}", ledger.accounts().count());

    // ── 2. Transactions — January 2026 ──────────────────────────────────
    print_section("2. TRANSACTIONS — JANUARY 2026");

    let ts_base = 1735689600_u64; // 2026-01-01 00:00:00 UTC
    let day = 86400_u64;

    // Helper: record and print
    struct Tx<'a> {
        desc: &'a str,
        label: &'a str,
        day_offset: u64,
        key: &'a str,
    }

    let transactions = [
        // Jan 1: Owner invests $50,000
        Tx { desc: "Owner capital investment", label: "Jan 01 — Capital investment $50,000.00", day_offset: 0, key: "CAP-001" },
        // Jan 5: Sales revenue $15,000
        Tx { desc: "Sales revenue — week 1", label: "Jan 05 — Sales revenue $15,000.00", day_offset: 4, key: "SALES-001" },
        // Jan 10: Salary payment $5,000
        Tx { desc: "Salary payment — January", label: "Jan 10 — Salary payment $5,000.00", day_offset: 9, key: "SAL-001" },
        // Jan 12: Rent payment $3,000
        Tx { desc: "Office rent — January", label: "Jan 12 — Rent payment $3,000.00", day_offset: 11, key: "RENT-001" },
        // Jan 15: Sales revenue $20,000
        Tx { desc: "Sales revenue — week 3", label: "Jan 15 — Sales revenue $20,000.00", day_offset: 14, key: "SALES-002" },
        // Jan 18: Purchase on credit $8,000
        Tx { desc: "Inventory purchase (on credit)", label: "Jan 18 — Purchase on credit $8,000.00", day_offset: 17, key: "PUR-001" },
        // Jan 20: Transfer to bank $40,000
        Tx { desc: "Transfer cash to bank", label: "Jan 20 — Bank transfer $40,000.00", day_offset: 19, key: "TRF-001" },
        // Jan 25: Sales revenue $12,000
        Tx { desc: "Sales revenue — end of month", label: "Jan 25 — Sales revenue $12,000.00", day_offset: 24, key: "SALES-003" },
    ];

    let tx_data: [(
        &[(kromia_ledger::AccountId, i128)],
        &[(kromia_ledger::AccountId, i128)],
    ); 8] = [
        (&[(cash, 50_000_00)], &[(equity, 50_000_00)]),              // capital
        (&[(cash, 15_000_00)], &[(revenue, 15_000_00)]),             // sales 1
        (&[(salary_exp, 5_000_00)], &[(cash, 5_000_00)]),            // salary
        (&[(rent_exp, 3_000_00)], &[(cash, 3_000_00)]),              // rent
        (&[(cash, 20_000_00)], &[(revenue, 20_000_00)]),             // sales 2
        (&[(cash, 8_000_00)], &[(payable, 8_000_00)]),               // purchase
        (&[(bank, 40_000_00)], &[(cash, 40_000_00)]),                // transfer
        (&[(cash, 12_000_00)], &[(revenue, 12_000_00)]),             // sales 3
    ];

    for (i, tx) in transactions.iter().enumerate() {
        ledger
            .record_transaction_full(
                tx.desc,
                tx_data[i].0,
                tx_data[i].1,
                ts_base + tx.day_offset * day,
                Some(tx.key),
            )
            .unwrap();
        println!("  ✅ {}", tx.label);
    }

    println!("  Total entries: {}", ledger.entries().len());

    // ── 3. Cross-Currency Exchange (USD → IDR) ──────────────────────────
    print_section("3. CROSS-CURRENCY EXCHANGE (USD → IDR)");

    // Rate: 1 USD cent = 157 IDR → rate = 157 * RATE_SCALE = 157_000_000
    // $100.00 = 10_000 cents → 10_000 × 157 = 1,570,000 IDR
    let usd_amount: i128 = 10_000; // $100.00 in cents
    let rate: i128 = 157 * RATE_SCALE; // 1 cent = 157 IDR
    let idr_amount: i128 = usd_amount * rate / RATE_SCALE; // 1,570,000 IDR

    ledger
        .record_exchange_full(
            "USD to IDR exchange — rate 15,700 IDR/USD",
            bank,
            usd_amount,
            idr_account,
            idr_amount,
            rate,
            ts_base + 27 * day, // Jan 28
            Some("FX-001"),
        )
        .unwrap();

    println!(
        "  ✅ Jan 28 — Exchanged {} → {} IDR (rate: 15,700 IDR/USD)",
        format_balance_with_currency(usd_amount, "$"),
        format_idr(idr_amount),
    );

    // ── 4. End-of-Month Balances ────────────────────────────────────────
    print_section("4. END-OF-MONTH BALANCES");

    let usd_accounts = [
        (cash, "$"),
        (bank, "$"),
        (payable, "$"),
        (equity, "$"),
        (revenue, "$"),
        (salary_exp, "$"),
        (rent_exp, "$"),
    ];

    println!("  {:<22} {:>10}  {:>18}", "Account", "Type", "Balance");
    println!("  {}", "─".repeat(54));
    for (id, sym) in &usd_accounts {
        let acc = ledger.get_account(*id).unwrap();
        println!(
            "  {:<22} {:>10}  {:>18}",
            acc.name,
            acc.account_type.to_string(),
            format_balance_with_currency(acc.balance, sym)
        );
    }
    // IDR account (precision=0, format manually)
    let idr_acc = ledger.get_account(idr_account).unwrap();
    println!(
        "  {:<22} {:>10}  {:>14} IDR",
        idr_acc.name,
        idr_acc.account_type.to_string(),
        format_idr(idr_acc.balance)
    );

    // ── 5. Hash Chain Integrity ─────────────────────────────────────────
    print_section("5. HASH CHAIN INTEGRITY");

    let valid = ledger.verify_chain();
    println!(
        "  Chain status: {} ({} entries)",
        if valid { "✅ VERIFIED" } else { "❌ BROKEN" },
        ledger.entries().len()
    );
    if let Some(first) = ledger.entries().first() {
        println!("  Genesis hash : {}…", &first.hash[..16]);
    }
    if let Some(last) = ledger.entries().last() {
        println!("  Latest hash  : {}…", &last.hash[..16]);
    }

    // ── 6. Idempotency Protection ───────────────────────────────────────
    print_section("6. IDEMPOTENCY PROTECTION");

    let dup = ledger.record_transaction_full(
        "Owner capital investment (DUPLICATE!)",
        &[(cash, 50_000_00)],
        &[(equity, 50_000_00)],
        ts_base,
        Some("CAP-001"), // same key → rejected
    );
    match dup {
        Err(e) => println!("  ✅ Duplicate rejected: {e}"),
        Ok(_) => println!("  ❌ BUG: duplicate was accepted!"),
    }

    // ── 7. JSON Persistence ─────────────────────────────────────────────
    print_section("7. JSON PERSISTENCE (save → load → verify)");

    let json = ledger.save_json().unwrap();
    println!("  Serialized: {} bytes ({} entries)", json.len(), ledger.entries().len());

    let restored = Ledger::load_json(&json).unwrap();
    println!(
        "  Restored:   {} entries, chain {}",
        restored.entries().len(),
        if restored.verify_chain() { "✅ VERIFIED" } else { "❌ BROKEN" }
    );

    let bal_before = ledger.get_balance(cash).unwrap();
    let bal_after = restored.get_balance(cash).unwrap();
    println!(
        "  Cash balance: {} == {} → {}",
        format_balance_with_currency(bal_before, "$"),
        format_balance_with_currency(bal_after, "$"),
        if bal_before == bal_after { "✅ match" } else { "❌ mismatch" }
    );

    // ── 8. Tamper Detection ─────────────────────────────────────────────
    print_section("8. TAMPER DETECTION");

    let tampered = json.replacen("Owner capital investment", "TAMPERED DESCRIPTION!", 1);
    match Ledger::load_json(&tampered) {
        Err(e) => println!("  ✅ Tampered JSON detected: {e}"),
        Ok(_) => println!("  ❌ BUG: tampered JSON was accepted!"),
    }

    // ── 9. Reconciliation (Ledger vs Bank Statement) ────────────────────
    print_section("9. RECONCILIATION (ledger vs bank statement)");

    // Internal records (from our ledger)
    let internal = vec![
        ReconcileRecord { id: "TRF-001".into(), amount: 40_000_00, date: "2026-01-20".into() },
        ReconcileRecord { id: "FX-001".into(),  amount: 10_000,    date: "2026-01-28".into() },
        ReconcileRecord { id: "FEE-001".into(), amount: 25_00,     date: "2026-01-31".into() },
    ];

    // Bank statement (external) — with deliberate anomalies
    let external = vec![
        ReconcileRecord { id: "TRF-001".into(), amount: 40_000_00, date: "2026-01-20".into() },  // perfect match
        ReconcileRecord { id: "FX-001".into(),  amount: 10_000,    date: "2026-01-29".into() },  // date mismatch
        ReconcileRecord { id: "INT-001".into(), amount: 1_250,     date: "2026-01-31".into() },  // bank-only (interest)
    ];

    let results = reconcile(&internal, &external);

    println!("  {:<12} {:<35}", "ID", "Status");
    println!("  {}", "─".repeat(48));
    for r in &results {
        let status = match &r.status {
            ReconcileStatus::Matched => "✅ Matched".into(),
            ReconcileStatus::AmountMismatch { internal, external } => {
                format!("⚠️  Amount: {internal} vs {external}")
            }
            ReconcileStatus::DateMismatch { internal, external } => {
                format!("⚠️  Date: {internal} vs {external}")
            }
            ReconcileStatus::InternalOnly => "📋 Ledger only (missing in bank)".into(),
            ReconcileStatus::ExternalOnly => "🏦 Bank only (missing in ledger)".into(),
            ReconcileStatus::MultipleMismatch { .. } => "⚠️  Multiple mismatches".into(),
        };
        println!("  {:<12} {status}", r.id);
    }

    let matched = results.iter().filter(|r| r.status == ReconcileStatus::Matched).count();
    let anomalies = results.len() - matched;
    println!();
    println!("  Result: {matched} matched, {anomalies} anomalies out of {} records", results.len());

    // ── 10. Entry History (Audit Trail) ─────────────────────────────────
    print_section("10. ENTRY HISTORY (last 3 entries)");

    let entries = ledger.entries();
    for entry in &entries[entries.len().saturating_sub(3)..] {
        println!(
            "  #{:<3} │ {:<40} │ D:{:>10} C:{:>10} │ {}…",
            entry.id,
            truncate(&entry.transaction.description, 40),
            entry.transaction.total_debit,
            entry.transaction.total_credit,
            &entry.hash[..12]
        );
    }

    // ── Done ────────────────────────────────────────────────────────────
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!(
        "║  All operations completed successfully.                      ║"
    );
    println!(
        "║  {} entries · {} accounts · Chain: ✅ · Anomalies caught: ✅   ║",
        ledger.entries().len(),
        ledger.accounts().count()
    );
    println!("╚══════════════════════════════════════════════════════════════╝");
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn print_section(title: &str) {
    println!();
    println!("── {title} ──");
    println!();
}

/// Format an IDR amount with thousands separators (precision=0, no decimals).
fn format_idr(amount: i128) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    let digits = abs.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    format!("{sign}{}", result.chars().rev().collect::<String>())
}

/// Truncate a string to `max_len` characters, appending "…" if needed.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
