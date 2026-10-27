//! Money formatting and parsing. All arithmetic is `Decimal`; no `f64` anywhere.

use rust_decimal::{Decimal, RoundingStrategy};

/// Rounds to 2 dp with banker's rounding (NSNumberFormatter's default), returning the rounded
/// value and whether the sign should be shown.
fn round_cents(amount: Decimal) -> (Decimal, bool) {
    let rounded = amount.round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven);
    // The sign follows the unrounded value, so -0.001 prints "-$0.00" like Swift.
    let negative = amount.is_sign_negative() && !amount.is_zero();
    (rounded.abs(), negative)
}

/// Inserts ',' between groups of three digits of a non-negative integer string.
fn group_thousands(int_part: &str) -> String {
    let len = int_part.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// US currency like Swift NumberFormatter(.currency, "USD", en_US):
/// round to 2 dp with banker's rounding (MidpointNearestEven, NSNumberFormatter's default),
/// group thousands with ',', "$" prefix, negative = "-$…". Examples:
/// 1234.5 → "$1,234.50"; -1234.567 → "-$1,234.57"; 0 → "$0.00"; 0.125 → "$0.12"; 0.135 → "$0.14";
/// 1000000 → "$1,000,000.00"; -0.001 → "-$0.00" (Swift keeps the sign of the unrounded value; see pitfalls).
pub fn format_currency(amount: Decimal) -> String {
    let (abs, negative) = round_cents(amount);
    let text = abs.to_string();
    let (int_part, frac) = match text.split_once('.') {
        Some((i, f)) => (i.to_string(), format!("{f:0<2}")),
        None => (text.clone(), "00".to_string()),
    };
    let sign = if negative { "-" } else { "" };
    format!("{sign}${}.{frac}", group_thousands(&int_part))
}

/// Amount shown while editing (Swift AmountEditField.decimalToString): no grouping,
/// 0–2 fraction digits (trailing zeros dropped), banker's rounding: 12.50 → "12.5", 3 → "3", -0.125 → "-0.12".
pub fn format_amount_for_edit(amount: Decimal) -> String {
    let (abs, negative) = round_cents(amount);
    let text = abs.to_string();
    let trimmed = match text.split_once('.') {
        Some((i, f)) => {
            let f = f.trim_end_matches('0');
            if f.is_empty() {
                i.to_string()
            } else {
                format!("{i}.{f}")
            }
        }
        None => text,
    };
    if negative {
        format!("-{trimmed}")
    } else {
        trimmed
    }
}

/// Swift commitEdit: strip "$" and ",", trim, parse Decimal. Invalid → None (caller keeps the old value).
pub fn parse_amount_input(s: &str) -> Option<Decimal> {
    let cleaned: String = s.chars().filter(|c| *c != '$' && *c != ',').collect();
    cleaned.trim().parse::<Decimal>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    /// Currency examples from the doc comment of `format_currency`.
    fn format_currency_examples() {
        assert_eq!(format_currency(dec!(1234.5)), "$1,234.50");
        assert_eq!(format_currency(dec!(-1234.567)), "-$1,234.57");
        assert_eq!(format_currency(dec!(0)), "$0.00");
        assert_eq!(format_currency(dec!(0.125)), "$0.12");
        assert_eq!(format_currency(dec!(0.135)), "$0.14");
        assert_eq!(format_currency(dec!(1000000)), "$1,000,000.00");
        assert_eq!(format_currency(dec!(-0.001)), "-$0.00");
    }

    #[test]
    fn format_currency_bankers_rounding() {
        assert_eq!(format_currency(dec!(2.345)), "$2.34");
        assert_eq!(format_currency(dec!(2.355)), "$2.36");
    }

    #[test]
    fn format_amount_for_edit_examples() {
        assert_eq!(format_amount_for_edit(dec!(12.50)), "12.5");
        assert_eq!(format_amount_for_edit(dec!(3)), "3");
        assert_eq!(format_amount_for_edit(dec!(-0.125)), "-0.12");
        assert_eq!(format_amount_for_edit(dec!(1234567.891)), "1234567.89");
    }

    #[test]
    fn parse_amount_input_strips_symbols() {
        assert_eq!(parse_amount_input("$1,234.50"), Some(dec!(1234.50)));
        assert_eq!(parse_amount_input(" -3 "), Some(dec!(-3)));
        assert_eq!(parse_amount_input("abc"), None);
        assert_eq!(parse_amount_input(""), None);
    }
}
