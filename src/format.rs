//! Human-readable balance formatting and parsing.
//!
//! Converts fixed-point [`Balance`] values (i128) to formatted strings and back.
//! Supports any decimal precision (USD=2, IDR=0, BTC=8, etc.).
//!
//! - **Precision-aware** (recommended): [`format_amount`], [`parse_amount`]
//! - **Convenience (precision=2)**: [`format_balance`], [`parse_balance`]
//!
//! # Example
//!
//! ```
//! use kromia_ledger::format::{format_amount, parse_amount, format_balance};
//!
//! // USD (precision=2): 100.00 stored as 10_000
//! assert_eq!(format_amount(10_000, 2), "100.00");
//!
//! // IDR (precision=0): 1,000,000 stored as 1_000_000
//! assert_eq!(format_amount(1_000_000, 0), "1,000,000");
//!
//! // BTC (precision=8): 1.00000001 stored as 100_000_001
//! assert_eq!(format_amount(100_000_001, 8), "1.00000001");
//!
//! // Backward-compatible (always precision=2)
//! assert_eq!(format_balance(1_234_56), "1,234.56");
//! ```

use crate::types::Balance;

// ── Precision-aware formatting ──────────────────────────────────────

/// Format a fixed-point balance for any currency precision.
///
/// - `precision = 0` → no decimal point (IDR, JPY)
/// - `precision = 2` → 2 decimal places (USD, EUR)
/// - `precision = 8` → 8 decimal places (BTC)
///
/// Output always includes thousands separators.
///
/// # Examples
///
/// ```
/// use kromia_ledger::format::format_amount;
///
/// assert_eq!(format_amount(0, 2), "0.00");
/// assert_eq!(format_amount(150_00, 2), "150.00");
/// assert_eq!(format_amount(1_000_000, 0), "1,000,000");
/// assert_eq!(format_amount(-42_50, 2), "-42.50");
/// assert_eq!(format_amount(100_000_001, 8), "1.00000001");
/// ```
pub fn format_amount(amount: Balance, precision: u8) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    let divisor = 10u128.pow(precision as u32);
    let whole = abs / divisor;
    let frac = abs % divisor;

    let formatted_whole = format_with_thousands(whole);

    if precision == 0 {
        format!("{sign}{formatted_whole}")
    } else {
        format!("{sign}{formatted_whole}.{frac:0>width$}", width = precision as usize)
    }
}

/// Format a balance with a currency symbol prefix and precision.
///
/// # Examples
///
/// ```
/// use kromia_ledger::format::format_amount_with_currency;
///
/// assert_eq!(format_amount_with_currency(250_00, "$", 2), "$250.00");
/// assert_eq!(format_amount_with_currency(1_500_000, "Rp", 0), "Rp1,500,000");
/// ```
pub fn format_amount_with_currency(amount: Balance, symbol: &str, precision: u8) -> String {
    format!("{symbol}{}", format_amount(amount, precision))
}

// ── Precision-aware parsing ─────────────────────────────────────────

/// Parse a human-readable string into a fixed-point [`Balance`] for any precision.
///
/// Accepts: `"1234.56"`, `"1,234.56"`, `"-42.50"`, `"100"`, `"1.00000001"`.
/// Commas are stripped before parsing.
///
/// # Errors
///
/// Returns `Err(String)` if the input is empty, has non-numeric characters,
/// has more than one decimal point, or has more fractional digits than `precision`.
///
/// # Examples
///
/// ```
/// use kromia_ledger::format::parse_amount;
///
/// assert_eq!(parse_amount("100", 2).unwrap(), 10_000);
/// assert_eq!(parse_amount("1,234.56", 2).unwrap(), 1_234_56);
/// assert_eq!(parse_amount("1,000,000", 0).unwrap(), 1_000_000);
/// assert_eq!(parse_amount("1.00000001", 8).unwrap(), 100_000_001);
/// ```
pub fn parse_amount(s: &str, precision: u8) -> Result<Balance, String> {
    let s = s.trim().replace(',', "");
    let negative = s.starts_with('-');
    let s = s.trim_start_matches(['-', '+']);
    if s.is_empty() {
        return Err("empty input".to_string());
    }

    let divisor = 10u128.pow(precision as u32);
    let parts: Vec<&str> = s.split('.').collect();

    match parts.len() {
        1 => {
            let whole: u128 = parts[0].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            let val = (whole * divisor) as Balance;
            Ok(if negative { -val } else { val })
        }
        2 => {
            if precision == 0 {
                return Err("decimal point not allowed for precision=0".to_string());
            }
            let whole: u128 = parts[0].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            let frac_str = parts[1];
            if frac_str.len() > precision as usize {
                return Err(format!(
                    "too many fractional digits: got {}, max {precision}",
                    frac_str.len()
                ));
            }
            // Right-pad with zeros to match precision
            let padded = format!("{frac_str:0<width$}", width = precision as usize);
            let frac: u128 = padded.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            let val = (whole * divisor + frac) as Balance;
            Ok(if negative { -val } else { val })
        }
        _ => Err("invalid format: multiple decimal points".to_string()),
    }
}

