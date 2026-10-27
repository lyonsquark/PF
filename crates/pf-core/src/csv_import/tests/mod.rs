//! L1 tests for CSV import: rules R-CSV-1 … R-CSV-16. Fixtures live in `fixtures/csv/`.

mod columns;
mod lookup_unmapped;
mod parse;
mod plan;
mod translate;

use jiff::tz::TimeZone;

use super::{ColumnMapping, ImportField, ParsedCsv, parse_csv};

/// Reads a CSV fixture from the repo-level `fixtures/csv/` directory.
pub fn fixture(name: &str) -> String {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/csv/");
    std::fs::read_to_string(format!("{base}{name}")).expect("fixture exists")
}

/// Parses a fixture file.
pub fn parsed(name: &str) -> ParsedCsv {
    parse_csv(&fixture(name))
}

/// The test time zone for fixture-style cases.
pub fn chicago() -> TimeZone {
    TimeZone::get("America/Chicago").expect("tzdb")
}

/// Owned strings from string literals.
pub fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// `(index, field)` pairs from `ColumnMapping::assignments` output.
pub fn pairs(assigned: &[(usize, String, ImportField)]) -> Vec<(usize, ImportField)> {
    assigned.iter().map(|(i, _, f)| (*i, *f)).collect()
}

/// A column mapping from `(index, field)` pairs, all other fields empty.
pub fn mapping_from(assignments: &[(usize, ImportField)]) -> ColumnMapping {
    let mut m = ColumnMapping::default();
    m.apply_assignments(assignments);
    m
}
