# Kromia Ledger

A deterministic, immutable, and cryptographically chained financial ledger engine built in Rust.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Tests](https://img.shields.io/badge/tests-34%20passing-brightgreen.svg)]()

## Overview

Kromia Ledger is a military-grade, double-entry bookkeeping engine designed for absolute mathematical precision. It uses fixed-point arithmetic (`i128`) with zero floating-point operations, ensuring deterministic results across all platforms — including WebAssembly.

Every ledger entry is cryptographically chained via SHA-256, making the entire transaction history tamper-evident and auditable.

## Features

| Feature | Description |
|---|---|
| **Double-Entry Bookkeeping** | Every transaction enforces Σ Debit = Σ Credit. Unbalanced transactions are rejected. |
| **Fixed-Point Arithmetic** | `i128`-based. Zero floating-point. Precision configurable per currency. |
| **Cryptographic Chaining** | SHA-256 hash chain links every entry to its predecessor. Tamper = detected. |
| **Currency-Aware** | Each account carries a `Currency` (code + precision). Cross-currency transactions are rejected. |
| **Idempotency Keys** | Optional external key per transaction prevents double-processing. |
| **Atomic Mutations** | All-or-nothing — if any validation fails, ledger state is unchanged. |
| **Reconciliation Engine** | O(n+m) matching of internal vs. external datasets with mismatch classification. |
| **JSON Persistence** | Save/load full ledger state with automatic chain integrity verification on restore. |
| **WebAssembly Ready** | Compiles to both native and WASM via `wasm-bindgen`. |

## Quick Start

### Installation

```toml
# Cargo.toml
[dependencies]
kromia-ledger = { git = "https://github.com/reyvanevan/kromia-ledger.git" }
```

### Basic Usage (Rust)

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

fn main() {
    let mut ledger = Ledger::new();

    // Create accounts with currency
    let cash = ledger.create_account("Cash", "1000", AccountType::Asset, Currency::usd()).unwrap();
    let revenue = ledger.create_account("Revenue", "4000", AccountType::Revenue, Currency::usd()).unwrap();

    // Record a transaction (amount in smallest unit: 150_00 = $150.00)
    ledger.record_transaction(
        "Invoice payment received",
        &[(cash, 150_00)],    // debits
        &[(revenue, 150_00)], // credits
    ).unwrap();

    // Verify chain integrity
    assert!(ledger.verify_chain());
    assert_eq!(ledger.trial_balance(), 0);
}
```

### With Idempotency Key

```rust
// Prevent double-processing of the same order
ledger.record_transaction_full(
    "Order #1234",
    &[(cash, 500_00)],
    &[(revenue, 500_00)],
    1709251200,             // explicit UTC timestamp
    Some("ORDER-1234"),     // idempotency key
).unwrap();

// Second attempt with same key → Err(DuplicateIdempotencyKey)
let dup = ledger.record_transaction_full(
    "Order #1234 retry",
    &[(cash, 500_00)],
    &[(revenue, 500_00)],
    1709251201,
    Some("ORDER-1234"),
);
assert!(dup.is_err());
```

### Multi-Currency (IDR)

```rust
use kromia_ledger::{Ledger, AccountType, Currency};

let mut ledger = Ledger::new();
let kas = ledger.create_account("Kas", "1100", AccountType::Asset, Currency::idr()).unwrap();
let pendapatan = ledger.create_account("Pendapatan", "4100", AccountType::Revenue, Currency::idr()).unwrap();

// IDR has precision=0, so 500_000 = Rp 500.000
ledger.record_transaction(
    "Penjualan",
    &[(kas, 500_000)],
    &[(pendapatan, 500_000)],
).unwrap();

// Cross-currency is rejected:
let cash_usd = ledger.create_account("Cash USD", "1200", AccountType::Asset, Currency::usd()).unwrap();
let result = ledger.record_transaction(
    "Invalid",
    &[(cash_usd, 100_00)],
    &[(pendapatan, 100_00)],  // pendapatan is IDR!
);
assert!(result.is_err()); // CurrencyMismatch
```

### Reconciliation

```rust
use kromia_ledger::{reconcile, ReconcileRecord, ReconcileStatus};

let internal = vec![
    ReconcileRecord { id: "TX001".into(), amount: 100_00, date: "2026-03-01".into() },
    ReconcileRecord { id: "TX002".into(), amount: 200_00, date: "2026-03-01".into() },
];
let external = vec![
    ReconcileRecord { id: "TX001".into(), amount: 99_00,  date: "2026-03-01".into() },
    ReconcileRecord { id: "TX003".into(), amount: 300_00, date: "2026-03-02".into() },
];

let results = reconcile(&internal, &external);
// TX001 → AmountMismatch { internal: 10000, external: 9900 }
// TX002 → InternalOnly
// TX003 → ExternalOnly
```

### Balance Formatting

```rust
use kromia_ledger::{format_balance, format_balance_with_currency, parse_balance};

assert_eq!(format_balance(1_234_567_89), "1,234,567.89");
assert_eq!(format_balance_with_currency(250_00, "$"), "$250.00");
assert_eq!(format_balance_with_currency(1_500_000_00, "Rp"), "Rp1,500,000.00");

assert_eq!(parse_balance("1,234.56").unwrap(), 1_234_56);
```

### JSON Persistence

```rust
// Save
let json = ledger.save_json().unwrap();
std::fs::write("ledger.json", &json).unwrap();

// Load (automatically verifies chain integrity)
let json = std::fs::read_to_string("ledger.json").unwrap();
let restored = Ledger::load_json(&json).unwrap();
assert!(restored.verify_chain());
```

### WebAssembly (JavaScript/TypeScript)

Build:

```bash
wasm-pack build --target web
```

Usage:

```js
import init, { WasmLedger } from './pkg/kromia_ledger.js';

await init();
const ledger = new WasmLedger();

// create_account(name, code, type, currency_code, precision)
// type: 0=Asset, 1=Liability, 2=Equity, 3=Revenue, 4=Expense
const cash = ledger.create_account("Cash", "1000", 0, "USD", 2);
const rev  = ledger.create_account("Revenue", "4000", 3, "USD", 2);

ledger.record_transaction(JSON.stringify({
    description: "Payment",
    debits: [[cash, 15000]],
    credits: [[rev, 15000]],
    idempotency_key: "ORDER-001"   // optional
}));

console.log("Chain valid:", ledger.verify_chain());
console.log("Trial balance:", ledger.trial_balance());
console.log("Entries:", ledger.entry_count());

// Persistence
const snapshot = ledger.save_json();
const restored = WasmLedger.load_json(snapshot);
```

## Architecture

```
src/
├── lib.rs          — Ledger struct, public API, atomic transaction engine
├── types.rs        — Balance (i128), AccountId, AccountType, Currency,
│                     Account, Transaction, LedgerEntry, hash computation
├── validation.rs   — LedgerError enum (10 error variants)
├── chain.rs        — HashChain: SHA-256 genesis → append → verify
├── reconcile.rs    — O(n+m) reconciliation with 5-way status classification
├── format.rs       — Balance ↔ human-readable string (with thousands sep)
└── wasm.rs         — wasm-bindgen interface for JS/TS consumption
```

## API Reference

### `Ledger`

| Method | Description |
|---|---|
| `new()` | Create empty ledger |
| `create_account(name, code, type, currency)` | Register a new account |
| `deactivate_account(id)` | Soft-disable an account |
| `get_account(id)` / `account_by_code(code)` | Lookup account |
| `get_balance(id)` | Current balance of an account |
| `accounts()` | Iterate all accounts |
| `record_transaction(desc, debits, credits)` | Record with auto-timestamp |
| `record_transaction_at(desc, debits, credits, ts)` | Record with explicit timestamp |
| `record_transaction_full(desc, debits, credits, ts, key)` | Full control: timestamp + idempotency |
| `entries()` / `find_entry(id)` | Query ledger entries |
| `entries_for_account(id)` | Entries involving a specific account |
| `entries_in_range(from, to)` | Entries within a timestamp range |
| `verify_chain()` | Validate entire SHA-256 hash chain |
| `trial_balance()` | Must return 0 if everything is correct |
| `save_json()` / `load_json(json)` | Serialize/restore with integrity check |

### `Currency`

| Constructor | Code | Precision |
|---|---|---|
| `Currency::usd()` | USD | 2 |
| `Currency::idr()` | IDR | 0 |
| `Currency::eur()` | EUR | 2 |
| `Currency::new("JPY", 0)` | Any | Custom |

## Error Handling

All operations return `Result<T, LedgerError>`. Error variants:

| Error | Cause |
|---|---|
| `Unbalanced` | Σ Debit ≠ Σ Credit |
| `EmptyTransaction` | No debit/credit lines |
| `InvalidAmount` | Amount ≤ 0 |
| `AccountNotFound` | Account ID doesn't exist |
| `InactiveAccount` | Account was deactivated |
| `DuplicateAccountCode` | Account code already in use |
| `CurrencyMismatch` | Mixed currencies in one transaction |
| `DuplicateIdempotencyKey` | Idempotency key already used |
| `ChainBroken` | Hash chain integrity violation |
| `Serialization` | JSON serialize/deserialize failure |

## Design Decisions

- **Why `i128`?** — Supports values up to ±1.7×10³⁸, enough for any real-world currency without overflow, even at the smallest unit (satoshis, wei, etc.).
- **Why not `f64`?** — Floating-point is non-deterministic across architectures. `0.1 + 0.2 ≠ 0.3` breaks financial systems. Fixed-point is absolute.
- **Why hash chain?** — Each entry's hash includes its predecessor. Changing any historical entry invalidates all subsequent hashes. You cannot silently rewrite history.
- **Why atomic?** — Validation happens in read-only phases. Mutation only begins after all checks pass. Partial state corruption is structurally impossible.

## Building

```bash
# Native (release)
cargo build --release

# WebAssembly
wasm-pack build --target web

# Run tests
cargo test

# Lint
cargo clippy
```

## Testing

```bash
$ cargo test

running 34 tests
...
test result: ok. 34 passed; 0 failed
```

Test coverage:

| Module | Tests | What's Covered |
|---|---|---|
| `chain` | 4 | Genesis, chaining, tamper detection, serialization roundtrip |
| `format` | 7 | Format, thousands, currency prefix, parse, roundtrip, edge cases |
| `reconcile` | 5 | Match, mismatch, internal-only, external-only, 10k performance |
| `ledger` | 18 | Balance, atomicity, inactive, duplicates, determinism, currency mismatch, idempotency, persistence, tamper |

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Author

**M Reyvan Purnama** — [GitHub](https://github.com/reyvanevan)