// ── Backward-compatible (precision = 2) ─────────────────────────────

/// Format a fixed-point balance with 2 decimal places (USD convention).
///
/// This is a convenience wrapper around [`format_amount`] with `precision = 2`.
///
/// # Examples
///
/// ```
/// use kromia_ledger::format_balance;
///
/// assert_eq!(format_balance(0), "0.00");
/// assert_eq!(format_balance(150_00), "150.00");
/// assert_eq!(format_balance(1_234_567_89), "1,234,567.89");
/// assert_eq!(format_balance(-42_50), "-42.50");
/// ```
pub fn format_balance(amount: Balance) -> String {
    format_amount(amount, 2)
}

/// Format a balance with a currency symbol (precision = 2).
///
/// This is a convenience wrapper around [`format_amount_with_currency`] with `precision = 2`.
///
/// # Examples
///
/// ```
/// use kromia_ledger::format_balance_with_currency;
///
/// assert_eq!(format_balance_with_currency(250_00, "$"), "$250.00");
/// assert_eq!(format_balance_with_currency(1_500_000_00, "Rp"), "Rp1,500,000.00");
/// ```
pub fn format_balance_with_currency(amount: Balance, symbol: &str) -> String {
    format_amount_with_currency(amount, symbol, 2)
}

/// Parse a human-readable balance string (precision = 2).
///
/// This is a convenience wrapper around [`parse_amount`] with `precision = 2`.
///
/// # Examples
///
/// ```
/// use kromia_ledger::parse_balance;
///
/// assert_eq!(parse_balance("100").unwrap(), 100_00);
/// assert_eq!(parse_balance("1,234.56").unwrap(), 1_234_56);
/// assert_eq!(parse_balance("-42.50").unwrap(), -42_50);
/// assert!(parse_balance("abc").is_err());
/// ```
pub fn parse_balance(s: &str) -> Result<Balance, String> {
    parse_amount(s, 2)
}

// ── Internal helpers ────────────────────────────────────────────────

