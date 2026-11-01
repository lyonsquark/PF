//! Report numbers as pure functions over `Db`: P&L (R-RPT-1..3), drill-downs (R-RPT-3b/3c),
//! balance sheet (R-RPT-4..4d) and monthly matrix (R-RPT-5..5f). The Reports UI only renders them.

mod balance_sheet;
mod drill;
mod matrix;
mod pnl;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod golden;

pub use balance_sheet::{BalanceSheet, BsAccountRow, BsObligationRow, balance_sheet};
pub use drill::{
    BudgetDrillRow, DrillRow, budget_drill_rows, budget_drill_title, drill_rows, pnl_drill_title,
};
pub use matrix::{
    MONTH_SHORT, MatrixCell, MatrixMode, MatrixRow, MonthlyMatrix, default_matrix_cutoff,
    matrix_cell, matrix_drill, matrix_drill_through, matrix_drill_title, matrix_year_options,
    monthly_matrix,
};
pub use pnl::{PlRow, profit_and_loss};

use jiff::civil::Date;
use pf_model::{
    BudgetItem, BudgetItemType, Category, CategoryId, CategoryType, Db, ScenarioId, TimePeriod,
    Transaction,
};
use rust_decimal::Decimal;

use crate::balance::is_visible;
use crate::filter::period_start;
use crate::projector::generate_dates;

/// Colour class used by the UI: >0 Positive (green), <0 Negative (red), 0 Zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Greater than zero.
    Positive,
    /// Less than zero.
    Negative,
    /// Exactly zero.
    Zero,
}

/// R-RPT-3a: colour class of an amount.
pub fn tone(d: Decimal) -> Tone {
    if d > Decimal::ZERO {
        Tone::Positive
    } else if d < Decimal::ZERO {
        Tone::Negative
    } else {
        Tone::Zero
    }
}

/// R-RPT-0: the live category with this id. A soft-deleted or unknown category gives None.
pub fn live_category(db: &Db, id: Option<CategoryId>) -> Option<&Category> {
    db.category(id?).filter(|c| !c.is_deleted)
}

/// R-RPT-0: the report bucket of a row: the live category's id, or None (uncategorized).
fn bucket(db: &Db, id: Option<CategoryId>) -> Option<CategoryId> {
    live_category(db, id).map(|c| c.id)
}

/// R-RPT-0: true when the live category is of type Transfer.
pub fn is_transfer(db: &Db, id: Option<CategoryId>) -> bool {
    live_category(db, id).is_some_and(|c| c.category_type == CategoryType::Transfer)
}

/// R-RPT-1: a budget magnitude with the sign of its type: Income positive, others negative.
/// A zero stays a plain zero (no negative zero).
fn signed(t: BudgetItemType, magnitude: Decimal) -> Decimal {
    let m = magnitude.abs();
    if m.is_zero() {
        Decimal::ZERO
    } else if t == BudgetItemType::Income {
        m
    } else {
        -m
    }
}

/// R-RPT-1: signed `|amount × count|` for `count` occurrences of the item.
fn signed_total(item: &BudgetItem, count: usize) -> Decimal {
    signed(item.item_type, item.amount.abs() * Decimal::from(count))
}

/// R-RPT-1 / R-RPT-3c: signed `|amount × n|`, where n is the number of occurrences of `item`
/// in [start, end] (from `generate_dates`).
pub fn budget_total_in_window(item: &BudgetItem, start: Date, end: Date) -> Decimal {
    signed_total(item, generate_dates(item, start, end).len())
}

