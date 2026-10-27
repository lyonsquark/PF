//! Profit & Loss report: budgeted and actual sums per category (R-RPT-1, R-RPT-2, R-RPT-3).

use std::collections::BTreeMap;

use jiff::civil::Date;
use pf_model::{BudgetItemId, CategoryId, CategoryType, Db, ScenarioId, TransactionId};
use rust_decimal::Decimal;

use super::{bucket, counted_transactions, signed_total};
use crate::projector::generate_dates;

/// One P&L row, or the final Total row.
#[derive(Debug, Clone, PartialEq)]
pub struct PlRow {
    /// Category full name, "Uncategorized", or "Total".
    pub label: String,
    /// None for Uncategorized and Total.
    pub category_id: Option<CategoryId>,
    /// Signed budget for the range (R-RPT-1).
    pub budgeted: Decimal,
    /// Signed actual for the range (R-RPT-2).
    pub actual: Decimal,
    /// True only for the Total row.
    pub is_total: bool,
    /// Transactions behind `actual`, in (date, id) order.
    pub transaction_ids: Vec<TransactionId>,
    /// Budget items behind `budgeted`, in id order.
    pub budget_item_ids: Vec<BudgetItemId>,
}

impl PlRow {
    /// R-RPT-3: actual − budgeted.
    pub fn difference(&self) -> Decimal {
        self.actual - self.budgeted
    }
}

/// Sums and ids for one category while the report is built.
#[derive(Default)]
struct Bucket {
    budgeted: Decimal,
    actual: Decimal,
    transaction_ids: Vec<TransactionId>,
    budget_item_ids: Vec<BudgetItemId>,
}

impl Bucket {
    /// R-RPT-3: a row is kept when budgeted or actual is non-zero.
    fn is_nonzero(&self) -> bool {
        !self.budgeted.is_zero() || !self.actual.is_zero()
    }

    fn into_row(self, label: String, category_id: Option<CategoryId>) -> PlRow {
        PlRow {
            label,
            category_id,
            budgeted: self.budgeted,
            actual: self.actual,
            is_total: false,
            transaction_ids: self.transaction_ids,
            budget_item_ids: self.budget_item_ids,
        }
    }
}

/// R-RPT-1/2/3: rows sorted by label, then the Total row (always present, even when empty).
/// The Total row's ids are the rows' ids concatenated in row order.
pub fn profit_and_loss(
    db: &Db,
    start: Date,
    end: Date,
    scenario: Option<ScenarioId>,
) -> Vec<PlRow> {
    let mut buckets: BTreeMap<Option<CategoryId>, Bucket> = BTreeMap::new();

    // R-RPT-1: budget items, bucketed by live category; items with no occurrence are skipped.
    for item in db.live_budget_items() {
        let count = generate_dates(item, start, end).len();
        if count == 0 {
            continue;
        }
        let b = buckets.entry(bucket(db, item.category_id)).or_default();
        b.budgeted += signed_total(item, count);
        b.budget_item_ids.push(item.id);
    }

    // R-RPT-2: actual transactions in (date, id) order.
    for tx in counted_transactions(db, start, end, scenario) {
        let b = buckets.entry(bucket(db, tx.category_id)).or_default();
        b.actual += tx.amount;
        b.transaction_ids.push(tx.id);
    }

    // R-RPT-3: one row per non-transfer live category, then Uncategorized.
    let mut rows: Vec<PlRow> = Vec::new();
    for cat in db.live_categories() {
        if cat.category_type == CategoryType::Transfer {
            continue;
        }
        if let Some(b) = buckets.remove(&Some(cat.id))
            && b.is_nonzero()
        {
            rows.push(b.into_row(db.category_full_name(cat.id), Some(cat.id)));
        }
    }
    if let Some(b) = buckets.remove(&None)
        && b.is_nonzero()
    {
        rows.push(b.into_row("Uncategorized".to_string(), None));
    }
    rows.sort_by(|a, b| a.label.cmp(&b.label));

    let total = total_row(&rows);
    rows.push(total);
    rows
}

