//! Monthly matrix: actual and budget per (category, month), cell display and drill-down
//! (R-RPT-5 .. R-RPT-5e).

use std::collections::BTreeMap;

use jiff::civil::Date;
use pf_model::{CategoryId, Db, ScenarioId, TransactionId};
use rust_decimal::Decimal;

use super::{bucket, counted_transactions, is_transfer, signed};
use crate::projector::generate_dates;

/// Month headers, English short names (R-RPT-5d).
pub const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// What the matrix shows in each cell (R-RPT-5d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatrixMode {
    /// "Actual + Budget" (default).
    #[default]
    All,
    /// "Actual Only".
    ActualOnly,
    /// "Budget Only".
    BudgetOnly,
}

impl MatrixMode {
    /// All modes in picker order.
    pub const ALL: [MatrixMode; 3] = [
        MatrixMode::All,
        MatrixMode::ActualOnly,
        MatrixMode::BudgetOnly,
    ];

    /// The picker label.
    pub fn label(self) -> &'static str {
        match self {
            MatrixMode::All => "Actual + Budget",
            MatrixMode::ActualOnly => "Actual Only",
            MatrixMode::BudgetOnly => "Budget Only",
        }
    }
}

/// One category row: twelve monthly actuals and budgets.
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixRow {
    /// Category full name, or "Uncategorized".
    pub label: String,
    /// None for Uncategorized.
    pub category_id: Option<CategoryId>,
    /// Actual per month (index 0 = January).
    pub actual: [Decimal; 12],
    /// Budget per month (index 0 = January).
    pub budget: [Decimal; 12],
}

impl MatrixRow {
    /// Σ of the twelve actual months.
    pub fn actual_total(&self) -> Decimal {
        self.actual.iter().sum()
    }

    /// Σ of the twelve budget months.
    pub fn budget_total(&self) -> Decimal {
        self.budget.iter().sum()
    }
}

/// The whole matrix for one year. NET is per-month sums over everything that passed the filters.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthlyMatrix {
    /// The year.
    pub year: i16,
    /// Category rows sorted by label.
    pub rows: Vec<MatrixRow>,
    /// NET actual per month.
    pub net_actual: [Decimal; 12],
    /// NET budget per month.
    pub net_budget: [Decimal; 12],
}

/// Jan 1 and Dec 31 of `year` (R-RPT-5a). None when the year is outside jiff's range.
fn year_bounds(year: i16) -> Option<(Date, Date)> {
    Some((Date::new(year, 1, 1).ok()?, Date::new(year, 12, 31).ok()?))
}

/// Zero-based month index of a date (0 = January).
fn month_index(date: Date) -> usize {
    usize::try_from(date.month() - 1).unwrap_or(0)
}

/// R-RPT-5a/5b/5c: the monthly matrix for `year` under `scenario`.
pub fn monthly_matrix(db: &Db, year: i16, scenario: Option<ScenarioId>) -> MonthlyMatrix {
    let zero = [Decimal::ZERO; 12];
    let Some((from, to)) = year_bounds(year) else {
        return MonthlyMatrix {
            year,
            rows: Vec::new(),
            net_actual: zero,
            net_budget: zero,
        };
    };

    let mut actual: BTreeMap<Option<CategoryId>, [Decimal; 12]> = BTreeMap::new();
    let mut budget: BTreeMap<Option<CategoryId>, [Decimal; 12]> = BTreeMap::new();
    let mut net_actual = zero;
    let mut net_budget = zero;

    // R-RPT-5a: actuals, per (category, month).
    for tx in counted_transactions(db, from, to, scenario) {
        let m = month_index(tx.date);
        actual.entry(bucket(db, tx.category_id)).or_insert(zero)[m] += tx.amount;
        net_actual[m] += tx.amount;
    }

    // R-RPT-5b: every occurrence of every live budget item adds its signed amount to its month, except
    // an item in a Transfer category.
    for item in db
        .live_budget_items()
        .filter(|item| !is_transfer(db, item.category_id))
    {
        let cat = bucket(db, item.category_id);
        for date in generate_dates(item, from, to) {
            let m = month_index(date);
            let value = signed(item.item_type, item.amount);
            budget.entry(cat).or_insert(zero)[m] += value;
            net_budget[m] += value;
        }
    }

    // R-RPT-5c: rows for live categories with any non-zero month (a Transfer category never has one, see
    // R-RPT-5a and R-RPT-5b), then Uncategorized, sorted by label.
    let mut rows: Vec<MatrixRow> = Vec::new();
    for cat in db.live_categories() {
        let a = actual.get(&Some(cat.id)).copied().unwrap_or(zero);
        let b = budget.get(&Some(cat.id)).copied().unwrap_or(zero);
        if has_value(&a) || has_value(&b) {
            rows.push(MatrixRow {
                label: db.category_full_name(cat.id),
                category_id: Some(cat.id),
                actual: a,
                budget: b,
            });
        }
    }
    let a = actual.get(&None).copied().unwrap_or(zero);
    let b = budget.get(&None).copied().unwrap_or(zero);
    if has_value(&a) || has_value(&b) {
        rows.push(MatrixRow {
            label: "Uncategorized".to_string(),
            category_id: None,
            actual: a,
            budget: b,
        });
    }
    rows.sort_by(|x, y| x.label.cmp(&y.label));

    MonthlyMatrix {
        year,
        rows,
        net_actual,
        net_budget,
    }
}

