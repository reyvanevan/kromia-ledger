use kromia_ledger::*;
use std::time::Instant;

fn main() {
    println!("=== KROMIA LEDGER STRESS TEST ===\n");

    // ── 1. Record 100K transactions ─────────────────────────
    let mut ledger = Ledger::new();
    let cash    = ledger.create_account("Cash",    "1000", AccountType::Asset,   Currency::usd()).unwrap();
    let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
    let expense = ledger.create_account("Rent",    "5000", AccountType::Expense, Currency::usd()).unwrap();
    let equity  = ledger.create_account("Equity",  "3000", AccountType::Equity,  Currency::usd()).unwrap();
    let retained = ledger.create_account("Retained Earnings", "3100", AccountType::Equity, Currency::usd()).unwrap();

    // Seed capital
    ledger.record_transaction_at("Capital", &[(cash, 999_999_999_99)], &[(equity, 999_999_999_99)], 1).unwrap();

    let t0 = Instant::now();
    for i in 0..100_000_u64 {
        let ts = 100 + i;
        if i % 2 == 0 {
            ledger.record_transaction_at("Sale", &[(cash, 100_00)], &[(revenue, 100_00)], ts).unwrap();
        } else {
            ledger.record_transaction_at("Expense", &[(expense, 50_00)], &[(cash, 50_00)], ts).unwrap();
        }
    }
    let d1 = t0.elapsed();
    println!("1. Record 100K transactions:   {:>8.1}ms", d1.as_secs_f64() * 1000.0);

    // ── 2. Verify hash chain ────────────────────────────────
    let t1 = Instant::now();
    assert!(ledger.verify_chain());
    let d2 = t1.elapsed();
    println!("2. Verify 100K chain:          {:>8.1}ms", d2.as_secs_f64() * 1000.0);

    // ── 3. Trial balance ────────────────────────────────────
    let t2 = Instant::now();
    assert_eq!(ledger.trial_balance(), 0);
    let d3 = t2.elapsed();
    println!("3. Trial balance:              {:>8.3}ms", d3.as_secs_f64() * 1000.0);

    // ── 4. Financial reports ────────────────────────────────
    let t3 = Instant::now();
    let _tb = ledger.trial_balance_report("USD");
    let _bs = ledger.balance_sheet("USD", 999_999);
    let _is = ledger.income_statement("USD", 0, 999_999);
    let _gl = ledger.general_ledger(cash, 0, 999_999);
    let d4 = t3.elapsed();
    println!("4. Generate 4 reports:         {:>8.1}ms", d4.as_secs_f64() * 1000.0);

    // ── 5. Period closing ───────────────────────────────────
    let t4 = Instant::now();
    let entry_id = ledger.close_period("USD", 200_000, retained).unwrap();
    let d5 = t4.elapsed();
    assert!(entry_id.is_some());
    assert_eq!(ledger.get_balance(revenue).unwrap(), 0);
    assert_eq!(ledger.get_balance(expense).unwrap(), 0);
    println!("5. Close period:               {:>8.3}ms", d5.as_secs_f64() * 1000.0);

    // ── 6. Sealed period enforcement ────────────────────────
    let t5 = Instant::now();
    for _ in 0..10_000 {
        let err = ledger.record_transaction_at("Blocked", &[(cash, 100)], &[(revenue, 100)], 150_000);
        assert!(err.is_err());
    }
    let d6 = t5.elapsed();
    println!("6. 10K sealed-period rejects:  {:>8.1}ms", d6.as_secs_f64() * 1000.0);

    // ── 7. JSON serialization roundtrip ─────────────────────
    let t6 = Instant::now();
    let json = ledger.save_json().unwrap();
    let d7a = t6.elapsed();

    let t7 = Instant::now();
    let restored = Ledger::load_json(&json).unwrap();
    let d7b = t7.elapsed();
    assert!(restored.verify_chain());
    assert_eq!(restored.closed_periods().len(), 1);
    println!("7a. Serialize 100K entries:    {:>8.1}ms  ({:.1} MB JSON)", d7a.as_secs_f64() * 1000.0, json.len() as f64 / 1_048_576.0);
    println!("7b. Deserialize + verify:     {:>8.1}ms", d7b.as_secs_f64() * 1000.0);

    // ── 8. Reconciliation 100K ──────────────────────────────
    let internal: Vec<_> = (0..100_000u64).map(|i| ReconcileRecord {
        id: format!("TX-{i:06}"),
        amount: if i % 2 == 0 { 100_00 } else { 50_00 },
        date: "2026-01-15".into(),
    }).collect();
    let mut external = internal.clone();
    // Introduce 50 anomalies
    for i in 0..50 {
        external[i * 2000].amount += 1;
    }

    let t8 = Instant::now();
    let results = reconcile(&internal, &external);
    let d8 = t8.elapsed();
    let mismatches = results.iter().filter(|r| !matches!(r.status, ReconcileStatus::Matched)).count();
    println!("8. Reconcile 100K (50 anom):   {:>8.1}ms", d8.as_secs_f64() * 1000.0);

    // ── Summary ─────────────────────────────────────────────
    let total_entries = ledger.entries().len();
    println!("\n=== SUMMARY ===");
    println!("Total entries:      {}", total_entries);
    println!("Closed periods:     {}", ledger.closed_periods().len());
    println!("Trial balance:      {}", ledger.trial_balance());
    println!("Chain valid:        {}", ledger.verify_chain());
    println!("Reconcile mismatches: {}", mismatches);
    println!("JSON size:          {:.1} MB", json.len() as f64 / 1_048_576.0);
    println!("\n✅ ALL STRESS TESTS PASSED");
}
