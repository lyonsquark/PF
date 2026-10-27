//! Pure business logic: balances, filters, projector, categorize, markdown, CSV, reports, charts. No IO, no clock.

pub mod balance;
pub mod categorize;
pub mod charts;
pub mod color;
pub mod csv_import;
pub mod filter;
pub mod format;
pub mod markdown;
pub mod names;
pub mod obligation;
pub mod projector;
pub mod reports;