/// R-RPT-5f: the cutoff month (1..=12) a year starts with: this month in the current year, otherwise
/// December.
pub fn default_matrix_cutoff(year: i16, today: Date) -> u8 {
    if year == today.year() {
        u8::try_from(today.month()).unwrap_or(12)
    } else {
        12
    }
}

impl MonthlyMatrix {
    /// R-RPT-5f: the matrix as far as month `cutoff` (1..=12): later months are zero in every row and in
    /// NET, so the totals leave them out, and a row with nothing left is dropped.
    pub fn through(mut self, cutoff: u8) -> MonthlyMatrix {
        let shown = usize::from(cutoff).min(12);
        let clear = |months: &mut [Decimal; 12]| {
            for value in months.iter_mut().skip(shown) {
                *value = Decimal::ZERO;
            }
        };
        for row in &mut self.rows {
            clear(&mut row.actual);
            clear(&mut row.budget);
        }
        clear(&mut self.net_actual);
        clear(&mut self.net_budget);
        self.rows
            .retain(|row| has_value(&row.actual) || has_value(&row.budget));
        self
    }
}

/// True when any month is non-zero.
fn has_value(months: &[Decimal; 12]) -> bool {
    months.iter().any(|v| !v.is_zero())
}

/// The text of one matrix cell after the mode is applied (R-RPT-5d).
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixCell {
    /// "—": both actual and budget are zero (Actual + Budget mode only).
    Dash,
    /// A single amount.
    One(Decimal),
    /// Actual, budget, and their difference (Actual + Budget mode, budget non-zero).
    Three {
        /// Actual amount.
        actual: Decimal,
        /// Budget amount.
        budget: Decimal,
        /// actual − budget.
        diff: Decimal,
    },
}

/// R-RPT-5d: what one cell shows for `mode`.
pub fn matrix_cell(mode: MatrixMode, actual: Decimal, budget: Decimal) -> MatrixCell {
    match mode {
        MatrixMode::ActualOnly => MatrixCell::One(actual),
        MatrixMode::BudgetOnly => MatrixCell::One(budget),
        MatrixMode::All => {
            if actual.is_zero() && budget.is_zero() {
                MatrixCell::Dash
            } else if budget.is_zero() {
                MatrixCell::One(actual)
            } else {
                MatrixCell::Three {
                    actual,
                    budget,
                    diff: actual - budget,
                }
            }
        }
    }
}

/// R-RPT-5: year choices: current year − 2 ..= current year + 5.
pub fn matrix_year_options(current_year: i16) -> std::ops::RangeInclusive<i16> {
    (current_year - 2)..=(current_year + 5)
}

/// R-RPT-5e: ids of the transactions behind one cell. `category` None means Uncategorized;
/// `month` is 1..=12, or None for the whole year (the Total column). Same filters as R-RPT-5a.
pub fn matrix_drill(
    db: &Db,
    year: i16,
    scenario: Option<ScenarioId>,
    category: Option<CategoryId>,
    month: Option<u8>,
) -> Vec<TransactionId> {
    let Some((from, to)) = year_bounds(year) else {
        return Vec::new();
    };
    counted_transactions(db, from, to, scenario)
        .into_iter()
        .filter(|tx| bucket(db, tx.category_id) == category)
        .filter(|tx| month.is_none_or(|m| i32::from(tx.date.month()) == i32::from(m)))
        .map(|tx| tx.id)
        .collect()
}

