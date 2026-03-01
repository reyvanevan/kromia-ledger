//! # Kromia Ledger — Performance Benchmarks
//!
//! Criterion-based benchmarks measuring throughput on realistic workloads.
//!
//! Run: `cargo bench`
//!
//! ## Benchmarks
//!
//! | Name | Workload | What it measures |
//! |---|---|---|
//! | `record_100k` | 100,000 balanced transactions | Transaction recording + SHA-256 hashing |
//! | `reconcile_100k` | 100K internal vs 100K external (50 anomalies) | O(n+m) reconciliation engine |
//! | `verify_chain_100k` | Verify 100K-entry hash chain | Chain integrity verification |

#![allow(clippy::inconsistent_digit_grouping)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use kromia_ledger::{
    reconcile, AccountType, Currency, Ledger, ReconcileRecord, ReconcileStatus,
};

// ── Deterministic seed for reproducible benchmarks ──────────────────────

const SEED: u64 = 0x004B_524F_4D1A_0001; // "KROM" + ledger v1

// ── Helpers ─────────────────────────────────────────────────────────────

/// Build a ledger with `n` accounts spread across 5 types, all in USD.
fn setup_accounts(ledger: &mut Ledger, rng: &mut ChaCha8Rng, n: usize) -> Vec<kromia_ledger::AccountId> {
    let usd = Currency::usd();
    let types = [
        AccountType::Asset,
        AccountType::Liability,
        AccountType::Equity,
        AccountType::Revenue,
        AccountType::Expense,
    ];
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let at = types[rng.random_range(0..types.len())];
        let code = format!("{:05}", i);
        let name = format!("Account-{i}");
        let id = ledger.create_account(&name, &code, at, usd.clone()).unwrap();
        ids.push(id);
    }
    ids
}

/// Pre-build a ledger with 100K entries and return it.
fn ledger_with_entries(n: usize) -> Ledger {
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    let mut ledger = Ledger::new();
    let accounts = setup_accounts(&mut ledger, &mut rng, 20);

    for i in 0..n {
        let amount = rng.random_range(1_00..100_000_00_i128); // $1 — $100,000
        let debit_acc = accounts[rng.random_range(0..accounts.len())];
        let mut credit_acc = accounts[rng.random_range(0..accounts.len())];
        while credit_acc == debit_acc {
            credit_acc = accounts[rng.random_range(0..accounts.len())];
        }
        let ts = 1_735_689_600 + (i as u64);
        ledger
            .record_transaction_at(
                "bench-tx",
                &[(debit_acc, amount)],
                &[(credit_acc, amount)],
                ts,
            )
            .unwrap();
    }
    ledger
}

/// Generate reconciliation datasets: `n` records with `anomaly_count` injected anomalies.
///
/// Anomaly breakdown (equal distribution):
/// - Missing from external (InternalOnly)
/// - Missing from internal (ExternalOnly)
/// - Amount mismatch
/// - Date mismatch
fn generate_reconcile_data(
    n: usize,
    anomaly_count: usize,
) -> (Vec<ReconcileRecord>, Vec<ReconcileRecord>) {
    let mut rng = ChaCha8Rng::seed_from_u64(SEED);

    // Anomaly budget: split into 4 categories
    let missing_external = anomaly_count / 4;
    let missing_internal = anomaly_count / 4;
    let amount_mismatch = anomaly_count / 4;
    let date_mismatch = anomaly_count - missing_external - missing_internal - amount_mismatch;

    let mut internal = Vec::with_capacity(n);
    let mut external = Vec::with_capacity(n);

    for i in 0..n {
        let id = format!("TX-{i:06}");
        let amount = rng.random_range(1_00..500_000_00_i128);
        let day = rng.random_range(1..=28_u32);
        let date = format!("2026-01-{day:02}");

        if i < missing_external {
            // Record exists in internal only
            internal.push(ReconcileRecord { id, amount, date });
            continue;
        }

        if i < missing_external + missing_internal {
            // Record exists in external only
            external.push(ReconcileRecord { id, amount, date });
            continue;
        }

        if i < missing_external + missing_internal + amount_mismatch {
            // Both exist but amount differs by $1
            internal.push(ReconcileRecord { id: id.clone(), amount, date: date.clone() });
            external.push(ReconcileRecord { id, amount: amount + 1_00, date });
            continue;
        }

        if i < missing_external + missing_internal + amount_mismatch + date_mismatch {
            // Both exist but date differs by 1 day
            let alt_day = if day < 28 { day + 1 } else { day - 1 };
            let alt_date = format!("2026-01-{alt_day:02}");
            internal.push(ReconcileRecord { id: id.clone(), amount, date });
            external.push(ReconcileRecord { id, amount, date: alt_date });
            continue;
        }

        // Normal matched records
        internal.push(ReconcileRecord { id: id.clone(), amount, date: date.clone() });
        external.push(ReconcileRecord { id, amount, date });
    }

    (internal, external)
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_record_transactions(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_transaction");
    group.sample_size(10); // 100K txns per iteration — 10 samples is plenty

    for &n in &[10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                ledger_with_entries(n);
            });
        });
    }

    group.finish();
}

fn bench_reconcile(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconcile");

    for &(n, anomalies) in &[(10_000_usize, 20_usize), (100_000, 50)] {
        let (internal, external) = generate_reconcile_data(n, anomalies);

        group.bench_with_input(
            BenchmarkId::new(format!("{n}_records_{anomalies}_anomalies"), n),
            &n,
            |b, _| {
                b.iter(|| {
                    let results = reconcile(&internal, &external);
                    // Ensure the engine actually found anomalies (not optimized away)
                    assert!(results.iter().any(|r| r.status != ReconcileStatus::Matched));
                });
            },
        );
    }

    group.finish();
}

fn bench_verify_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify_chain");
    group.sample_size(10);

    for &n in &[10_000, 100_000] {
        let ledger = ledger_with_entries(n);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                assert!(ledger.verify_chain());
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_record_transactions, bench_reconcile, bench_verify_chain);
criterion_main!(benches);