/// Insert thousands separators into a whole-number part.
fn format_with_thousands(n: u128) -> String {
    let digits = n.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_amount (precision-aware) ─────────────────────────────

    #[test]
    fn format_usd_precision_2() {
        assert_eq!(format_amount(0, 2), "0.00");
        assert_eq!(format_amount(1, 2), "0.01");
        assert_eq!(format_amount(150_00, 2), "150.00");
        assert_eq!(format_amount(-42_50, 2), "-42.50");
        assert_eq!(format_amount(1_234_567_89, 2), "1,234,567.89");
        assert_eq!(format_amount(100_000_00, 2), "100,000.00");
    }

    #[test]
    fn format_idr_precision_0() {
        assert_eq!(format_amount(0, 0), "0");
        assert_eq!(format_amount(1_000_000, 0), "1,000,000");
        assert_eq!(format_amount(500, 0), "500");
        assert_eq!(format_amount(-78_500, 0), "-78,500");
    }

    #[test]
    fn format_btc_precision_8() {
        assert_eq!(format_amount(100_000_000, 8), "1.00000000");
        assert_eq!(format_amount(100_000_001, 8), "1.00000001");
        assert_eq!(format_amount(50_000, 8), "0.00050000");
        assert_eq!(format_amount(21_000_000_00_000_000, 8), "21,000,000.00000000");
    }

    #[test]
    fn format_amount_with_symbol() {
        assert_eq!(format_amount_with_currency(250_00, "$", 2), "$250.00");
        assert_eq!(format_amount_with_currency(1_500_000, "Rp", 0), "Rp1,500,000");
        assert_eq!(format_amount_with_currency(100_000_000, "₿", 8), "₿1.00000000");
    }

    // ── parse_amount (precision-aware) ──────────────────────────────

    #[test]
    fn parse_usd_precision_2() {
        assert_eq!(parse_amount("100", 2).unwrap(), 10_000);
        assert_eq!(parse_amount("1,234.56", 2).unwrap(), 1_234_56);
        assert_eq!(parse_amount("-42.50", 2).unwrap(), -42_50);
        assert_eq!(parse_amount("0.01", 2).unwrap(), 1);
        assert_eq!(parse_amount("42.5", 2).unwrap(), 42_50);
    }

    #[test]
    fn parse_idr_precision_0() {
        assert_eq!(parse_amount("1,000,000", 0).unwrap(), 1_000_000);
        assert_eq!(parse_amount("500", 0).unwrap(), 500);
        assert!(parse_amount("100.50", 0).is_err()); // no decimals allowed
    }

    #[test]
    fn parse_btc_precision_8() {
        assert_eq!(parse_amount("1.00000001", 8).unwrap(), 100_000_001);
        assert_eq!(parse_amount("0.0005", 8).unwrap(), 50_000);
        assert_eq!(parse_amount("21000000", 8).unwrap(), 21_000_000_00_000_000);
    }

    #[test]
    fn parse_too_many_decimals_rejected() {
        assert!(parse_amount("1.123", 2).is_err()); // 3 digits > precision 2
        assert!(parse_amount("1.123456789", 8).is_err()); // 9 digits > precision 8
    }

    #[test]
    fn parse_invalid_inputs() {
        assert!(parse_amount("", 2).is_err());
        assert!(parse_amount("abc", 2).is_err());
        assert!(parse_amount("1.2.3", 2).is_err());
    }

    // ── Roundtrip tests ─────────────────────────────────────────────

    #[test]
    fn roundtrip_usd() {
        for &v in &[0i128, 1, 99, 100_00, -42_50, 1_234_567_89] {
            assert_eq!(parse_amount(&format_amount(v, 2), 2).unwrap(), v);
        }
    }

    #[test]
    fn roundtrip_idr() {
        for &v in &[0i128, 500, 1_000_000, -78_500] {
            assert_eq!(parse_amount(&format_amount(v, 0), 0).unwrap(), v);
        }
    }

    #[test]
    fn roundtrip_btc() {
        for &v in &[0i128, 1, 100_000_000, 100_000_001, 50_000] {
            assert_eq!(parse_amount(&format_amount(v, 8), 8).unwrap(), v);
        }
    }

    // ── Backward compat (precision=2 wrappers) ──────────────────────

    #[test]
    fn format_balance_compat() {
        assert_eq!(format_balance(0), "0.00");
        assert_eq!(format_balance(150_00), "150.00");
        assert_eq!(format_balance(1), "0.01");
        assert_eq!(format_balance(-42_50), "-42.50");
        assert_eq!(format_balance(1_234_567_89), "1,234,567.89");
    }

    #[test]
    fn format_currency_compat() {
        assert_eq!(format_balance_with_currency(250_00, "$"), "$250.00");
        assert_eq!(format_balance_with_currency(1_500_000_00, "Rp"), "Rp1,500,000.00");
    }

    #[test]
    fn parse_balance_compat() {
        assert_eq!(parse_balance("100").unwrap(), 100_00);
        assert_eq!(parse_balance("1,234.56").unwrap(), 1_234_56);
        assert_eq!(parse_balance("-42.50").unwrap(), -42_50);
    }
}
