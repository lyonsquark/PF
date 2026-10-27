//! R-CHT-2 and R-CHT-3: cumulative spending month against month, and budget pacing.

use jiff::Span;
use jiff::civil::Date;
use pf_model::{BudgetItemType, Db, Transaction};
use rust_decimal::Decimal;

use super::{
    ChartFilter, filtered_transactions, month_bounds, month_label_short, same_month, within,
};
use crate::projector::generate_dates;

/// A cumulative line: `(day of month, running Σ|amount|)`.
#[derive(Debug, Clone, PartialEq)]
pub struct CumulativeSeries {
    /// Legend label, "Mar 26", with " (A)" / " (B)" when both months are the same.
    pub label: String,
    /// One point per day, from day 1.
    pub points: Vec<(u8, Decimal)>,
}

/// R-CHT-2: both series and the totals shown in the header.
#[derive(Debug, Clone, PartialEq)]
pub struct SpendingComparison {
    /// Month A (solid line).
    pub a: CumulativeSeries,
    /// Month B (dashed line).
    pub b: CumulativeSeries,
    /// Total spent in month A, not truncated to today.
    pub total_a: Decimal,
    /// Total spent in month B.
    pub total_b: Decimal,
}

/// R-CHT-2 / R-CHT-2a / R-CHT-2b: cumulative spending for `month_a` and `month_b` (any date in
/// each month). Spending is F⁻ᵗ with `amount < 0`.
pub fn spending_comparison(
    db: &Db,
    f: &ChartFilter,
    month_a: Date,
    month_b: Date,
) -> SpendingComparison {
    let spending: Vec<&Transaction> = filtered_transactions(db, f, false)
        .into_iter()
        .filter(|tx| tx.amount < Decimal::ZERO)
        .collect();
    let (a_first, a_last) = month_bounds(month_a);
    let (b_first, b_last) = month_bounds(month_b);
    // R-CHT-2a: A is cut at today's day when A is the current month (Swift quirk); B never is
    let max_a = if same_month(month_a, f.today) {
        f.today.day()
    } else {
        a_last.day()
    };
    let max_b = b_last.day();
    // R-CHT-2b: the suffixes appear only when both sides show the same month
    let same = same_month(month_a, month_b);
    let suffix = |s: &str| {
        if same {
            format!("{s} (A)")
        } else {
            s.to_string()
        }
    };
    let suffix_b = |s: &str| {
        if same {
            format!("{s} (B)")
        } else {
            s.to_string()
        }
    };
    SpendingComparison {
        a: CumulativeSeries {
            label: suffix(&month_label_short(month_a)),
            points: cumulative(&spending, a_first, a_last, max_a),
        },
        b: CumulativeSeries {
            label: suffix_b(&month_label_short(month_b)),
            points: cumulative(&spending, b_first, b_last, max_b),
        },
        total_a: total(&spending, a_first, a_last),
        total_b: total(&spending, b_first, b_last),
    }
}

/// R-CHT-2a: running Σ|amount| per day 1..=max_day of the month `[first, last]`.
fn cumulative(
    spending: &[&Transaction],
    first: Date,
    last: Date,
    max_day: i8,
) -> Vec<(u8, Decimal)> {
    let month = within(spending, first, last);
    let mut running = Decimal::ZERO;
    (1..=max_day)
        .map(|day| {
            running += month
                .iter()
                .filter(|tx| tx.date.day() == day)
                .map(|tx| tx.amount.abs())
                .sum::<Decimal>();
            (day as u8, running)
        })
        .collect()
}

/// Σ|amount| of the spending rows in `[first, last]`.
fn total(spending: &[&Transaction], first: Date, last: Date) -> Decimal {
    within(spending, first, last)
        .iter()
        .map(|tx| tx.amount.abs())
        .sum()
}

/// The ◀ ▶ month steppers: moves by `delta` months and clamps the day (R-CHT-2).
pub fn step_month(d: Date, delta: i32) -> Date {
    d.checked_add(Span::new().months(i64::from(delta)))
        .unwrap_or(d)
}

