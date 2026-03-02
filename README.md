# Kromia Ledger

> A deterministic, tamper-evident, double-entry financial ledger engine — written in Rust, runs anywhere including WebAssembly.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Tests](https://img.shields.io/badge/tests-109%20passing-brightgreen.svg)]()

---

## Why Kromia Ledger?

Most financial systems use floating-point math and mutable databases. Both are wrong for accounting:

- **Floating-point** is non-deterministic — `0.1 + 0.2 ≠ 0.3` on different architectures
- **Mutable records** can be silently edited — a $10,000 entry becomes $1,000 with no trace

Kromia Ledger solves both:

- **Fixed-point `i128` arithmetic** — zero floating-point, exact results on every platform
- **SHA-256 hash chain** — any modification to any historical entry is immediately detectable, forever

---

## Performance

Benchmarked on a standard laptop (`cargo bench`, release profile, seeded deterministic data):

| Workload | Scale | Time |
|---|---|---|
| Record transactions (SHA-256 chained) | 10,000 | **53 ms** |
| Record transactions (SHA-256 chained) | 100,000 | **532 ms** |
| Reconcile internal vs external dataset | 10K records, 20 anomalies | **6.7 ms** |
| Reconcile internal vs external dataset | 100K records, 50 anomalies | **93 ms** |
| Verify full hash chain | 10,000 entries | **39 ms** |
| Verify full hash chain | 100,000 entries | **371 ms** |

> **100,000 cryptographically-chained financial transactions recorded in under 1 second.**
> Reproduce: `cargo bench`

---

## Features

| Feature | Description |
|---|---|
| **Double-Entry Bookkeeping** | Every transaction enforces Σ Debit = Σ Credit — unbalanced entries are rejected |
| **Fixed-Point Arithmetic** | `i128`-based, zero floating-point, configurable precision per currency |
| **Cryptographic Hash Chain** | SHA-256 links every entry to its predecessor — tamper any record, the chain breaks |
| **Multi-Currency** | Per-account currency isolation — cross-currency mixing is a runtime error |
| **Currency Exchange** | Integer-scaled exchange rates (6 decimal precision) for cross-currency transactions |
| **Idempotency Keys** | Optional external key per transaction prevents double-processing |
| **Atomic Mutations** | All-or-nothing — validation runs before any state is mutated |
| **Reconciliation Engine** | O(n+m) matching of internal vs external datasets with 5-way mismatch classification |
| **Audit Trail** | Tamper-evident `AuditMeta` (actor, source, notes) — included in SHA-256 hash, query by actor |
| **Financial Reports** | Trial Balance, Balance Sheet, Income Statement, General Ledger — all `Serialize` for JSON export |
| **Storage Trait** | Pluggable `LedgerStore` backends — `MemoryStore` (WASM), `JsonFileStore` (native), or implement your own |
| **JSON Persistence** | Full ledger serialization with automatic chain integrity verification on restore |
| **WebAssembly Ready** | Compiles to native and WASM via `wasm-bindgen` — same logic, both targets |

---

## Quick Start

### Add to your project

```toml
[dependencies]
kromia-ledger = { git = "https://github.com/reyvanevan/kromia-ledger.git" }
```

### Run the full demo

```bash
cargo run --example quickstart
```

### Basic usage

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

let mut ledger = Ledger::new();

// Create accounts
let cash    = ledger.create_account("Cash",    "1000", AccountType::Asset,   Currency::usd()).unwrap();
let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

// Record a transaction (amounts in smallest unit — 150_00 = $150.00)
ledger.record_transaction(
    "Invoice payment received",
    &[(cash, 150_00)],    // debit: cash increases
    &[(revenue, 150_00)], // credit: revenue increases
).unwrap();

assert!(ledger.verify_chain());
assert_eq!(ledger.trial_balance(), 0);
```

### With idempotency key (prevent double-processing)

```rust
// First attempt — succeeds
ledger.record_transaction_full(
    "Order #A1234 payment",
    &[(cash, 500_00)],
    &[(revenue, 500_00)],
    1735689600,          // explicit UTC timestamp
    Some("ORDER-A1234"), // idempotency key
).unwrap();

// Retry with same key — rejected
let retry = ledger.record_transaction_full(
    "Order #A1234 payment",
    &[(cash, 500_00)],
    &[(revenue, 500_00)],
    1735689601,
    Some("ORDER-A1234"),
);
assert!(retry.is_err()); // DuplicateIdempotencyKey
```

### Cross-currency exchange

```rust
use kromia_ledger::RATE_SCALE;

let bank_usd = ledger.create_account("Bank USD", "1100", AccountType::Asset, Currency::usd()).unwrap();
let bank_idr = ledger.create_account("Bank IDR", "1200", AccountType::Asset, Currency::idr()).unwrap();

// Exchange $100.00 → IDR at 15,700 IDR/USD
// Rate: 1 USD cent = 157 IDR units → rate = 157 × RATE_SCALE
let rate       = 157 * RATE_SCALE;
let usd_amount = 10_000_i128;                      // $100.00 in cents
let idr_amount = usd_amount * rate / RATE_SCALE;   // 1,570,000 IDR

ledger.record_exchange(
    "USD to IDR — rate 15,700",
    bank_usd, usd_amount,
    bank_idr, idr_amount,
    rate,
).unwrap();
```

### Reconciliation

```rust
use kromia_ledger::{reconcile, ReconcileRecord, ReconcileStatus};

let internal = vec![
    ReconcileRecord { id: "TX001".into(), amount: 100_00, date: "2026-01-15".into() },
    ReconcileRecord { id: "TX002".into(), amount: 200_00, date: "2026-01-15".into() },
];
let external = vec![
    ReconcileRecord { id: "TX001".into(), amount: 99_00,  date: "2026-01-15".into() }, // amount mismatch
    ReconcileRecord { id: "TX003".into(), amount: 300_00, date: "2026-01-16".into() }, // bank-only
];

let results = reconcile(&internal, &external);
// TX001 → AmountMismatch { internal: 10000, external: 9900 }
// TX002 → InternalOnly  (missing in bank statement)
// TX003 → ExternalOnly  (missing in ledger)
```

### JSON Persistence + Tamper Detection

```rust
// Save full ledger state
let snapshot = ledger.save_json().unwrap();

// Restore — automatically verifies hash chain
let restored = Ledger::load_json(&snapshot).unwrap();
assert!(restored.verify_chain());

// Modify any byte in the snapshot → instant detection
let tampered = snapshot.replace("Invoice payment", "TAMPERED");
assert!(Ledger::load_json(&tampered).is_err()); // ChainBroken
```

### Balance Formatting

```rust
use kromia_ledger::{format_balance, format_balance_with_currency, parse_balance};

assert_eq!(format_balance(1_234_567_89),              "1,234,567.89");
assert_eq!(format_balance_with_currency(250_00, "$"), "$250.00");
assert_eq!(parse_balance("1,234.56").unwrap(),         1_234_56);
```

### Audit Trail

```rust
use kromia_ledger::AuditMeta;

let audit = AuditMeta::new("reyvan")
    .with_source("POST /api/v1/transactions")
    .with_notes("Monthly closing");

ledger.record_transaction_audited(
    "Salary payment",
    &[(cash, 5_000_00)],
    &[(revenue, 5_000_00)],
    1735689600,
    Some("SALARY-2026-01"),
    audit,
).unwrap();

// Query by actor
let entries = ledger.entries_by_actor("reyvan");
assert_eq!(entries.len(), 1);
```

### Financial Reports

```rust
// Trial Balance
let tb = ledger.trial_balance_report("USD");
assert_eq!(tb.total_debit, tb.total_credit);

// Balance Sheet (point-in-time)
let bs = ledger.balance_sheet("USD", 1735689600);
assert_eq!(bs.total_assets, bs.total_liabilities_equity);

// Income Statement (date range)
let is = ledger.income_statement("USD", 0, u64::MAX);
println!("Net income: {}", is.net_income);

// General Ledger (per-account detail with running balance)
let gl = ledger.general_ledger(cash, 0, u64::MAX);
for line in &gl.lines {
    println!("{}: debit={} credit={} balance={}",
        line.description, line.debit, line.credit, line.running_balance);
}
```

### Storage Backends

```rust
use kromia_ledger::store::{LedgerStore, MemoryStore};

// Save to memory store (works in WASM)
let mut store = MemoryStore::new();
store.save(&ledger).unwrap();

// Load back — chain verified automatically
let restored = store.load().unwrap();
assert!(restored.verify_chain());
```

```rust,ignore
use kromia_ledger::store::{LedgerStore, JsonFileStore};

// File-based persistence (native only)
let mut store = JsonFileStore::new("company-ledger.json");
store.save(&ledger).unwrap();
let restored = store.load().unwrap();
```

---

## WebAssembly

Build:

```bash
wasm-pack build --target web
```

Use from JavaScript/TypeScript:

```js
import init, { WasmLedger } from './pkg/kromia_ledger.js';

await init();
const ledger = new WasmLedger();

// create_account(name, code, type, currency_code, precision)
// type: 0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense
const cash = ledger.create_account("Cash",    "1000", 0, "USD", 2);
const rev  = ledger.create_account("Revenue", "4000", 3, "USD", 2);

ledger.record_transaction(JSON.stringify({
    description: "Payment",
    debits:  [[cash, 15000]],
    credits: [[rev,  15000]],
    idempotency_key: "ORDER-001",
}));

console.log("Chain valid:   ", ledger.verify_chain());
console.log("Trial balance: ", ledger.trial_balance());
console.log("Entry count:   ", ledger.entry_count());

const snapshot = ledger.save_json();
const restored = WasmLedger.load_json(snapshot);
```

---

## Architecture

```
kromia-ledger/
├── src/
│   ├── lib.rs          — Ledger struct, module declarations, public re-exports
│   ├── account.rs      — AccountId, AccountType, Currency, ExchangeRate, Account + balance ops
│   ├── audit.rs        — AuditMeta (actor, source, notes) — tamper-evident provenance
│   ├── transaction.rs  — TransactionLine, Transaction constructors + Ledger recording methods
│   ├── entry.rs        — LedgerEntry, SHA-256 hash computation, timestamp helpers
│   ├── exchange.rs     — Cross-currency exchange (Ledger methods)
│   ├── persistence.rs  — JSON save/load with automatic chain verification
│   ├── queries.rs      — Read-only queries, entries_for_account, verify_chain, trial_balance
│   ├── report.rs       — Financial reports: Trial Balance, Balance Sheet, Income Statement, General Ledger
│   ├── store.rs        — LedgerStore trait + MemoryStore (WASM) + JsonFileStore (native)
│   ├── types.rs        — Re-export hub for all core types
│   ├── validation.rs   — LedgerError enum (13 variants, thiserror)
│   ├── chain.rs        — HashChain: genesis → append → verify
│   ├── reconcile.rs    — O(n+m) reconciliation engine, 5-way status classification
│   ├── format.rs       — Balance ↔ human-readable string (thousands separator, configurable precision)
│   └── wasm.rs         — wasm-bindgen thin wrapper (cfg wasm32)
├── examples/
│   └── quickstart.rs   — Full API demo: accounts, transactions, exchange, reports, persistence
├── benches/
│   └── performance.rs  — Criterion benchmarks: 100K transactions, 100K reconciliation
└── tests/
    ├── account_tests.rs       — 9 tests
    ├── transaction_tests.rs   — 6 tests
    ├── exchange_tests.rs      — 8 tests
    ├── persistence_tests.rs   — 4 tests
    ├── audit_tests.rs         — 7 tests
    ├── report_tests.rs        — 19 tests
    └── store_tests.rs         — 13 tests
```

---

## API Reference

### `Ledger`

| Method | Description |
|---|---|
| `new()` | Create an empty ledger |
| **Account Management** | |
| `create_account(name, code, type, currency)` | Register a new account |
| `deactivate_account(id)` | Soft-disable an account |
| `get_account(id)` / `account_by_code(code)` | Look up an account |
| `get_balance(id)` | Current balance in smallest currency unit |
| `accounts()` | Iterator over all accounts |
| **Transaction Recording** | |
| `record_transaction(desc, debits, credits)` | Record with system clock |
| `record_transaction_at(desc, debits, credits, ts)` | Record with explicit UTC timestamp |
| `record_transaction_full(desc, debits, credits, ts, key)` | Full control: timestamp + idempotency key |
| `record_transaction_audited(desc, debits, credits, ts, key, audit)` | Full control + audit trail |
| **Currency Exchange** | |
| `record_exchange(desc, from, from_amt, to, to_amt, rate)` | Cross-currency exchange |
| `record_exchange_full(...)` | Exchange with explicit timestamp + idempotency key |
| `record_exchange_audited(...)` | Exchange with full audit trail |
| **Queries** | |
| `entries()` / `find_entry(id)` | Query ledger entries |
| `entries_for_account(id)` | All entries involving a specific account |
| `entries_in_range(from_ts, to_ts)` | Entries within a timestamp range |
| `entries_by_actor(actor)` | Entries by audit trail actor |
| `verify_chain()` | Validate entire SHA-256 hash chain |
| `trial_balance()` | Returns `0` for any balanced single-currency ledger |
| `trial_balance_by_currency()` | Per-currency trial balance map |
| **Financial Reports** | |
| `trial_balance_report(currency)` | All accounts with debit/credit columns |
| `balance_sheet(currency, as_of)` | Assets = Liabilities + Equity |
| `income_statement(currency, from, to)` | Revenue − Expenses = Net Income |
| `general_ledger(account_id, from, to)` | Per-account history with running balance |
| **Persistence** | |
| `save_json()` / `load_json(json)` | Serialize / restore with automatic integrity check |

### `LedgerStore` trait

| Method | Description |
|---|---|
| `save(&mut self, &Ledger)` | Persist entire ledger state |
| `load(&self) -> Result<Ledger>` | Restore with automatic chain verification |
| `has_data(&self) -> bool` | Check if store contains data |

Built-in backends: `MemoryStore` (tests, WASM), `JsonFileStore` (native, file-based).

### `AuditMeta`

| Method | Description |
|---|---|
| `new(actor)` | Create with actor identifier |
| `.with_source(source)` | Attach origin (IP, endpoint, etc.) |
| `.with_notes(notes)` | Attach free-form justification |

### `Currency`

```rust
Currency::usd()          // USD, precision = 2 (cents)
Currency::idr()          // IDR, precision = 0
Currency::eur()          // EUR, precision = 2
Currency::new("BTC", 8)  // any ISO 4217 code, custom precision
```

### Error Handling

All mutations return `Result<T, LedgerError>`:

| Variant | Cause |
|---|---|
| `Unbalanced` | Σ Debit ≠ Σ Credit |
| `EmptyTransaction` | No debit/credit lines provided |
| `InvalidAmount` | Any amount ≤ 0 |
| `AccountNotFound` | Account ID does not exist |
| `InactiveAccount` | Account was deactivated |
| `DuplicateAccountCode` | Account code already registered |
| `CurrencyMismatch` | Mixed currencies in one transaction |
| `DuplicateIdempotencyKey` | Idempotency key already used |
| `ExchangeRateMismatch` | `to_amount` doesn't match `from_amount × rate / RATE_SCALE` |
| `InvalidExchangeRate` | Rate ≤ 0 |
| `ChainBroken` | Hash chain integrity violation (tamper detected) |
| `Serialization` | JSON parse/serialize failure |
| `Storage` | Backend I/O error (file, memory, database) |

---

## Design Decisions

**Why `i128` and not `f64`?**
Floating-point arithmetic is non-deterministic across CPU architectures and WASM runtimes. `0.1 + 0.2 = 0.30000000000000004` is not acceptable in a financial system. `i128` supports values up to ±1.7×10³⁸ — enough for satoshis, wei, and any real-world currency at any scale.

**Why a hash chain?**
Each entry's SHA-256 hash is computed from its own content *and* its predecessor's hash. You cannot modify any historical entry without invalidating every subsequent hash. There is no silent rewrite of history.

**Why atomic mutations?**
The recording methods follow a strict 3-phase pattern: *(1) validate idempotency → (2) validate accounts, balances, currency — read-only → (3) mutate state*. Phase 3 can only be reached if phases 1 and 2 succeed. Partial corruption is structurally impossible.

---

## Development

```bash
# Run all 109 tests
cargo test

# Lint (zero warnings policy)
cargo clippy --all-targets

# Generate documentation
cargo doc --no-deps --open

# Run benchmarks
cargo bench

# Run interactive demo
cargo run --example quickstart

# Build for WebAssembly
wasm-pack build --target web
```

---

## Testing

```
109 tests total — 0 failures

Unit tests   (inline):     24  — chain (4), format (7), reconcile (5), report (8)
Integration  (tests/):     66  — account (9), transaction (6), exchange (8), persistence (4),
                                  audit (7), report (19), store (13)
Doc-tests:                 19  — API usage examples embedded in source docs
```

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

---

## Author

**M Reyvan Purnama** — [GitHub](https://github.com/reyvanevan) · [LinkedIn](https://linkedin.com/in/reyvanevan)
