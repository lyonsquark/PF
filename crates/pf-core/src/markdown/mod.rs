//! Markdown backup import and export: the migration bridge with the Swift app.
//! `docs/markdown-format.md` is the spec; `MarkdownDataService.swift` is the behavioural truth.

pub mod escape;
mod export;
mod import;
pub mod values;

#[cfg(test)]
mod roundtrip;
#[cfg(test)]
mod tests;

pub use escape::{esc, unesc};
pub use export::export_markdown;
pub use import::import_markdown;

use pf_model::Db;

/// Result of parsing a backup. The Swift importer never fails; it skips bad rows.
#[derive(Debug, Clone)]
pub struct MarkdownImport {
    /// A brand-new Db holding only the imported rows.
    pub db: Db,
    /// Data rows that were dropped (too few columns, bad enum/number/date, duplicate name).
    pub skipped_rows: usize,
}

#[cfg(test)]
mod helpers {
    use jiff::Timestamp;
    use jiff::tz::TimeZone;

    use super::{MarkdownImport, import_markdown};

    /// Reads a file from the repo-level `fixtures/` directory.
    pub fn fixture(name: &str) -> String {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
        std::fs::read_to_string(format!("{p}{name}")).expect("fixture exists")
    }
    pub fn chicago() -> TimeZone {
        TimeZone::get("America/Chicago").expect("tzdb")
    }
    pub fn new_york() -> TimeZone {
        TimeZone::get("America/New_York").expect("tzdb")
    }
    pub fn tokyo() -> TimeZone {
        TimeZone::get("Asia/Tokyo").expect("tzdb")
    }
    pub fn now() -> Timestamp {
        "2026-03-15T12:00:00Z".parse().expect("ts")
    }
    pub fn import(text: &str, tz: &TimeZone) -> MarkdownImport {
        import_markdown(text, tz, now(), &mut |_| {})
    }
}