/// R-RPT-3: the Total row, summing the given (already kept) rows.
fn total_row(rows: &[PlRow]) -> PlRow {
    PlRow {
        label: "Total".to_string(),
        category_id: None,
        budgeted: rows.iter().map(|r| r.budgeted).sum(),
        actual: rows.iter().map(|r| r.actual).sum(),
        is_total: true,
        transaction_ids: rows
            .iter()
            .flat_map(|r| r.transaction_ids.iter().copied())
            .collect(),
        budget_item_ids: rows
            .iter()
            .flat_map(|r| r.budget_item_ids.iter().copied())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, d};
    use pf_model::{AccountType, BudgetFrequency, BudgetItemType, RowKey, TransactionStatus};
    use rust_decimal_macros::dec;

    /// (label, budgeted, actual) of each row.
    fn summary(rows: &[PlRow]) -> Vec<(String, Decimal, Decimal)> {
        rows.iter()
            .map(|r| (r.label.clone(), r.budgeted, r.actual))
            .collect()
    }

    #[test]
    fn r_rpt_0_deleted_category_counts_as_uncategorized() {
        // Covers: R-RPT-0
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let groc = t.category("Groceries", CategoryType::Expense);
        let old_dining = t.category("Old Dining", CategoryType::Expense);
        let old_transfer = t.category("Old Transfer", CategoryType::Transfer);
        t.tx(chk, "2026-01-05", "-20", "A").category(groc).done();
        t.tx(chk, "2026-01-06", "-30", "B")
            .category(old_dining)
            .done();
        t.tx(chk, "2026-01-07", "-40", "C")
            .category(old_transfer)
            .done();
        t.delete(RowKey::Category(old_dining));
        t.delete(RowKey::Category(old_transfer));
        let db = t.build();

        let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-01-31"), None);
        assert_eq!(
            summary(&rows),
            vec![
                ("Groceries".to_string(), dec!(0), dec!(-20)),
                ("Uncategorized".to_string(), dec!(0), dec!(-70)),
                ("Total".to_string(), dec!(0), dec!(-90)),
            ]
        );

        // The matrix agrees: the deleted Transfer no longer excludes its transaction.
        let matrix = super::super::monthly_matrix(&db, 2026, None);
        let uncat = matrix
            .rows
            .iter()
            .find(|r| r.label == "Uncategorized")
            .expect("uncategorized row");
        assert_eq!(uncat.actual[0], dec!(-70));
    }

    #[test]
    fn r_rpt_1_budgeted_sign_and_count() {
        // Covers: R-RPT-1
        let mut t = TestDb::new();
        let inc = t.category("Income", CategoryType::Income);
        let rent = t.category("Rent", CategoryType::Expense);
        t.budget_item(
            BudgetItemType::Income,
            "Salary",
            "4000",
            "2026-01-25",
            BudgetFrequency::Monthly,
        )
        .category(inc)
        .done();
        // Stored negative: still −|amount| for a Bill.
        t.budget_item(
            BudgetItemType::Bill,
            "Rent",
            "-100",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .category(rent)
        .done();
        // A plain Expense, outside the range: contributes nothing.
        t.budget_item(
            BudgetItemType::Expense,
            "Later",
            "50",
            "2026-07-01",
            BudgetFrequency::Monthly,
        )
        .category(rent)
        .done();
        let db = t.build();

        let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-03-31"), None);
        // Salary on Jan 25, Feb 25, Mar 25 (3 × 4000); Rent on Jan 1, Feb 1, Mar 1 (3 × −100).
        assert_eq!(
            summary(&rows),
            vec![
                ("Income".to_string(), dec!(12000), dec!(0)),
                ("Rent".to_string(), dec!(-300), dec!(0)),
                ("Total".to_string(), dec!(11700), dec!(0)),
            ]
        );
    }

    #[test]
    fn r_rpt_2_actual_filters() {
        // Covers: R-RPT-2
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let ext = t.external_account("Brokerage", AccountType::Investment);
        let groc = t.category("Groceries", CategoryType::Expense);
        let xfer = t.category("Credit Card Payment", CategoryType::Transfer);
        t.tx(chk, "2026-01-01", "-10", "start day")
            .category(groc)
            .done();
        t.tx(chk, "2026-01-31", "-20", "end day")
            .category(groc)
            .done();
        t.tx(chk, "2026-02-01", "-1000", "after")
            .category(groc)
            .done();
        t.tx(chk, "2025-12-31", "-500", "before")
            .category(groc)
            .done();
        t.tx(ext, "2026-01-15", "-300", "external")
            .category(groc)
            .done();
        t.orphan_tx("2026-01-10", "-5", "orphan").done();
        t.tx(chk, "2026-01-12", "-7", "transfer")
            .category(xfer)
            .done();
        t.tx(chk, "2026-01-13", "-9", "deleted")
            .category(groc)
            .deleted()
            .done();
        let db = t.build();

        let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-01-31"), None);
        assert_eq!(
            summary(&rows),
            vec![
                ("Groceries".to_string(), dec!(0), dec!(-30)),
                ("Uncategorized".to_string(), dec!(0), dec!(-5)),
                ("Total".to_string(), dec!(0), dec!(-35)),
            ]
        );
        let total = rows.last().expect("total");
        assert_eq!(total.transaction_ids.len(), 3);
    }

    #[test]
    fn r_rpt_2_scenario_visibility() {
        // Covers: R-RPT-2, R-SCN-1
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let groc = t.category("Groceries", CategoryType::Expense);
        let plan_a = t.scenario("Plan A");
        let plan_b = t.scenario("Plan B");
        t.tx(chk, "2026-01-05", "-10", "real").category(groc).done();
        t.tx(chk, "2026-01-06", "-100", "a")
            .category(groc)
            .scenario(plan_a)
            .done();
        t.tx(chk, "2026-01-07", "-1000", "b")
            .category(groc)
            .scenario(plan_b)
            .done();
        let db = t.build();
        let (start, end) = (d("2026-01-01"), d("2026-01-31"));

        let reality = profit_and_loss(&db, start, end, None);
        assert_eq!(reality[0].actual, dec!(-10));
        let with_a = profit_and_loss(&db, start, end, Some(plan_a));
        assert_eq!(with_a[0].actual, dec!(-110));
    }

    #[test]
    fn r_rpt_3_rows_sorted_total_last() {
        // Covers: R-RPT-3
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let zeta = t.category("Zeta", CategoryType::Expense);
        let alpha = t.category("Alpha", CategoryType::Expense);
        let income = t.category("Income", CategoryType::Income);
        let housing = t.category("Housing", CategoryType::Expense);
        let utilities = t.child_category(housing, "Utilities");
        let zero = t.category("Zero", CategoryType::Expense);
        let xfer = t.category("Transfers", CategoryType::Transfer);
        t.tx(chk, "2026-01-02", "-5", "z").category(zeta).done();
        t.tx(chk, "2026-01-03", "-1", "a").category(alpha).done();
        t.tx(chk, "2026-01-04", "100", "i").category(income).done();
        t.tx(chk, "2026-01-05", "-3", "u")
            .category(utilities)
            .done();
        t.tx(chk, "2026-01-06", "5", "z+").category(zero).done();
        t.tx(chk, "2026-01-07", "-5", "z-").category(zero).done();
        t.tx(chk, "2026-01-08", "-50", "x").category(xfer).done();
        t.budget_item(
            BudgetItemType::Bill,
            "Zeta bill",
            "10",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .category(zeta)
        .done();
        // No budget and no actual: no row.
        let _ = t.category("Beta", CategoryType::Expense);
        let db = t.build();

        let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-01-31"), None);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Alpha", "Housing/Utilities", "Income", "Zeta", "Total"]
        );
        let total = rows.last().expect("total");
        assert!(total.is_total);
        assert_eq!(total.budgeted, dec!(-10));
        assert_eq!(total.actual, dec!(91));
        // Zeta: budgeted −10, actual −5 ⇒ difference +5.
        let zeta_row = &rows[3];
        assert_eq!(zeta_row.difference(), dec!(5));
        assert_eq!(zeta_row.budget_item_ids.len(), 1);
        assert_eq!(total.difference(), dec!(101));
    }

    #[test]
    fn r_rpt_3_empty_db_has_only_total() {
        // Covers: R-RPT-3
        let db = Db::new();
        let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-01-31"), None);
        assert_eq!(rows.len(), 1);
        let total = &rows[0];
        assert_eq!(total.label, "Total");
        assert!(total.is_total);
        assert_eq!(total.budgeted, Decimal::ZERO);
        assert_eq!(total.actual, Decimal::ZERO);
        assert_eq!(total.category_id, None);
    }

    #[test]
    fn r_rpt_2_pending_counts_in_actual() {
        // Covers: R-RPT-2 (status does not filter P&L actuals)
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let groc = t.category("Groceries", CategoryType::Expense);
        t.tx(chk, "2026-01-05", "-12.50", "pending")
            .category(groc)
            .status(TransactionStatus::Pending)
            .done();
        let db = t.build();
        let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-01-31"), None);
        assert_eq!(rows[0].actual, dec!(-12.50));
    }
}
