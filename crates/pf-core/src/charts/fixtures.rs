//! Test helpers: `fixtures/small.md` imported with the `America/Chicago` zone, and the
//! standard chart filter (Checking and Rewards Card, Reality, today 2026-03-15).

use std::collections::BTreeSet;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use pf_model::testing::d;
use pf_model::{AccountId, Db};

use super::ChartFilter;
use crate::markdown::import_markdown;

/// The fixed "today" of the chart tests.
pub(crate) fn today() -> Date {
    d("2026-03-15")
}

/// Imports `fixtures/small.md` (America/Chicago) through the T05 importer.
pub(crate) fn small_db() -> Db {
    let path = format!("{}/../../fixtures/small.md", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("fixture exists");
    let tz = TimeZone::get("America/Chicago").expect("tzdb");
    let now: Timestamp = "2026-03-15T12:00:00Z".parse().expect("timestamp");
    import_markdown(&text, &tz, now, &mut |_| {}).db
}

/// Id of the live account named `name` in `db`.
pub(crate) fn account_id(db: &Db, name: &str) -> AccountId {
    db.account_by_name(name).expect("account exists").id
}

/// The standard filter on `small.md`: Checking and Rewards Card, Reality, today 2026-03-15.
pub(crate) fn small_filter(db: &Db) -> ChartFilter {
    ChartFilter {
        accounts: BTreeSet::from([account_id(db, "Checking"), account_id(db, "Rewards Card")]),
        scenario: None,
        today: today(),
    }
}

/// Payees of a row list, in order.
pub(crate) fn payees<'a>(rows: &[&'a pf_model::Transaction]) -> Vec<&'a str> {
    rows.iter().map(|tx| tx.payee.as_str()).collect()
}
