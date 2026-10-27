//! Encoding and decoding of single Markdown cell values (`docs/markdown-format.md` → "Value encoding").
//! Time zones are always parameters; nothing here reads the system clock or zone.

use std::str::FromStr;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use rust_decimal::Decimal;

/// R-MD-1: Swift `Decimal.description`: no trailing zeros and never an exponent (`12.5`, `-1200`, `0`).
pub fn fmt_decimal(d: Decimal) -> String {
    d.normalize().to_string()
}

/// R-MD-2: strict decimal parse after trimming. `""` and anything non-numeric give `None`.
pub fn parse_decimal(s: &str) -> Option<Decimal> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    Decimal::from_str(t).ok()
}

/// R-MD-2: Swift `NSString.boolValue`. After trimming, true iff the first character is `t`, `T`,
/// `y`, `Y` or a digit 1-9, or an optional sign is followed by zeros and then a digit 1-9.
pub fn swift_bool(s: &str) -> bool {
    let t = s.trim();
    if t.starts_with([
        't', 'T', 'y', 'Y', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    ]) {
        return true;
    }
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(t);
    unsigned
        .trim_start_matches('0')
        .starts_with(['1', '2', '3', '4', '5', '6', '7', '8', '9'])
}

/// R-MD-1: a calendar date as local midnight in `tz`, converted to UTC, e.g. `2026-02-01T05:00:00Z`.
pub fn fmt_date(d: Date, tz: &TimeZone) -> String {
    match d.to_zoned(tz.clone()) {
        Ok(zoned) => fmt_timestamp(zoned.timestamp()),
        // Only dates outside jiff's range get here. The civil form still parses back.
        Err(_) => d.to_string(),
    }
}

/// R-MD-1: ISO-8601 in UTC with `Z`, whole seconds only (Swift's formatter never writes fractions).
pub fn fmt_timestamp(t: Timestamp) -> String {
    t.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// R-MD-2: an ISO timestamp becomes its local day in `tz`; otherwise `YYYY-MM-DD`. Empty gives `None`.
pub fn parse_date(s: &str, tz: &TimeZone) -> Option<Date> {
    if s.is_empty() {
        return None;
    }
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ts.to_zoned(tz.clone()).date());
    }
    s.parse::<Date>().ok()
}

/// R-MD-2: an ISO timestamp as is; `YYYY-MM-DD` as local midnight in `tz`. Empty gives `None`.
pub fn parse_timestamp(s: &str, tz: &TimeZone) -> Option<Timestamp> {
    if s.is_empty() {
        return None;
    }
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ts);
    }
    let date = s.parse::<Date>().ok()?;
    date.to_zoned(tz.clone()).ok().map(|z| z.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn ny() -> TimeZone {
        TimeZone::get("America/New_York").expect("tzdb")
    }
    fn chicago() -> TimeZone {
        TimeZone::get("America/Chicago").expect("tzdb")
    }
    fn tokyo() -> TimeZone {
        TimeZone::get("Asia/Tokyo").expect("tzdb")
    }
    fn date(s: &str) -> Date {
        s.parse().expect("civil date")
    }

    /// Covers: R-MD-1
    #[test]
    fn fmt_decimal_matches_swift_description() {
        assert_eq!(fmt_decimal(dec!(12.50)), "12.5");
        assert_eq!(fmt_decimal(dec!(-1200.00)), "-1200");
        assert_eq!(fmt_decimal(dec!(0.00)), "0");
        assert_eq!(fmt_decimal(dec!(219.10)), "219.1");
        assert_eq!(fmt_decimal(dec!(-0.5)), "-0.5");
    }

    /// Covers: R-MD-2
    #[test]
    fn parse_decimal_strict() {
        assert_eq!(parse_decimal("12.50"), Some(dec!(12.50)));
        assert_eq!(parse_decimal("-5"), Some(dec!(-5)));
        assert_eq!(parse_decimal("+5"), Some(dec!(5)));
        assert_eq!(parse_decimal(" 7.25 "), Some(dec!(7.25)));
        assert_eq!(parse_decimal(""), None);
        assert_eq!(parse_decimal("Rent"), None);
        assert_eq!(parse_decimal("1,000"), None);
    }

    /// Covers: R-MD-2
    #[test]
    fn swift_bool_matches_nsstring() {
        for s in [
            "true", "TRUE", "YES", "yes", "y", "Y", "T", "1", "9", " 1 ", "+5", "-5", "01", "007",
            "5abc",
        ] {
            assert!(swift_bool(s), "expected true for {s:?}");
        }
        for s in [
            "false", "", "0", "00", "+0", "no", "n", "N", "F", "f", "-", "+", "x", "-true", " ",
        ] {
            assert!(!swift_bool(s), "expected false for {s:?}");
        }
    }

    /// Covers: R-MD-1
    #[test]
    fn fmt_date_is_local_midnight_utc() {
        assert_eq!(fmt_date(date("2026-02-01"), &ny()), "2026-02-01T05:00:00Z");
        assert_eq!(
            fmt_date(date("2026-02-01"), &chicago()),
            "2026-02-01T06:00:00Z"
        );
        assert_eq!(
            fmt_date(date("2026-02-01"), &tokyo()),
            "2026-01-31T15:00:00Z"
        );
        // New York is on daylight time in July (UTC-4).
        assert_eq!(fmt_date(date("2026-07-01"), &ny()), "2026-07-01T04:00:00Z");
    }

    /// Covers: R-MD-2
    #[test]
    fn parse_date_converts_to_local_day() {
        let ts = "2026-03-13T04:59:06Z";
        assert_eq!(parse_date(ts, &chicago()), Some(date("2026-03-12")));
        assert_eq!(parse_date(ts, &tokyo()), Some(date("2026-03-13")));
        for tz in [chicago(), tokyo(), ny()] {
            assert_eq!(parse_date("2025-01-25", &tz), Some(date("2025-01-25")));
        }
        assert_eq!(parse_date("", &chicago()), None);
        assert_eq!(parse_date("garbage", &chicago()), None);
    }

    /// Covers: R-MD-1
    #[test]
    fn fmt_timestamp_drops_fraction() {
        let t: Timestamp = "2026-02-01T05:00:00.123Z".parse().expect("ts");
        assert_eq!(fmt_timestamp(t), "2026-02-01T05:00:00Z");
    }

    /// Covers: R-MD-2
    #[test]
    fn parse_timestamp_accepts_iso_and_date_only() {
        let iso = parse_timestamp("2026-02-01T05:00:00Z", &chicago());
        assert_eq!(iso, Some("2026-02-01T05:00:00Z".parse().expect("ts")));
        let midnight = parse_timestamp("2026-02-01", &chicago());
        assert_eq!(midnight, Some("2026-02-01T06:00:00Z".parse().expect("ts")));
        assert_eq!(parse_timestamp("", &chicago()), None);
        assert_eq!(parse_timestamp("nope", &chicago()), None);
    }
}