/// R-RPT-2 / R-RPT-5a: live transactions that count toward a report over [from, to] (both
/// inclusive) under `scenario`. Excluded: external accounts (orphans count), rows not visible
/// under the scenario (R-SCN-1), and rows whose live category is Transfer (R-RPT-0).
/// Sorted by (date, id) ascending.
pub(crate) fn counted_transactions(
    db: &Db,
    from: Date,
    to: Date,
    scenario: Option<ScenarioId>,
) -> Vec<&Transaction> {
    let mut v: Vec<&Transaction> = db
        .live_transactions()
        .filter(|tx| is_visible(tx, scenario))
        .filter(|tx| from <= tx.date && tx.date <= to)
        .filter(|tx| {
            tx.account_id
                .and_then(|a| db.account(a))
                .is_none_or(|acc| !acc.is_external)
        })
        .filter(|tx| !is_transfer(db, tx.category_id))
        .collect();
    v.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    v
}

/// R-RPT-3d: default P&L range: one month before `today` through `today`.
pub fn default_pnl_range(today: Date) -> (Date, Date) {
    (
        period_start(TimePeriod::PastMonth, today).unwrap_or(today),
        today,
    )
}

/// R-RPT-6: range for a quick-select period. A period without a start (All, Archival) keeps
/// `current_start`. The end is always `today`.
pub fn quick_select_range(period: TimePeriod, today: Date, current_start: Date) -> (Date, Date) {
    (period_start(period, today).unwrap_or(current_start), today)
}

/// R-RPT-3d: quick-select buttons in display order.
pub const QUICK_SELECT: [(&str, TimePeriod); 4] = [
    ("1M", TimePeriod::PastMonth),
    ("60D", TimePeriod::Past60Days),
    ("YTD", TimePeriod::YearToDate),
    ("1Y", TimePeriod::PastYear),
];

/// The three reports, in picker order (R-RPT-3d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportKind {
    /// "Profit & Loss" (default).
    #[default]
    ProfitAndLoss,
    /// "Balance Sheet".
    BalanceSheet,
    /// "Monthly Matrix".
    MonthlyMatrix,
}

impl ReportKind {
    /// All report kinds in picker order.
    pub const ALL: [ReportKind; 3] = [
        ReportKind::ProfitAndLoss,
        ReportKind::BalanceSheet,
        ReportKind::MonthlyMatrix,
    ];

    /// The picker label.
    pub fn label(self) -> &'static str {
        match self {
            ReportKind::ProfitAndLoss => "Profit & Loss",
            ReportKind::BalanceSheet => "Balance Sheet",
            ReportKind::MonthlyMatrix => "Monthly Matrix",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::d;

    #[test]
    fn r_rpt_3a_tone() {
        // Covers: R-RPT-3a
        assert_eq!(tone(Decimal::ONE), Tone::Positive);
        assert_eq!(tone(-Decimal::ONE), Tone::Negative);
        assert_eq!(tone(Decimal::ZERO), Tone::Zero);
    }

    #[test]
    fn r_rpt_3d_default_and_quick_ranges() {
        // Covers: R-RPT-3d, R-RPT-6
        let today = d("2026-03-15");
        assert_eq!(default_pnl_range(today), (d("2026-02-15"), today));
        let current = d("2020-05-05");
        let ytd = quick_select_range(TimePeriod::YearToDate, today, current);
        assert_eq!(ytd, (d("2026-01-01"), today));
        let sixty = quick_select_range(TimePeriod::Past60Days, today, current);
        assert_eq!(sixty, (d("2026-01-14"), today));
        let all = quick_select_range(TimePeriod::All, today, current);
        assert_eq!(all, (current, today));
        let labels: Vec<&str> = QUICK_SELECT.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, ["1M", "60D", "YTD", "1Y"]);
        let periods: Vec<TimePeriod> = QUICK_SELECT.iter().map(|(_, p)| *p).collect();
        assert_eq!(
            periods,
            [
                TimePeriod::PastMonth,
                TimePeriod::Past60Days,
                TimePeriod::YearToDate,
                TimePeriod::PastYear
            ]
        );
        let kinds: Vec<&str> = ReportKind::ALL.iter().map(|k| k.label()).collect();
        assert_eq!(kinds, ["Profit & Loss", "Balance Sheet", "Monthly Matrix"]);
    }
}
