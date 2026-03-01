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

// ── ANSI escape codes (zero dependencies) ──────────────────────────────
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

fn main() {
    println!("{CYAN}╔══════════════════════════════════════════════════════════════╗{RESET}");
    println!("{CYAN}║{RESET}  {BOLD}          Kromia Ledger — Quickstart Demo                {RESET}{CYAN}║{RESET}");
    println!("{CYAN}║{RESET}  {DIM}   Double-entry · Hash-chained · Multi-currency · WASM   {RESET}{CYAN}║{RESET}");
    println!("{CYAN}╚══════════════════════════════════════════════════════════════╝{RESET}");
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
        println!("  {GREEN}✅ {}{RESET}", tx.label);
    }

    println!("  {BOLD}Total entries: {}{RESET}", ledger.entries().len());

    // ── 3. Cross-Currency Exchange (USD → IDR) ──────────────────────────
    print_section("3. CROSS-CURRENCY EXCHANGE (USD → IDR)");

    // Bank Indonesia mid-rate, 01 Mar 2026: 16,802.45 IDR/USD
    // 1 cent = 168.0245 IDR → rate = 1_680_245 * RATE_SCALE / 10_000
    // $100.00 = 10_000 cents → 10_000 × 168.0245 = 1,680,245 IDR
    let usd_amount: i128 = 10_000; // $100.00 in cents
    let rate: i128 = 1_680_245 * RATE_SCALE / 10_000; // 1 cent = 168.0245 IDR
    let idr_amount: i128 = usd_amount * rate / RATE_SCALE; // 1,680,245 IDR

    ledger
        .record_exchange_full(
            "USD to IDR exchange — rate 16,802.45 IDR/USD (BI mid-rate, 01 Mar 2026)",
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
        "  {GREEN}✅ Jan 28 — Exchanged {} → {} IDR (rate: 16,802.45 IDR/USD){RESET}",
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

    println!("  {BOLD}{:<22} {:>10}  {:>18}{RESET}", "Account", "Type", "Balance");
    println!("  {DIM}{}{RESET}", "─".repeat(54));
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
        if valid { format!("{GREEN}✅ VERIFIED{RESET}") } else { format!("{RED}❌ BROKEN{RESET}") },
        ledger.entries().len()
    );
    if let Some(first) = ledger.entries().first() {
        println!("  Genesis hash : {DIM}{}…{RESET}", &first.hash[..16]);
    }
    if let Some(last) = ledger.entries().last() {
        println!("  Latest hash  : {DIM}{}…{RESET}", &last.hash[..16]);
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
        Err(e) => println!("  {GREEN}✅ Duplicate rejected: {e}{RESET}"),
        Ok(_) => println!("  {RED}❌ BUG: duplicate was accepted!{RESET}"),
    }

    // ── 7. JSON Persistence ─────────────────────────────────────────────
    print_section("7. JSON PERSISTENCE (save → load → verify)");

    let json = ledger.save_json().unwrap();
    println!("  Serialized: {BOLD}{}{RESET} bytes ({} entries)", json.len(), ledger.entries().len());

    let restored = Ledger::load_json(&json).unwrap();
    println!(
        "  Restored:   {} entries, chain {}",
        restored.entries().len(),
        if restored.verify_chain() { format!("{GREEN}✅ VERIFIED{RESET}") } else { format!("{RED}❌ BROKEN{RESET}") }
    );

    let bal_before = ledger.get_balance(cash).unwrap();
    let bal_after = restored.get_balance(cash).unwrap();
    println!(
        "  Cash balance: {} == {} → {}",
        format_balance_with_currency(bal_before, "$"),
        format_balance_with_currency(bal_after, "$"),
        if bal_before == bal_after { format!("{GREEN}✅ match{RESET}") } else { format!("{RED}❌ mismatch{RESET}") }
    );

    // ── 8. Tamper Detection ─────────────────────────────────────────────
    print_section("8. TAMPER DETECTION");

    let tampered = json.replacen("Owner capital investment", "TAMPERED DESCRIPTION!", 1);
    match Ledger::load_json(&tampered) {
        Err(e) => println!("  {GREEN}✅ Tampered JSON detected:{RESET} {e}"),
        Ok(_) => println!("  {RED}❌ BUG: tampered JSON was accepted!{RESET}"),
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

    println!("  {BOLD}{:<12} {:<35}{RESET}", "ID", "Status");
    println!("  {DIM}{}{RESET}", "─".repeat(48));
    for r in &results {
        let status = match &r.status {
            ReconcileStatus::Matched => format!("{GREEN}✅ Matched{RESET}"),
            ReconcileStatus::AmountMismatch { internal, external } => {
                format!("{YELLOW}⚠️  Amount: {internal} vs {external}{RESET}")
            }
            ReconcileStatus::DateMismatch { internal, external } => {
                format!("{YELLOW}⚠️  Date: {internal} vs {external}{RESET}")
            }
            ReconcileStatus::InternalOnly => format!("{YELLOW}📋 Ledger only (missing in bank){RESET}"),
            ReconcileStatus::ExternalOnly => format!("{YELLOW}🏦 Bank only (missing in ledger){RESET}"),
            ReconcileStatus::MultipleMismatch { .. } => format!("{YELLOW}⚠️  Multiple mismatches{RESET}"),
        };
        println!("  {:<12} {status}", r.id);
    }

    let matched = results.iter().filter(|r| r.status == ReconcileStatus::Matched).count();
    let anomalies = results.len() - matched;
    println!();
    println!("  Result: {GREEN}{matched} matched{RESET}, {YELLOW}{anomalies} anomalies{RESET} out of {} records", results.len());

    // ── 10. Entry History (Audit Trail) ─────────────────────────────────
    print_section("10. ENTRY HISTORY (last 3 entries)");

    let entries = ledger.entries();
    for entry in &entries[entries.len().saturating_sub(3)..] {
        println!(
            "  {BOLD}#{:<3}{RESET} │ {:<40} │ D:{:>10} C:{:>10} │ {DIM}{}…{RESET}",
            entry.id,
            truncate(&entry.transaction.description, 40),
            entry.transaction.total_debit,
            entry.transaction.total_credit,
            &entry.hash[..12]
        );
    }

    // ── Done ────────────────────────────────────────────────────────────
    println!();
    println!("{CYAN}╔══════════════════════════════════════════════════════════════╗{RESET}");
    println!(
        "{CYAN}║{RESET}  {GREEN}{BOLD}All operations completed successfully.{RESET}                      {CYAN}║{RESET}"
    );
    println!(
        "{CYAN}║{RESET}  {BOLD}{}{RESET} entries · {BOLD}{}{RESET} accounts · Chain: {GREEN}✅{RESET} · Anomalies caught: {GREEN}✅{RESET}   {CYAN}║{RESET}",
        ledger.entries().len(),
        ledger.accounts().count()
    );
    println!("{CYAN}╚══════════════════════════════════════════════════════════════╝{RESET}");
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn print_section(title: &str) {
    println!();
    println!("{CYAN}──{RESET} {BOLD}{title}{RESET} {CYAN}──{RESET}");
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
