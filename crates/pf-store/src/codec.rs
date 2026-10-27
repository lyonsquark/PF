//! Encoding helpers between Rust values and SQLite TEXT/INTEGER columns.
//! A value that cannot be decoded becomes `StoreError::Corrupt` naming its table and column.

use std::str::FromStr;

use jiff::Timestamp;
use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::store::StoreError;

/// Builds the `Corrupt` error for a value that could not be decoded.
pub(crate) fn corrupt(table: &'static str, column: &'static str, value: &str) -> StoreError {
    StoreError::Corrupt {
        table,
        column,
        value: value.to_string(),
    }
}

/// Decodes a money amount. The scale is kept, so "-12.50" stays "-12.50".
pub(crate) fn decimal(
    value: &str,
    table: &'static str,
    column: &'static str,
) -> Result<Decimal, StoreError> {
    Decimal::from_str(value).map_err(|_| corrupt(table, column, value))
}

/// Decodes a calendar date (`YYYY-MM-DD`).
pub(crate) fn date(
    value: &str,
    table: &'static str,
    column: &'static str,
) -> Result<Date, StoreError> {
    value
        .parse::<Date>()
        .map_err(|_| corrupt(table, column, value))
}

/// Decodes an RFC 3339 instant.
pub(crate) fn timestamp(
    value: &str,
    table: &'static str,
    column: &'static str,
) -> Result<Timestamp, StoreError> {
    value
        .parse::<Timestamp>()
        .map_err(|_| corrupt(table, column, value))
}

/// Decodes an integer stored as text (meta and settings numbers).
pub(crate) fn int(
    value: &str,
    table: &'static str,
    column: &'static str,
) -> Result<i64, StoreError> {
    value
        .trim()
        .parse::<i64>()
        .map_err(|_| corrupt(table, column, value))
}

/// Decodes a JSON array of strings (`categories.match_patterns`).
pub(crate) fn json_strings(
    value: &str,
    table: &'static str,
    column: &'static str,
) -> Result<Vec<String>, StoreError> {
    serde_json::from_str::<Vec<String>>(value).map_err(|_| corrupt(table, column, value))
}

/// Decodes a JSON array of month numbers (`budget_items.specific_months`).
pub(crate) fn json_u32s(
    value: &str,
    table: &'static str,
    column: &'static str,
) -> Result<Vec<u32>, StoreError> {
    serde_json::from_str::<Vec<u32>>(value).map_err(|_| corrupt(table, column, value))
}

/// Encodes a list of strings as a JSON array. Serializing strings cannot fail,
/// so the fallback branch is unreachable in practice.
pub(crate) fn strings_json(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

/// Encodes a list of month numbers as a JSON array. Serializing integers cannot fail,
/// so the fallback branch is unreachable in practice.
pub(crate) fn u32s_json(items: &[u32]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

/// Decodes an INTEGER 0/1 flag. Any non-zero value is true.
pub(crate) fn flag(value: i64) -> bool {
    value != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{d, m};

    #[test]
    fn codec_decimal_keeps_scale() {
        let text = m("-12.50").to_string();
        assert_eq!(text, "-12.50");
        let back = decimal(&text, "transactions", "amount").expect("decodes");
        assert_eq!(back.to_string(), "-12.50");
        assert_eq!(back, m("-12.50"));
    }

    #[test]
    fn codec_date_and_timestamp_round_trip() {
        let day = d("2026-01-05");
        assert_eq!(day.to_string(), "2026-01-05");
        assert_eq!(
            date(&day.to_string(), "transactions", "date").expect("date"),
            day
        );

        let ts: Timestamp = "2026-02-01T05:00:00.5Z".parse().expect("timestamp");
        let text = ts.to_string();
        assert_eq!(
            timestamp(&text, "obligations", "created_date").expect("ts"),
            ts
        );
    }

    #[test]
    fn codec_corrupt_decimal_errors() {
        match decimal("abc", "transactions", "amount") {
            Err(StoreError::Corrupt {
                table,
                column,
                value,
            }) => {
                assert_eq!(table, "transactions");
                assert_eq!(column, "amount");
                assert_eq!(value, "abc");
            }
            other => panic!("expected Corrupt, got {other:?}"),
        }
    }

    #[test]
    fn json_lists_round_trip_and_reject_garbage() {
        let text = strings_json(&["a".to_string(), "b c".to_string()]);
        let back = json_strings(&text, "categories", "match_patterns").expect("list");
        assert_eq!(back, vec!["a", "b c"]);
        let months = u32s_json(&[1, 6, 12]);
        assert_eq!(
            json_u32s(&months, "budget_items", "specific_months").expect("months"),
            vec![1, 6, 12]
        );
        assert!(json_u32s("not json", "budget_items", "specific_months").is_err());
    }
}
