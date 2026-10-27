//! Drill-down rows behind report numbers: transactions (R-RPT-3b) and budget items (R-RPT-3c).

use jiff::civil::Date;
use pf_model::{BudgetItemId, Db, TransactionId};
use rust_decimal::Decimal;

use super::{budget_total_in_window, live_category};

/// One transaction row in a drill-down table (R-RPT-3b).
#[derive(Debug, Clone, PartialEq)]
pub struct DrillRow {
    /// Transaction id (the tie-breaker for ordering).
    pub id: TransactionId,
    /// Transaction date.
    pub date: Date,
    /// Account name, or "Unknown" when there is none.
    pub account: String,
    /// Payee.
    pub payee: String,
    /// Signed amount.
    pub amount: Decimal,
    /// Memo, or "" when none.
    pub memo: String,
    /// Context names joined with ", ".
    pub contexts: String,
    /// Status display label ("Pending", "Cleared", ...).
    pub status: &'static str,
    /// Scenario name, "Reality" for reality rows, or "" when no scenario is active.
    pub scenario: String,
}

/// R-RPT-3b: rows for `ids`, sorted by date descending, then id descending. The Scenario column is
/// empty unless `scenario_active`. Unknown ids are skipped.
pub fn drill_rows(db: &Db, ids: &[TransactionId], scenario_active: bool) -> Vec<DrillRow> {
    let mut rows: Vec<DrillRow> = ids
        .iter()
        .filter_map(|id| db.transaction(*id))
        .map(|tx| DrillRow {
            id: tx.id,
            date: tx.date,
            account: tx
                .account_id
                .and_then(|a| db.account(a))
                .map_or_else(|| "Unknown".to_string(), |a| a.name.clone()),
            payee: tx.payee.clone(),
            amount: tx.amount,
            memo: tx.memo.clone().unwrap_or_default(),
            contexts: tx
                .context_ids
                .iter()
                .filter_map(|c| db.context(*c))
                .map(|c| c.name.as_str())
                .collect::<Vec<&str>>()
                .join(", "),
            status: tx.status.label(),
            scenario: if !scenario_active {
                String::new()
            } else {
                tx.scenario_id
                    .and_then(|s| db.scenario(s))
                    .map_or_else(|| "Reality".to_string(), |s| s.name.clone())
            },
        })
        .collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    rows
}

/// R-RPT-3b: the P&L drill-down title.
pub fn pnl_drill_title(label: &str) -> String {
    format!("{label} Transactions")
}

/// One budget item row in a drill-down table (R-RPT-3c).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetDrillRow {
    /// Budget item id.
    pub id: BudgetItemId,
    /// Item name.
    pub name: String,
    /// Live category name, or "Uncategorized".
    pub category: String,
    /// Raw type string ("Bill", "Expense", "Income").
    pub item_type: &'static str,
    /// Raw frequency string.
    pub frequency: &'static str,
    /// Amount per period (stored, unsigned).
    pub amount: Decimal,
    /// Start date of the item.
    pub start_date: Date,
    /// Signed total in the window (R-RPT-1).
    pub calculated_total: Decimal,
}

