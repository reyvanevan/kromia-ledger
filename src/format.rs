//! Human-readable balance formatting and parsing.
//!
//! Converts fixed-point [`Balance`] values (i128) to formatted strings like
//! `"1,234.56"` and back. All functions assume 2-decimal precision.
//!
//! # Example
//!
//! ```
//! use kromia_ledger::format::{format_balance, parse_balance};
//!
//! assert_eq!(format_balance(1_234_56), "1,234.56");
//! assert_eq!(parse_balance("1,234.56").unwrap(), 1_234_56);
//! ```

use crate::types::Balance;

/// Format a fixed-point balance with thousands separators.
///
/// The input is in the smallest currency unit (cents for USD).
/// Output uses 2 decimal places with comma-separated thousands.
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
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    let whole = abs / 100;
    let frac = abs % 100;

    let whole_str = whole.to_string();
    let mut with_sep = String::with_capacity(whole_str.len() + whole_str.len() / 3);
    for (i, ch) in whole_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            with_sep.push(',');
        }
        with_sep.push(ch);
    }
    let formatted: String = with_sep.chars().rev().collect();
    format!("{sign}{formatted}.{frac:02}")
}

/// Format a balance with a currency symbol prefix.
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
    format!("{symbol}{}", format_balance(amount))
}

/// Parse a human-readable balance string into a fixed-point [`Balance`].
///
/// Accepts formats: `"1234.56"`, `"1,234.56"`, `"-42.50"`, `"100"`.
/// Commas are stripped before parsing. Negative values use a leading `-`.
///
/// # Errors
///
/// Returns `Err(String)` if the input is empty, contains non-numeric characters,
/// or has more than one decimal point.
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
    let s = s.trim().replace(',', "");
    let negative = s.starts_with('-');
    let s = s.trim_start_matches(['-', '+']);
    if s.is_empty() {
        return Err("empty input".to_string());
    }
    let parts: Vec<&str> = s.split('.').collect();
    match parts.len() {
        1 => {
            let whole: u128 = parts[0].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            let val = (whole * 100) as Balance;
            Ok(if negative { -val } else { val })
        }
        2 => {
            let whole: u128 = parts[0].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            let frac_normalized = match parts[1].len() {
                0 => "00".to_string(),
                1 => format!("{}0", parts[1]),
                _ => parts[1][..2].to_string(),
            };
            let frac: u128 = frac_normalized.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            let val = (whole * 100 + frac) as Balance;
            Ok(if negative { -val } else { val })
        }
        _ => Err("invalid format: multiple decimal points".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_basics() {
        assert_eq!(format_balance(0), "0.00");
        assert_eq!(format_balance(150_00), "150.00");
        assert_eq!(format_balance(1), "0.01");
        assert_eq!(format_balance(-42_50), "-42.50");
    }

    #[test]
    fn format_thousands() {
        assert_eq!(format_balance(1_234_567_89), "1,234,567.89");
        assert_eq!(format_balance(100_000_00), "100,000.00");
    }

    #[test]
    fn format_currency() {
        assert_eq!(format_balance_with_currency(250_00, "$"), "$250.00");
        assert_eq!(format_balance_with_currency(1_500_000_00, "Rp"), "Rp1,500,000.00");
    }

    #[test]
    fn parse_basics() {
        assert_eq!(parse_balance("100").unwrap(), 100_00);
        assert_eq!(parse_balance("150.00").unwrap(), 150_00);
        assert_eq!(parse_balance("42.5").unwrap(), 42_50);
        assert_eq!(parse_balance("0.01").unwrap(), 1);
        assert_eq!(parse_balance("-42.50").unwrap(), -42_50);
    }

    #[test]
    fn parse_with_commas() {
        assert_eq!(parse_balance("1,234.56").unwrap(), 1_234_56);
    }

    #[test]
    fn parse_roundtrip() {
        for &v in &[0i128, 1, 99, 100_00, -42_50, 1_234_567_89] {
            assert_eq!(parse_balance(&format_balance(v)).unwrap(), v);
        }
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_balance("").is_err());
        assert!(parse_balance("abc").is_err());
        assert!(parse_balance("1.2.3").is_err());
    }
}