/// R-CHT-3, R-CHT-3a, R-CHT-3b: budget pacing for the month of the base date.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetPacing {
    /// Total budget of the month (R-CHT-3). At most zero means no budget: the lines are empty.
    pub total_budget: Decimal,
    /// Budget line, one point per day of the month (dashed grey).
    pub budget_line: Vec<(u8, Decimal)>,
    /// Actual cumulative spending up to the base date's day (solid blue).
    pub actual_line: Vec<(u8, Decimal)>,
    /// The last actual point (0 when none).
    pub spent: Decimal,
    /// True when `spent` exceeds the total budget (header turns red).
    pub over_budget: bool,
}

/// R-CHT-3: budget pacing for the month of `base_date`. Budget items ignore the account
/// selection; the actual line respects it.
pub fn budget_pacing(db: &Db, f: &ChartFilter, base_date: Date) -> BudgetPacing {
    let (first, last) = month_bounds(base_date);
    // R-CHT-3: Σ |amount| × occurrences in the month, over live non-income items
    let total: Decimal = db
        .live_budget_items()
        .filter(|item| item.item_type != BudgetItemType::Income)
        .map(|item| {
            let count = generate_dates(item, first, last).len();
            item.amount.abs() * Decimal::from(count)
        })
        .sum();
    if total <= Decimal::ZERO {
        return BudgetPacing {
            total_budget: total,
            budget_line: Vec::new(),
            actual_line: Vec::new(),
            spent: Decimal::ZERO,
            over_budget: false,
        };
    }

    // R-CHT-3a: budget line, total / days × d
    let days = last.day();
    let daily = total / Decimal::from(days);
    let budget_line: Vec<(u8, Decimal)> = (1..=days)
        .map(|day| (day as u8, daily * Decimal::from(day)))
        .collect();

    // R-CHT-3a: actual line, cumulative spending for days 1..=min(base day, days in month)
    let spending: Vec<&Transaction> = filtered_transactions(db, f, false)
        .into_iter()
        .filter(|tx| tx.amount < Decimal::ZERO)
        .collect();
    let month = within(&spending, first, last);
    let mut running = Decimal::ZERO;
    let actual_line: Vec<(u8, Decimal)> = (1..=base_date.day().min(days))
        .map(|day| {
            running += month
                .iter()
                .filter(|tx| tx.date.day() == day)
                .map(|tx| tx.amount.abs())
                .sum::<Decimal>();
            (day as u8, running)
        })
        .collect();

    // R-CHT-3b
    let spent = actual_line.last().map_or(Decimal::ZERO, |p| p.1);
    BudgetPacing {
        total_budget: total,
        budget_line,
        actual_line,
        spent,
        over_budget: spent > total,
    }
}

#[cfg(test)]
mod tests {
    use pf_model::testing::{TestDb, d, m};
    use pf_model::{AccountType, BudgetFrequency, BudgetItemType, CategoryType};
    use rust_decimal_macros::dec;

    use super::*;
    use crate::charts::fixtures::{small_db, small_filter};

    #[test]
    /// Covers: R-CHT-2, R-CHT-2a
    fn r_cht_2a_cumulative_series() {
        let db = small_db();
        let f = small_filter(&db);
        let cmp = spending_comparison(&db, &f, d("2026-03-15"), d("2026-02-15"));

        // A: March is the current month, so it stops at today (15 points).
        assert_eq!(cmp.a.points.len(), 15);
        let a = |day: usize| cmp.a.points[day - 1].1;
        assert_eq!(a(1), m("120.45"));
        assert_eq!(a(2), m("262.75"));
        assert_eq!(a(10), m("712.75"));
        for day in 12..=15 {
            assert_eq!(a(day), m("719.50"));
        }
        // B: February is never truncated (28 points).
        assert_eq!(cmp.b.points.len(), 28);
        let b = |day: usize| cmp.b.points[day - 1].1;
        assert_eq!(b(3), m("64.99"));
        assert_eq!(b(14), m("152.99"));
        assert_eq!(b(17), m("241.09"));
        assert_eq!(b(28), m("1741.09"));
        assert_eq!(cmp.total_a, m("719.50"));
        assert_eq!(cmp.total_b, m("1741.09"));
    }

