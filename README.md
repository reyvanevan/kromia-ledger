# Kromia Ledger

A deterministic, immutable, and cryptographically chained financial ledger engine built in Rust.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## Overview

Kromia Ledger is a high-performance, double-entry bookkeeping engine designed for absolute mathematical precision. It uses fixed-point arithmetic (`i128`) with zero floating-point operations, ensuring deterministic results across all platforms — including WebAssembly.

Every ledger entry is cryptographically chained via SHA-256, making the entire history tamper-evident and auditable.

## Features

- **Double-Entry Bookkeeping** — Every transaction enforces Σ Debit = Σ Credit
- **Fixed-Point Arithmetic** — `i128`-based, 2-decimal precision (1.00 = 100 internal units)
- **Cryptographic Chaining** — SHA-256 hash chain for immutable, tamper-detectable history
- **Reconciliation Engine** — High-speed matching of internal vs. external datasets in O(n log n)
- **WebAssembly Ready** — Compiles to both native and WASM targets via `wasm-bindgen`
- **Zero Floating-Point** — Deterministic across all architectures

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
kromia-ledger = "0.1"
```

### Basic Usage

```rust
use kromia_ledger::{Ledger, AccountType, Balance};

fn main() {
    let mut ledger = Ledger::new();

    // Create accounts
    let cash = ledger.create_account("Cash", AccountType::Asset);
    let revenue = ledger.create_account("Revenue", AccountType::Revenue);

    // Record a transaction (amount in fixed-point: 150_00 = $150.00)
    ledger.record_transaction(
        "Invoice payment received",
        &[(cash, 150_00)],    // debits
        &[(revenue, 150_00)], // credits
    ).expect("Transaction must balance");

    // Verify chain integrity
    assert!(ledger.verify_chain());
}
```

## Architecture

```
src/
├── lib.rs          — Public API & module re-exports
├── types.rs        — Core types: Balance, AccountType, LedgerEntry, Transaction
├── validation.rs   — Balance validation & integrity checks
├── chain.rs        — SHA-256 hashing & cryptographic chaining
├── reconcile.rs    — Reconciliation / matching engine
└── wasm.rs         — wasm-bindgen interface layer
```

## Building

### Native

```bash
cargo build --release
```

### WebAssembly

```bash
wasm-pack build --target web
```

## Testing

```bash
cargo test
```

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Author

**M Reyvan Purnama** — [GitHub](https://github.com/reyvanevan)