/// R-RPT-3c: rows for `ids` sorted by name (ties by id), with the total for [start, end].
/// Unknown ids are skipped.
pub fn budget_drill_rows(
    db: &Db,
    ids: &[BudgetItemId],
    start: Date,
    end: Date,
) -> Vec<BudgetDrillRow> {
    let mut rows: Vec<BudgetDrillRow> = ids
        .iter()
        .filter_map(|id| db.budget_item(*id))
        .map(|item| BudgetDrillRow {
            id: item.id,
            name: item.name.clone(),
            category: live_category(db, item.category_id)
                .map_or_else(|| "Uncategorized".to_string(), |c| c.name.clone()),
            item_type: item.item_type.as_str(),
            frequency: item.frequency.as_str(),
            amount: item.amount,
            start_date: item.start_date,
            calculated_total: budget_total_in_window(item, start, end),
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    rows
}

/// R-RPT-3c: the budget drill-down title.
pub fn budget_drill_title(label: &str) -> String {
    format!("{label} Budget Items")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, d};
    use pf_model::{AccountType, BudgetFrequency, BudgetItemType, CategoryType, TransactionStatus};
    use rust_decimal_macros::dec;

    #[test]
    fn r_rpt_3b_drill_rows_format_and_order() {
        // Covers: R-RPT-3b
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let ca = t.context("a");
        let cb = t.context("b");
        let plan = t.scenario("Plan A");
        let orphan = t
            .orphan_tx("2026-01-03", "-5", "Orphan")
            .status(TransactionStatus::Reconciled)
            .done();
        let t1 = t
            .tx(chk, "2026-01-05", "-10", "Bob")
            .contexts(&[ca, cb])
            .status(TransactionStatus::Pending)
            .done();
        let t2 = t
            .tx(chk, "2026-01-05", "-20", "Ann")
            .memo("m")
            .scenario(plan)
            .status(TransactionStatus::Cleared)
            .done();
        let db = t.build();

        let rows = drill_rows(&db, &[orphan, t1, t2], true);
        let ids: Vec<TransactionId> = rows.iter().map(|r| r.id).collect();
        // Same date: higher id first; the older date last.
        assert_eq!(ids, [t2, t1, orphan]);

        assert_eq!(rows[0].memo, "m");
        assert_eq!(rows[0].status, "Cleared");
        assert_eq!(rows[0].scenario, "Plan A");
        assert_eq!(rows[0].account, "Checking");

        assert_eq!(rows[1].memo, "");
        assert_eq!(rows[1].contexts, "a, b");
        assert_eq!(rows[1].status, "Pending");
        assert_eq!(rows[1].scenario, "Reality");
        assert_eq!(rows[1].payee, "Bob");
        assert_eq!(rows[1].amount, dec!(-10));

        assert_eq!(rows[2].account, "Unknown");
        assert_eq!(rows[2].status, "Reconciled");
        assert_eq!(rows[2].scenario, "Reality");
        assert_eq!(rows[2].date, d("2026-01-03"));

        let hidden = drill_rows(&db, &[orphan, t1, t2], false);
        assert!(hidden.iter().all(|r| r.scenario.is_empty()));

        assert_eq!(pnl_drill_title("Groceries"), "Groceries Transactions");
    }

    #[test]
    fn r_rpt_3c_budget_drill_rows() {
        // Covers: R-RPT-3c, R-RPT-1
        let mut t = TestDb::new();
        let housing = t.category("Housing", CategoryType::Expense);
        let zebra = t
            .budget_item(
                BudgetItemType::Bill,
                "Zebra",
                "40",
                "2026-01-10",
                BudgetFrequency::Monthly,
            )
            .category(housing)
            .done();
        let apple = t
            .budget_item(
                BudgetItemType::Income,
                "Apple",
                "100",
                "2026-01-01",
                BudgetFrequency::Monthly,
            )
            .done();
        let gym = t
            .budget_item(
                BudgetItemType::Expense,
                "Gym",
                "10",
                "2026-01-05",
                BudgetFrequency::Weekly,
            )
            .done();
        let db = t.build();

        let rows = budget_drill_rows(&db, &[zebra, apple, gym], d("2026-01-01"), d("2026-03-31"));
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Apple", "Gym", "Zebra"]);

        // Apple: Income, Monthly, no category, Jan 1 / Feb 1 / Mar 1.
        assert_eq!(rows[0].category, "Uncategorized");
        assert_eq!(rows[0].item_type, "Income");
        assert_eq!(rows[0].frequency, "Monthly");
        assert_eq!(rows[0].calculated_total, dec!(300));
        assert_eq!(rows[0].start_date, d("2026-01-01"));

        // Gym: Expense, Weekly, 13 Mondays from Jan 5 to Mar 30.
        assert_eq!(rows[1].item_type, "Expense");
        assert_eq!(rows[1].frequency, "Weekly");
        assert_eq!(rows[1].calculated_total, dec!(-130));

        // Zebra: Bill, category Housing, 3 occurrences, negative.
        assert_eq!(rows[2].category, "Housing");
        assert_eq!(rows[2].item_type, "Bill");
        assert_eq!(rows[2].amount, dec!(40));
        assert_eq!(rows[2].calculated_total, dec!(-120));

        assert_eq!(budget_drill_title("Groceries"), "Groceries Budget Items");
    }
}