    #[test]
    /// Covers: R-CHT-2b
    fn r_cht_2b_labels_same_month_suffix() {
        let db = small_db();
        let f = small_filter(&db);
        let apart = spending_comparison(&db, &f, d("2026-03-15"), d("2026-02-15"));
        assert_eq!(apart.a.label, "Mar 26");
        assert_eq!(apart.b.label, "Feb 26");

        let same = spending_comparison(&db, &f, d("2026-03-01"), d("2026-03-20"));
        assert_eq!(same.a.label, "Mar 26 (A)");
        assert_eq!(same.b.label, "Mar 26 (B)");
        // B is the current month too, but still runs to the month's end.
        assert_eq!(same.b.points.len(), 31);
        assert_eq!(same.a.points.len(), 15);

        let b_current = spending_comparison(&db, &f, d("2026-02-10"), d("2026-03-10"));
        assert_eq!(b_current.b.points.len(), 31);
        assert_eq!(b_current.a.points.len(), 28);
    }

    #[test]
    /// Covers: R-CHT-3
    fn r_cht_3_budget_total_and_message() {
        let db = small_db();
        let f = small_filter(&db);
        let pacing = budget_pacing(&db, &f, d("2026-03-15"));
        assert_eq!(pacing.total_budget, m("2380"));

        // Only income items: no budget for the month.
        let mut t = TestDb::new();
        t.budget_item(
            BudgetItemType::Income,
            "Salary",
            "4000",
            "2026-01-25",
            BudgetFrequency::Monthly,
        )
        .done();
        let income_only = t.build();
        let f2 = ChartFilter {
            accounts: Default::default(),
            scenario: None,
            today: d("2026-03-15"),
        };
        let none = budget_pacing(&income_only, &f2, d("2026-03-15"));
        assert_eq!(none.total_budget, Decimal::ZERO);
        assert!(none.budget_line.is_empty() && none.actual_line.is_empty());
    }

    #[test]
    /// Covers: R-CHT-3a
    fn r_cht_3a_pacing_lines() {
        let db = small_db();
        let f = small_filter(&db);
        let pacing = budget_pacing(&db, &f, d("2026-03-15"));
        assert_eq!(pacing.budget_line.len(), 31);
        let at = |day: usize| pacing.budget_line[day - 1].1.round_dp(2);
        assert_eq!(at(15), m("1151.61"));
        assert_eq!(at(31), m("2380.00"));
        assert_eq!(pacing.actual_line.len(), 15);
        assert_eq!(pacing.actual_line.last().map(|p| p.1), Some(m("719.50")));
    }

    #[test]
    /// Covers: R-CHT-3b
    fn r_cht_3b_spent_and_over_budget() {
        let db = small_db();
        let f = small_filter(&db);
        let pacing = budget_pacing(&db, &f, d("2026-03-15"));
        assert_eq!(pacing.spent, m("719.50"));
        assert!(!pacing.over_budget);

        // Spending of 150 against a budget of 100 a month.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let food = t.category("Food", CategoryType::Expense);
        t.budget_item(
            BudgetItemType::Expense,
            "Food",
            "100",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .category(food)
        .done();
        t.tx(chk, "2026-03-05", "-150", "Market")
            .category(food)
            .done();
        let over = t.build();
        let f2 = ChartFilter {
            accounts: [super::super::fixtures::account_id(&over, "Checking")]
                .into_iter()
                .collect(),
            scenario: None,
            today: d("2026-03-15"),
        };
        let pacing = budget_pacing(&over, &f2, d("2026-03-15"));
        assert_eq!(pacing.total_budget, dec!(100));
        assert_eq!(pacing.spent, dec!(150));
        assert!(pacing.over_budget);
    }
}
