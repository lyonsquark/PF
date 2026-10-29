//! Statement reconciliation (R-REC-1 … R-REC-13): parse a statement, check it, and pair its lines with
//! an account's transactions. Pure; nothing is written here. New feature, no Swift source.

mod check;
mod matching;
mod payee;
mod report;
mod statement;

#[cfg(test)]
mod tests;

pub use check::{
    BalanceStyle, Reading, check_totals, line_amount, pf_balance, self_check, self_check_for,
    style_for_account,
};
pub use matching::{Confidence, Matching, Pair, candidates, run_matching};
pub use payee::{aliases_for, normalize_description, payee_score};
pub use report::{
    Choice, ReconcileInput, Report, Section, difference, reconcile, reconciled_balance,
};
pub use statement::{
    Statement, StatementError, StatementLine, parse_statement_csv, parse_statement_json,
};
