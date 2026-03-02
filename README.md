# Kromia Ledger

> A deterministic, tamper-evident, double-entry financial ledger engine — written in Rust, runs anywhere including WebAssembly.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Tests](https://img.shields.io/badge/tests-109%20passing-brightgreen.svg)]()
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

---

## Table of Contents

- [Why Kromia Ledger?](#why-kromia-ledger)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Core Concepts](#core-concepts)
- [Features](#features)
- [Usage Examples](#usage-examples)
- [WebAssembly](#webassembly)
- [Performance](#performance)
- [Architecture](#architecture)
- [API Reference](#api-reference)
- [Error Handling](#error-handling)
- [Design Decisions](#design-decisions)
- [Development](#development)
- [Testing](#testing)
- [Contributing](#contributing)
- [License](#license)

---

## Why Kromia Ledger?

Most financial systems use floating-point math and mutable databases. Both are wrong for accounting:

- **Floating-point** is non-deterministic — `0.1 + 0.2 ≠ 0.3` on different architectures
- **Mutable records** can be silently edited — a $10,000 entry becomes $1,000 with no trace

Kromia Ledger solves both:

- **Fixed-point `i128` arithmetic** — zero floating-point, exact results on every platform
- **SHA-256 hash chain** — any modification to any historical entry is immediately detectable, forever

---

## Prerequisites

| Requirement | Version | Check |
|---|---|---|
| **Rust** | 1.85+ (edition 2024) | `rustc --version` |
| **Cargo** | comes with Rust | `cargo --version` |
| **wasm-pack** *(optional, for WASM)* | latest | `wasm-pack --version` |

Install Rust: [https://rustup.rs](https://rustup.rs)

Install wasm-pack (only if you need WebAssembly):
```bash
cargo install wasm-pack
```

---

## Quick Start

### Add to your project

```toml
[dependencies]
kromia-ledger = { git = "https://github.com/reyvanevan/kromia-ledger.git" }
```

### Run the interactive demo

```bash
cargo run --example quickstart
```

### Minimal example

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

// Verify integrity
assert!(ledger.verify_chain());   // SHA-256 chain is valid
assert_eq!(ledger.trial_balance(), 0); // books are balanced
```

That's it — a working ledger in 10 lines. Read on for the concepts behind it.

---

## Core Concepts

### Amounts: Fixed-Point Integers

> **This is the most important thing to understand.** All monetary values are integers in the **smallest currency unit**.

| You write | It means | Currency unit |
|---|---|---|
| `150_00` | $150.00 | cents (USD, precision=2) |
| `1_000_000` | Rp 1,000,000 | rupiah (IDR, precision=0) |
| `1_50` | €1.50 | cents (EUR, precision=2) |
| `100_000_000` | 1.00000000 BTC | satoshis (BTC, precision=8) |

Rust's `_` separator is just for readability — `150_00` and `15000` are the same number.

**Why not floats?** Because `0.1 + 0.2 = 0.30000000000000004` in IEEE 754. In accounting, that's a bug. With integers, `10 + 20 = 30` — always, on every CPU, on every WASM runtime.

The type is `i128`, supporting values up to ±1.7×10³⁸ — enough for satoshis, wei, and any real-world currency at any scale.

### Double-Entry Bookkeeping

Every transaction in Kromia Ledger has two sides: **debits** and **credits**. The engine enforces that they must always be equal:

```
Σ Debits = Σ Credits  (for every transaction)
```

If you try to record a transaction where they don't match, you get `LedgerError::Unbalanced`.

**What do debit and credit mean?**

| Account Type | Debit (increases) | Credit (increases) |
|---|---|---|
| **Asset** (cash, bank, inventory) | ← Balance goes up | Balance goes down → |
| **Expense** (rent, salary) | ← Balance goes up | Balance goes down → |
| **Liability** (loans, payable) | Balance goes down → | ← Balance goes up |
| **Equity** (owner's capital) | Balance goes down → | ← Balance goes up |
| **Revenue** (sales, interest) | Balance goes down → | ← Balance goes up |

**Example:** You receive $150 cash from a sale.

```
Debit  Cash (Asset)     $150  ← cash increases
Credit Revenue          $150  ← revenue increases
                        ────
                Total:  $150 = $150 ✓
```

The `trial_balance()` method returns 0 when all transactions are balanced — it's a quick integrity check.

### SHA-256 Hash Chain

Every entry in the ledger includes a SHA-256 hash computed from:

1. The entry's own content (description, amounts, timestamp, audit info)
2. The **previous entry's hash**

```
Entry #1          Entry #2          Entry #3
┌──────────┐     ┌──────────┐     ┌──────────┐
│ data     │     │ data     │     │ data     │
│ prev: 00 │────→│ prev: a7 │────→│ prev: 3f │
│ hash: a7 │     │ hash: 3f │     │ hash: b2 │
└──────────┘     └──────────┘     └──────────┘
```

If someone changes Entry #1's data, its hash changes → Entry #2's `prev` no longer matches → the chain is broken. `verify_chain()` catches this instantly.

This is the same principle behind blockchain, but simpler — no consensus mechanism, no mining, just a tamper-evident audit trail.

### Account Types

Kromia Ledger uses five standard account types from accounting:

```rust
AccountType::Asset      // 0 — things you own (cash, inventory, receivables)
AccountType::Liability  // 1 — things you owe (loans, payables)
AccountType::Equity     // 2 — owner's stake (capital, retained earnings)
AccountType::Revenue    // 3 — income (sales, interest, fees)
AccountType::Expense    // 4 — costs (rent, salary, utilities)
```

### Currencies

Each account has exactly one currency. You cannot mix currencies in a single transaction — use `record_exchange()` for cross-currency operations.

```rust
Currency::usd()          // USD, precision = 2 (cents)
Currency::idr()          // IDR, precision = 0 (no subunit)
Currency::eur()          // EUR, precision = 2
Currency::new("BTC", 8)  // custom: 8 decimal places (satoshis)
```

---

## Features

| Feature | Description |
|---|---|
| **Double-Entry Bookkeeping** | Every transaction enforces Σ Debit = Σ Credit — unbalanced entries are rejected |
| **Fixed-Point Arithmetic** | `i128`-based, zero floating-point, configurable precision per currency |
| **Cryptographic Hash Chain** | SHA-256 links every entry to its predecessor — tamper any record, the chain breaks |
| **Multi-Currency** | Per-account currency isolation — cross-currency mixing is a compile-time safe, runtime error |
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

## Usage Examples

Every code block below is **self-contained** — you can copy-paste any one directly.

### Basic Transaction

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

let mut ledger = Ledger::new();
let cash    = ledger.create_account("Cash",    "1000", AccountType::Asset,   Currency::usd()).unwrap();
let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

ledger.record_transaction(
    "Invoice payment received",
    &[(cash, 150_00)],    // debit $150.00
    &[(revenue, 150_00)], // credit $150.00
).unwrap();

assert!(ledger.verify_chain());
assert_eq!(ledger.trial_balance(), 0);
```

### Idempotency Keys (Prevent Double-Processing)

An idempotency key ensures a transaction is recorded exactly once — even if the caller retries.

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

let mut ledger = Ledger::new();
let cash    = ledger.create_account("Cash",    "1000", AccountType::Asset,   Currency::usd()).unwrap();
let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

// First attempt — succeeds
ledger.record_transaction_full(
    "Order #A1234 payment",
    &[(cash, 500_00)],
    &[(revenue, 500_00)],
    1735689600,              // explicit UTC timestamp
    Some("ORDER-A1234"),     // idempotency key
).unwrap();

// Retry with same key — safely rejected (no duplicate entry)
let retry = ledger.record_transaction_full(
    "Order #A1234 payment",
    &[(cash, 500_00)],
    &[(revenue, 500_00)],
    1735689601,
    Some("ORDER-A1234"),
);
assert!(retry.is_err()); // LedgerError::DuplicateIdempotencyKey
```

### Cross-Currency Exchange

Exchange between two accounts with different currencies. The rate is integer-scaled using `RATE_SCALE` (1,000,000) for 6-decimal precision.

```rust
use kromia_ledger::{Ledger, AccountType, Currency, RATE_SCALE};

let mut ledger = Ledger::new();
let bank_usd = ledger.create_account("Bank USD", "1100", AccountType::Asset, Currency::usd()).unwrap();
let bank_idr = ledger.create_account("Bank IDR", "1200", AccountType::Asset, Currency::idr()).unwrap();

// Fund the USD account first
let equity = ledger.create_account("Owner Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();
ledger.record_transaction("Initial funding", &[(bank_usd, 10_000_00)], &[(equity, 10_000_00)]).unwrap();

// Exchange $100.00 → IDR at rate 15,700 IDR per USD
//
// How the rate works:
//   - bank_usd uses cents (precision=2), so $100.00 = 10_000 cents
//   - bank_idr uses whole rupiah (precision=0), so Rp 1,570,000 = 1_570_000
//   - Rate = (to_amount / from_amount) × RATE_SCALE
//         = (1_570_000 / 10_000) × 1_000_000
//         = 157 × 1_000_000
//         = 157_000_000
let rate       = 157 * RATE_SCALE;                  // 157_000_000
let usd_amount = 10_000_i128;                       // $100.00 in cents
let idr_amount = usd_amount * rate / RATE_SCALE;    // 1,570,000 IDR

ledger.record_exchange(
    "USD to IDR — rate 15,700",
    bank_usd, usd_amount,
    bank_idr, idr_amount,
    rate,
).unwrap();

assert_eq!(ledger.get_balance(bank_usd).unwrap(), 10_000_00 - 10_000); // 9,900.00 in cents
assert_eq!(ledger.get_balance(bank_idr).unwrap(), 1_570_000);           // Rp 1,570,000
```

### Reconciliation

Compare your internal records against an external dataset (e.g., bank statement) with O(n+m) performance.

```rust
use kromia_ledger::{reconcile, ReconcileRecord, ReconcileStatus};

let internal = vec![
    ReconcileRecord { id: "TX001".into(), amount: 100_00, date: "2026-01-15".into() },
    ReconcileRecord { id: "TX002".into(), amount: 200_00, date: "2026-01-15".into() },
];
let external = vec![
    ReconcileRecord { id: "TX001".into(), amount: 99_00,  date: "2026-01-15".into() },
    ReconcileRecord { id: "TX003".into(), amount: 300_00, date: "2026-01-16".into() },
];

let results = reconcile(&internal, &external);

// TX001 → AmountMismatch { internal: 10000, external: 9900 }
// TX002 → InternalOnly  (exists in your books, missing in bank statement)
// TX003 → ExternalOnly  (exists in bank statement, missing in your books)

assert_eq!(results.len(), 3);
```

Five possible statuses: `Matched`, `AmountMismatch`, `DateMismatch`, `MultipleMismatch`, `InternalOnly`, `ExternalOnly`.

### JSON Persistence + Tamper Detection

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

let mut ledger = Ledger::new();
let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
let eq   = ledger.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();
ledger.record_transaction("Invest", &[(cash, 500_00)], &[(eq, 500_00)]).unwrap();

// Save full ledger state
let snapshot = ledger.save_json().unwrap();

// Restore — hash chain is automatically verified
let restored = Ledger::load_json(&snapshot).unwrap();
assert!(restored.verify_chain());

// Tamper any byte → instant detection
let tampered = snapshot.replace("Invest", "TAMPERED");
assert!(Ledger::load_json(&tampered).is_err()); // LedgerError::ChainBroken
```

### Balance Formatting

Convert raw integer amounts to human-readable strings and back.

```rust
use kromia_ledger::{format_balance, format_balance_with_currency, parse_balance};

// Format: integer → string
assert_eq!(format_balance(1_234_567_89),              "1,234,567.89");
assert_eq!(format_balance_with_currency(250_00, "$"), "$250.00");

// Parse: string → integer
assert_eq!(parse_balance("1,234.56").unwrap(), 1_234_56);
```

### Audit Trail

Attach who, where, and why to every transaction — included in the SHA-256 hash.

```rust
use kromia_ledger::{Ledger, AccountType, Currency, AuditMeta};

let mut ledger = Ledger::new();
let cash    = ledger.create_account("Cash",   "1000", AccountType::Asset,   Currency::usd()).unwrap();
let revenue = ledger.create_account("Revenue","4000", AccountType::Revenue, Currency::usd()).unwrap();

let audit = AuditMeta::new("reyvan")                       // who
    .with_source("POST /api/v1/transactions")               // where
    .with_notes("Monthly closing adjustment");              // why

ledger.record_transaction_audited(
    "Adjustment entry",
    &[(cash, 5_000_00)],
    &[(revenue, 5_000_00)],
    1735689600,                 // UTC timestamp
    Some("ADJ-2026-01"),        // idempotency key
    audit,
).unwrap();

// Query all entries by a specific actor
let entries = ledger.entries_by_actor("reyvan");
assert_eq!(entries.len(), 1);
assert_eq!(entries[0].audit.as_ref().unwrap().actor, "reyvan");
```

### Financial Reports

Generate standard accounting reports — all serializable to JSON.

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

let mut ledger = Ledger::new();
let cash    = ledger.create_account("Cash",    "1000", AccountType::Asset,   Currency::usd()).unwrap();
let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();
let expense = ledger.create_account("Rent",    "5000", AccountType::Expense, Currency::usd()).unwrap();
let equity  = ledger.create_account("Equity",  "3000", AccountType::Equity,  Currency::usd()).unwrap();

ledger.record_transaction("Initial capital", &[(cash, 10_000_00)], &[(equity, 10_000_00)]).unwrap();
ledger.record_transaction("Sales",           &[(cash, 3_000_00)],  &[(revenue, 3_000_00)]).unwrap();
ledger.record_transaction("Rent payment",    &[(expense, 500_00)], &[(cash, 500_00)]).unwrap();

// Trial Balance — all accounts with debit/credit columns
let tb = ledger.trial_balance_report("USD");
assert_eq!(tb.total_debit, tb.total_credit); // always balanced

// Balance Sheet — Assets = Liabilities + Equity
let bs = ledger.balance_sheet("USD", 1735689600);
assert_eq!(bs.total_assets, bs.total_liabilities_equity);

// Income Statement — Revenue - Expenses = Net Income
let is_report = ledger.income_statement("USD", 0, u64::MAX);
assert_eq!(is_report.net_income, 3_000_00 - 500_00); // $2,500.00

// General Ledger — per-account detail with running balance
let gl = ledger.general_ledger(cash, 0, u64::MAX);
assert_eq!(gl.lines.len(), 3); // 3 transactions touched cash
assert_eq!(gl.closing_balance, 10_000_00 + 3_000_00 - 500_00); // $12,500.00
```

### Storage Backends

Persist and restore ledgers with pluggable backends.

```rust
use kromia_ledger::{Ledger, AccountType, Currency};
use kromia_ledger::store::{LedgerStore, MemoryStore};

let mut ledger = Ledger::new();
let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
let eq   = ledger.create_account("Equity", "3000", AccountType::Equity, Currency::usd()).unwrap();
ledger.record_transaction("Fund", &[(cash, 1_000_00)], &[(eq, 1_000_00)]).unwrap();

// Save to memory store (also works in WASM)
let mut store = MemoryStore::new();
store.save(&ledger).unwrap();

// Load back — chain is verified automatically
let restored = store.load().unwrap();
assert!(restored.verify_chain());
assert_eq!(restored.get_balance(cash).unwrap(), 1_000_00);
```

File-based persistence (native targets only):

```rust,ignore
use kromia_ledger::store::{LedgerStore, JsonFileStore};

let mut store = JsonFileStore::new("company-ledger.json");
store.save(&ledger).unwrap();
let restored = store.load().unwrap();
```

Implement `LedgerStore` for any backend (PostgreSQL, S3, Redis, etc.) — the trait is intentionally minimal: `save()`, `load()`, `has_data()`.

---

## WebAssembly

Kromia Ledger compiles to WASM — the same engine runs in the browser with zero servers.

### Build

```bash
wasm-pack build --target web
```

### Use from JavaScript / TypeScript

```js
import init, { WasmLedger } from './pkg/kromia_ledger.js';

await init();
const ledger = new WasmLedger();

// create_account(name, code, type, currency_code, precision)
// Account types: 0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense
const cash = ledger.create_account("Cash",    "1000", 0, "USD", 2);
const rev  = ledger.create_account("Revenue", "4000", 3, "USD", 2);

// Record a transaction via JSON
ledger.record_transaction(JSON.stringify({
    description: "Payment received",
    debits:  [[cash, 15000]],   // $150.00 in cents
    credits: [[rev,  15000]],
    idempotency_key: "ORDER-001",           // optional
    audit: { actor: "web-user" },           // optional
}));

console.log("Chain valid:   ", ledger.verify_chain());   // true
console.log("Trial balance: ", ledger.trial_balance());  // 0
console.log("Entry count:   ", ledger.entry_count());    // 1
console.log("Cash balance:  ", ledger.get_balance_formatted(cash)); // "150.00"

// Save / restore
const snapshot = ledger.save_json();
const restored = WasmLedger.load_json(snapshot);
```

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

| Method | Returns | Description |
|---|---|---|
| `new()` | `Ledger` | Create an empty ledger with a fresh hash chain |
| **Account Management** | | |
| `create_account(name, code, type, currency)` | `Result<AccountId>` | Register a new account |
| `deactivate_account(id)` | `Result<()>` | Soft-disable an account (rejects future transactions) |
| `get_account(id)` | `Option<&Account>` | Look up an account by ID |
| `account_by_code(code)` | `Option<&Account>` | Look up an account by its code (e.g., "1000") |
| `get_balance(id)` | `Option<Balance>` | Current balance in smallest currency unit |
| `accounts()` | `impl Iterator<&Account>` | Iterate over all accounts (sorted by ID) |
| **Transaction Recording** | | |
| `record_transaction(desc, debits, credits)` | `Result<u64>` | Record with system clock — returns entry ID |
| `record_transaction_at(desc, debits, credits, ts)` | `Result<u64>` | Record with explicit UTC timestamp |
| `record_transaction_full(desc, debits, credits, ts, key)` | `Result<u64>` | Full control: timestamp + idempotency key |
| `record_transaction_audited(desc, debits, credits, ts, key, audit)` | `Result<u64>` | Full control + audit trail |
| **Currency Exchange** | | |
| `record_exchange(desc, from, from_amt, to, to_amt, rate)` | `Result<u64>` | Cross-currency exchange — returns entry ID |
| `record_exchange_full(...)` | `Result<u64>` | Exchange with explicit timestamp + idempotency key |
| `record_exchange_audited(...)` | `Result<u64>` | Exchange with full audit trail |
| **Queries** | | |
| `entries()` | `&[LedgerEntry]` | All ledger entries (chronological) |
| `find_entry(id)` | `Option<&LedgerEntry>` | Look up a specific entry by ID |
| `entries_for_account(id)` | `Vec<&LedgerEntry>` | All entries involving a specific account |
| `entries_in_range(from_ts, to_ts)` | `Vec<&LedgerEntry>` | Entries within a timestamp range |
| `entries_by_actor(actor)` | `Vec<&LedgerEntry>` | Entries by audit trail actor |
| `verify_chain()` | `bool` | Validate entire SHA-256 hash chain |
| `trial_balance()` | `Balance` | Sum of all debits − credits (0 when balanced) |
| `trial_balance_by_currency()` | `BTreeMap<String, Balance>` | Per-currency trial balance map |
| **Financial Reports** | | |
| `trial_balance_report(currency)` | `TrialBalanceReport` | All accounts with debit/credit columns |
| `balance_sheet(currency, as_of)` | `BalanceSheet` | Assets = Liabilities + Equity |
| `income_statement(currency, from, to)` | `IncomeStatement` | Revenue − Expenses = Net Income |
| `general_ledger(account_id, from, to)` | `GeneralLedgerReport` | Per-account history with running balance |
| **Persistence** | | |
| `save_json()` | `Result<String>` | Serialize entire ledger to pretty-printed JSON |
| `load_json(json)` *(static)* | `Result<Ledger>` | Restore from JSON with automatic chain verification |

### `LedgerStore` trait

| Method | Returns | Description |
|---|---|---|
| `save(&mut self, &Ledger)` | `Result<()>` | Persist entire ledger state |
| `load(&self)` | `Result<Ledger>` | Restore with automatic chain verification |
| `has_data(&self)` | `bool` | Check if store contains previously saved data |

Built-in backends: `MemoryStore` (tests, WASM), `JsonFileStore` (native, file-based).

### `AuditMeta`

| Method | Returns | Description |
|---|---|---|
| `new(actor)` | `AuditMeta` | Create with actor identifier (user ID, API key, etc.) |
| `.with_source(source)` | `AuditMeta` | Attach origin (IP address, API endpoint, etc.) |
| `.with_notes(notes)` | `AuditMeta` | Attach free-form justification |

### `Currency`

```rust
Currency::usd()          // USD, precision = 2 (cents)
Currency::idr()          // IDR, precision = 0 (no subunit)
Currency::eur()          // EUR, precision = 2
Currency::new("BTC", 8)  // any code, custom precision (satoshis)
```

---

## Error Handling

All mutations return `Result<T, LedgerError>`. Every variant is designed to be actionable:

| Variant | Cause | Typical Fix |
|---|---|---|
| `Unbalanced` | Σ Debit ≠ Σ Credit | Fix the amounts so they match |
| `EmptyTransaction` | No debit/credit lines provided | Add at least one line on each side |
| `InvalidAmount` | Any amount ≤ 0 | Use positive integers only |
| `AccountNotFound` | Account ID does not exist | Check the ID from `create_account()` |
| `InactiveAccount` | Account was deactivated | Re-activate or use a different account |
| `DuplicateAccountCode` | Account code already registered | Use a unique code |
| `CurrencyMismatch` | Mixed currencies in one transaction | Use `record_exchange()` instead |
| `DuplicateIdempotencyKey` | Idempotency key already used | This is expected on retries — safe to ignore |
| `ExchangeRateMismatch` | `to_amount ≠ from_amount × rate / RATE_SCALE` | Recalculate the amounts |
| `InvalidExchangeRate` | Rate ≤ 0 | Use a positive rate |
| `ChainBroken` | Hash chain integrity violation | Data was tampered — do not trust this ledger |
| `Serialization` | JSON parse/serialize failure | Check the JSON string for syntax errors |
| `Storage` | Backend I/O error | Check file permissions, disk space, etc. |

---

## Design Decisions

**Why `i128` and not `f64`?**
Floating-point arithmetic is non-deterministic across CPU architectures and WASM runtimes. `0.1 + 0.2 = 0.30000000000000004` is not acceptable in a financial system. `i128` supports values up to ±1.7×10³⁸ — enough for satoshis, wei, and any real-world currency at any scale.

**Why a hash chain?**
Each entry's SHA-256 hash is computed from its own content *and* its predecessor's hash. You cannot modify any historical entry without invalidating every subsequent hash. There is no silent rewrite of history.

**Why atomic mutations?**
The recording methods follow a strict 3-phase pattern: *(1) validate idempotency → (2) validate accounts, balances, currency — read-only → (3) mutate state*. Phase 3 can only be reached if phases 1 and 2 succeed. Partial corruption is structurally impossible.

**Why integers for exchange rates?**
`RATE_SCALE = 1_000_000` gives 6-decimal precision for exchange rates without floating-point. The engine verifies that `to_amount == from_amount * rate / RATE_SCALE` — any rounding discrepancy is rejected.

---

## Development

```bash
# Run all 109 tests
cargo test

# Lint (zero warnings policy)
cargo clippy --all-targets

# Generate API documentation
cargo doc --no-deps --open

# Run benchmarks
cargo bench

# Run interactive demo
cargo run --example quickstart

# Build for WebAssembly
wasm-pack build --target web
```

### Minimum Supported Rust Version (MSRV)

**Rust 1.85** (edition 2024). Tested on stable. No nightly features required.

---

## Testing

```
109 tests total — 0 failures

Unit tests   (inline):     24  — chain (4), format (7), reconcile (5), report (8)
Integration  (tests/):     66  — account (9), transaction (6), exchange (8), persistence (4),
                                  audit (7), report (19), store (13)
Doc-tests:                 19  — API usage examples embedded in source docs
```

Run specific test suites:

```bash
cargo test --test account_tests      # just account tests
cargo test --test report_tests       # just report tests
cargo test --doc                     # just doc-tests
```

---

## Contributing

Contributions are welcome! Here's how:

1. **Fork** the repo and create a feature branch
2. **Write tests** for any new functionality
3. Ensure **all 109 tests pass**: `cargo test`
4. Ensure **zero clippy warnings**: `cargo clippy --all-targets`
5. Ensure **zero rustdoc warnings**: `cargo doc --no-deps`
6. Open a **pull request** with a clear description

### Code Style

- Zero `unsafe` — the entire codebase is safe Rust
- Zero `unwrap()` in library code — all errors are typed via `LedgerError`
- Every public method has a doc-comment with an example
- Tests cover both success and error paths

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

---

## Author

**M Reyvan Purnama** — [GitHub](https://github.com/reyvanevan) · [LinkedIn](https://linkedin.com/in/reyvanevan)
