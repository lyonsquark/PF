//! L1 tests for statement reconciliation: rules R-REC-1 … R-REC-13. Fixtures in `fixtures/statements/`.

mod check;
mod golden;
mod matching;
mod payee;
mod report;
mod statement;

use pf_model::testing::{d, m};

use super::{Statement, StatementLine};

/// Reads a statement fixture from the repo-level `fixtures/statements/` directory.
pub fn fixture(name: &str) -> String {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/statements/");
    std::fs::read_to_string(format!("{base}{name}")).expect("fixture exists")
}

/// A line with no section or reference.
pub fn line(date: &str, desc: &str, amount: &str) -> StatementLine {
    StatementLine {
        date: d(date),
        post_date: None,
        description: desc.to_string(),
        amount: m(amount),
        section: String::new(),
        reference: String::new(),
    }
}

/// A statement for `start`..`end` with the given balances and lines.
pub fn statement(
    start: &str,
    end: &str,
    opening: &str,
    closing: &str,
    lines: Vec<StatementLine>,
) -> Statement {
    Statement {
        institution: String::new(),
        account_hint: String::new(),
        period_start: d(start),
        period_end: d(end),
        opening_balance: m(opening),
        closing_balance: m(closing),
        total_credits: None,
        total_debits: None,
        lines,
    }
}