/// R-RPT-5f: ids of the transactions behind a Total cell: as `matrix_drill` for the whole year, without
/// the months after `cutoff` (1..=12).
pub fn matrix_drill_through(
    db: &Db,
    year: i16,
    scenario: Option<ScenarioId>,
    category: Option<CategoryId>,
    cutoff: u8,
) -> Vec<TransactionId> {
    let Some((from, to)) = year_bounds(year) else {
        return Vec::new();
    };
    counted_transactions(db, from, to, scenario)
        .into_iter()
        .filter(|tx| bucket(db, tx.category_id) == category)
        .filter(|tx| i32::from(tx.date.month()) <= i32::from(cutoff))
        .map(|tx| tx.id)
        .collect()
}

/// R-RPT-5e: drill-down title: "{label} — {Mon} {year}" for a month, "{label} {year}" for Total.
pub fn matrix_drill_title(label: &str, year: i16, month: Option<u8>) -> String {
    match month {
        Some(m) => {
            let name = MONTH_SHORT
                .get(usize::from(m).wrapping_sub(1))
                .copied()
                .unwrap_or("");
            format!("{label} — {name} {year}")
        }
        None => format!("{label} {year}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;
    use pf_model::{AccountType, BudgetFrequency, BudgetItemType, CategoryType};
    use rust_decimal_macros::dec;

    #[test]
    fn r_rpt_5_year_options_and_modes() {
        // Covers: R-RPT-5
        assert_eq!(matrix_year_options(2026), 2024..=2031);
        let labels: Vec<&str> = MatrixMode::ALL.iter().map(|m| m.label()).collect();
        assert_eq!(labels, ["Actual + Budget", "Actual Only", "Budget Only"]);
        assert_eq!(MatrixMode::default(), MatrixMode::All);
    }

    #[test]
    fn r_rpt_5f_default_cutoff() {
        // Covers: R-RPT-5f
        let today = Date::new(2026, 3, 15).expect("a valid date");
        assert_eq!(default_matrix_cutoff(2026, today), 3);
        assert_eq!(default_matrix_cutoff(2025, today), 12);
        assert_eq!(default_matrix_cutoff(2027, today), 12);
    }

    #[test]
    fn r_rpt_5f_through_leaves_later_months_out() {
        // Covers: R-RPT-5f
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let groc = t.category("Groceries", CategoryType::Expense);
        let fun = t.category("Fun", CategoryType::Expense);
        let plan = t.scenario("Plan");
        t.tx(chk, "2026-03-31", "-10", "in").category(groc).done();
        t.tx(chk, "2026-04-01", "-20", "out").category(groc).done();
        // Only after the cutoff, and in a scenario: its row goes.
        t.tx(chk, "2026-06-01", "-30", "later")
            .category(fun)
            .scenario(plan)
            .done();
        t.budget_item(
            BudgetItemType::Expense,
            "Food",
            "5",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .category(groc)
        .done();
        let db = t.build();

        let whole = monthly_matrix(&db, 2026, Some(plan));
        assert_eq!(whole.rows.len(), 2);
        let m = whole.through(3);
        assert_eq!(m.rows.len(), 1, "the Fun row has nothing left");
        let row = &m.rows[0];
        assert_eq!(row.actual_total(), dec!(-10));
        assert_eq!(row.budget_total(), dec!(-15));
        assert_eq!(m.net_actual.iter().sum::<Decimal>(), dec!(-10));
        assert_eq!(m.net_budget.iter().sum::<Decimal>(), dec!(-15));

        let all = monthly_matrix(&db, 2026, Some(plan)).through(12);
        assert_eq!(
            all,
            monthly_matrix(&db, 2026, Some(plan)),
            "December is all"
        );

        assert_eq!(
            matrix_drill_through(&db, 2026, None, Some(groc), 3).len(),
            1,
            "the Total drill-down matches the Total"
        );
        assert_eq!(
            matrix_drill_through(&db, 2026, None, Some(groc), 4).len(),
            2
        );
    }

    #[test]
    fn r_rpt_5d_cell_display() {
        // Covers: R-RPT-5d
        assert_eq!(
            matrix_cell(MatrixMode::All, dec!(0), dec!(0)),
            MatrixCell::Dash
        );
        assert_eq!(
            matrix_cell(MatrixMode::All, dec!(-5), dec!(0)),
            MatrixCell::One(dec!(-5))
        );
        assert_eq!(
            matrix_cell(MatrixMode::All, dec!(-5), dec!(-3)),
            MatrixCell::Three {
                actual: dec!(-5),
                budget: dec!(-3),
                diff: dec!(-2)
            }
        );
        assert_eq!(
            matrix_cell(MatrixMode::ActualOnly, dec!(0), dec!(-3)),
            MatrixCell::One(dec!(0))
        );
        assert_eq!(
            matrix_cell(MatrixMode::BudgetOnly, dec!(-5), dec!(-3)),
            MatrixCell::One(dec!(-3))
        );
    }

    #[test]
    fn r_rpt_5_dec31_and_jan1_included_next_year_excluded() {
        // Covers: R-RPT-5a
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let groc = t.category("Groceries", CategoryType::Expense);
        t.tx(chk, "2026-01-01", "-1", "jan1").category(groc).done();
        t.tx(chk, "2026-12-31", "-2", "dec31").category(groc).done();
        t.tx(chk, "2027-01-01", "-4", "next").category(groc).done();
        t.tx(chk, "2025-12-31", "-8", "prev").category(groc).done();
        let db = t.build();
        let m = monthly_matrix(&db, 2026, None);
        assert_eq!(m.rows[0].actual[0], dec!(-1));
        assert_eq!(m.rows[0].actual[11], dec!(-2));
        assert_eq!(m.rows[0].actual_total(), dec!(-3));
    }

    #[test]
    fn r_rpt_5_budget_income_positive_bill_negative_per_occurrence() {
        // Covers: R-RPT-5b
        let mut t = TestDb::new();
        let inc = t.category("Income", CategoryType::Income);
        t.budget_item(
            BudgetItemType::Income,
            "Pay",
            "-4000",
            "2026-01-25",
            BudgetFrequency::Monthly,
        )
        .category(inc)
        .end("2026-02-28")
        .done();
        t.budget_item(
            BudgetItemType::Bill,
            "Rent",
            "100",
            "2026-03-01",
            BudgetFrequency::Monthly,
        )
        .end("2026-03-31")
        .done();
        let db = t.build();
        let m = monthly_matrix(&db, 2026, None);
        assert_eq!(row(&m, "Income").budget[0], dec!(4000));
        assert_eq!(row(&m, "Income").budget[1], dec!(4000));
        assert_eq!(row(&m, "Income").budget[2], dec!(0));
        assert_eq!(row(&m, "Uncategorized").budget[2], dec!(-100));
        assert_eq!(m.net_budget[2], dec!(-100));
    }

    /// The row labelled `label`.
    fn row<'a>(m: &'a MonthlyMatrix, label: &str) -> &'a MatrixRow {
        m.rows
            .iter()
            .find(|r| r.label == label)
            .unwrap_or_else(|| panic!("row {label}"))
    }

    #[test]
    fn r_rpt_5c_transfer_category_has_no_row_and_no_net() {
        // Covers: R-RPT-5b, R-RPT-5c
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let payment = t.category("Payment", CategoryType::Transfer);
        let groceries = t.category("Groceries", CategoryType::Expense);
        t.tx(chk, "2026-03-02", "-500", "Card payment")
            .category(payment)
            .done();
        t.tx(chk, "2026-03-03", "-10", "Store")
            .category(groceries)
            .done();
        t.budget_item(
            BudgetItemType::Bill,
            "Card",
            "500",
            "2026-03-01",
            BudgetFrequency::Monthly,
        )
        .category(payment)
        .end("2026-03-31")
        .done();
        let db = t.build();
        let m = monthly_matrix(&db, 2026, None);
        let labels: Vec<&str> = m.rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Groceries"]);
        assert_eq!(m.net_actual[2], dec!(-10));
        assert_eq!(m.net_budget[2], dec!(0), "its budget is not in NET either");
    }

    #[test]
    fn r_rpt_5e_title_formats() {
        // Covers: R-RPT-5e
        assert_eq!(
            matrix_drill_title("Groceries", 2026, Some(3)),
            "Groceries — Mar 2026"
        );
        assert_eq!(matrix_drill_title("Housing", 2026, None), "Housing 2026");
    }
}
