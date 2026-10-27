//! R-CHT-6: the monthly cashflow waterfall, and its drill-down (R-CHT-6b).

use jiff::civil::Date;
use pf_model::{Db, TransactionId};
use rust_decimal::Decimal;

use super::{
    COLOR_BLUE, COLOR_EXPENSE, COLOR_INCOME, ChartFilter, group_name, month_label_long, month_txs,
    sum_abs_by,
};

/// Expense bar colours, cycled in bar order (R-CHT-6a). "Other" takes the next slot.
pub const EXPENSE_CYCLE: [&str; 8] = [
    "#ff3b30", "#ff6b55", "#ff9500", "#af52de", "#32ade6", "#ffcc00", "#ff6961", "#ff453a",
];

/// Expense groups kept as their own bars before the rest goes into "Other" (R-CHT-6a).
const MAX_GROUPS: usize = 6;

/// What a waterfall bar stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    /// The "Income" bar.
    Income,
    /// One expense group.
    Expense,
    /// The sum of the expense groups beyond the top six.
    Other,
    /// The "Savings" bar at the end.
    Savings,
}

/// One floating bar: from `y_start` to `y_end`.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterfallBar {
    /// Bar label; a group name, "Income", "Other" or "Savings".
    pub label: String,
    /// Running total before the bar.
    pub y_start: Decimal,
    /// Running total after the bar.
    pub y_end: Decimal,
    /// What the bar stands for.
    pub kind: BarKind,
    /// `#rrggbb` colour.
    pub color: String,
}

/// The whole waterfall for one month.
#[derive(Debug, Clone, PartialEq)]
pub struct Waterfall {
    /// "MMMM yyyy" of the base date.
    pub month_label: String,
    /// Σ of positive amounts.
    pub total_in: Decimal,
    /// Σ of |non-positive amounts|.
    pub total_out: Decimal,
    /// Bars in order; empty means "Not enough data".
    pub bars: Vec<WaterfallBar>,
}

/// R-CHT-6, R-CHT-6a: the waterfall for the month of `base_date` (F⁻ᵗ).
pub fn waterfall(db: &Db, f: &ChartFilter, base_date: Date) -> Waterfall {
    let rows = month_txs(db, f, base_date);
    let income: Decimal = rows
        .iter()
        .filter(|tx| tx.amount > Decimal::ZERO)
        .map(|tx| tx.amount)
        .sum();
    let expense_rows: Vec<_> = rows
        .iter()
        .copied()
        .filter(|tx| tx.amount <= Decimal::ZERO)
        .collect();
    let groups = sum_abs_by(&expense_rows, |tx| group_name(db, tx));
    let total_out: Decimal = groups.iter().map(|g| g.1).sum();
    let month_label = month_label_long(base_date);
    if income.is_zero() && groups.is_empty() {
        return Waterfall {
            month_label,
            total_in: income,
            total_out,
            bars: Vec::new(),
        };
    }

    let mut bars = vec![WaterfallBar {
        label: "Income".to_string(),
        y_start: Decimal::ZERO,
        y_end: income,
        kind: BarKind::Income,
        color: COLOR_INCOME.to_string(),
    }];
    let mut running = income;
    let mut colour_index = 0;
    for (name, value) in groups.iter().take(MAX_GROUPS) {
        let end = running - value;
        bars.push(expense_bar(
            name.clone(),
            running,
            end,
            BarKind::Expense,
            colour_index,
        ));
        colour_index += 1;
        running = end;
    }
    // R-CHT-6a: groups beyond the top six become one "Other" bar
    if groups.len() > MAX_GROUPS {
        let rest: Decimal = groups[MAX_GROUPS..].iter().map(|g| g.1).sum();
        let end = running - rest;
        bars.push(expense_bar(
            "Other".to_string(),
            running,
            end,
            BarKind::Other,
            colour_index,
        ));
        running = end;
    }
    // R-CHT-6a: savings is the running total, red when negative
    bars.push(WaterfallBar {
        label: "Savings".to_string(),
        y_start: Decimal::ZERO,
        y_end: running,
        kind: BarKind::Savings,
        color: if running >= Decimal::ZERO {
            COLOR_BLUE
        } else {
            COLOR_EXPENSE
        }
        .to_string(),
    });
    Waterfall {
        month_label,
        total_in: income,
        total_out,
        bars,
    }
}

/// An expense bar coloured from [`EXPENSE_CYCLE`] by its position among the expense bars.
fn expense_bar(
    label: String,
    y_start: Decimal,
    y_end: Decimal,
    kind: BarKind,
    index: usize,
) -> WaterfallBar {
    WaterfallBar {
        label,
        y_start,
        y_end,
        kind,
        color: EXPENSE_CYCLE[index % EXPENSE_CYCLE.len()].to_string(),
    }
}

/// R-CHT-6b: the drill-down for a clicked bar label: (title, transaction ids in (date, id) order).
/// Income ⇒ positive rows; Savings ⇒ all rows; Other ⇒ negative rows outside the top six;
/// otherwise ⇒ negative rows of that group.
pub fn waterfall_drill(
    db: &Db,
    f: &ChartFilter,
    base_date: Date,
    label: &str,
) -> (String, Vec<TransactionId>) {
    let rows = month_txs(db, f, base_date);
    let ids: Vec<TransactionId> = match label {
        "Income" => rows
            .iter()
            .filter(|tx| tx.amount > Decimal::ZERO)
            .map(|tx| tx.id)
            .collect(),
        "Savings" => rows.iter().map(|tx| tx.id).collect(),
        "Other" => {
            let expense_rows: Vec<_> = rows
                .iter()
                .copied()
                .filter(|tx| tx.amount <= Decimal::ZERO)
                .collect();
            let top: Vec<String> = sum_abs_by(&expense_rows, |tx| group_name(db, tx))
                .into_iter()
                .take(MAX_GROUPS)
                .map(|g| g.0)
                .collect();
            rows.iter()
                .filter(|tx| tx.amount < Decimal::ZERO && !top.contains(&group_name(db, tx)))
                .map(|tx| tx.id)
                .collect()
        }
        _ => rows
            .iter()
            .filter(|tx| tx.amount < Decimal::ZERO && group_name(db, tx) == label)
            .map(|tx| tx.id)
            .collect(),
    };
    (format!("{label} — {}", month_label_long(base_date)), ids)
}

#[cfg(test)]
mod tests {
    use pf_model::testing::{TestDb, d, m};
    use pf_model::{AccountType, CategoryType};

    use super::*;
    use crate::charts::fixtures::{account_id, small_db, small_filter};

    /// (label, y_start, y_end, colour) of each bar.
    fn bars(w: &Waterfall) -> Vec<(&str, Decimal, Decimal, &str)> {
        w.bars
            .iter()
            .map(|b| (b.label.as_str(), b.y_start, b.y_end, b.color.as_str()))
            .collect()
    }

    #[test]
    /// Covers: R-CHT-6, R-CHT-6a
    fn r_cht_6a_waterfall_feb() {
        let db = small_db();
        let f = small_filter(&db);
        let w = waterfall(&db, &f, d("2026-02-15"));
        assert_eq!(
            bars(&w),
            vec![
                ("Income", m("0"), m("4000"), "#34c759"),
                ("Housing", m("4000"), m("2346.91"), "#ff3b30"),
                ("Dining", m("2346.91"), m("2258.91"), "#ff6b55"),
                ("Savings", m("0"), m("2258.91"), "#007aff"),
            ]
        );
        assert_eq!(w.total_in, m("4000"));
        assert_eq!(w.total_out, m("1741.09"));
        assert_eq!(w.month_label, "February 2026");
    }

    #[test]
    /// Covers: R-CHT-6a
    fn r_cht_6a_waterfall_negative_savings_and_other() {
        let db = small_db();
        let f = small_filter(&db);
        let w = waterfall(&db, &f, d("2026-03-15"));
        assert_eq!(
            bars(&w),
            vec![
                ("Income", m("0"), m("0"), "#34c759"),
                ("Uncategorized", m("0"), m("-450"), "#ff3b30"),
                ("Groceries", m("-450"), m("-592.30"), "#ff6b55"),
                ("Housing", m("-592.30"), m("-712.75"), "#ff9500"),
                ("Dining", m("-712.75"), m("-719.50"), "#af52de"),
                ("Savings", m("0"), m("-719.50"), "#ff3b30"),
            ]
        );

        // Eight expense groups plus income: six bars, one Other, then Savings.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-03-01", "1000", "Pay").done();
        for i in 1..=8 {
            let cat = t.category(&format!("Cat{i}"), CategoryType::Expense);
            t.tx(chk, "2026-03-05", &format!("-{}", i * 10), "Shop")
                .category(cat)
                .done();
        }
        let big = t.build();
        let f2 = ChartFilter {
            accounts: [account_id(&big, "Checking")].into_iter().collect(),
            scenario: None,
            today: d("2026-03-15"),
        };
        let w = waterfall(&big, &f2, d("2026-03-15"));
        let kinds: Vec<BarKind> = w.bars.iter().map(|b| b.kind).collect();
        assert_eq!(kinds.iter().filter(|k| **k == BarKind::Other).count(), 1);
        let other_pos = kinds.iter().position(|k| *k == BarKind::Other);
        assert_eq!(other_pos, Some(7));
        assert_eq!(kinds.last(), Some(&BarKind::Savings));
        // Top six take 330, the other two groups 30 more.
        assert_eq!(w.bars[7].y_end, m("640"));
        assert_eq!(w.bars.last().map(|b| b.y_end), Some(m("640")));
    }

    #[test]
    /// Covers: R-CHT-6b
    fn r_cht_6b_waterfall_drill() {
        let db = small_db();
        let f = small_filter(&db);
        let base = d("2026-02-15");
        let title = |label: &str| waterfall_drill(&db, &f, base, label).0;
        let payees = |label: &str| -> Vec<String> {
            let ids = waterfall_drill(&db, &f, base, label).1;
            ids.iter()
                .filter_map(|id| db.transaction(*id))
                .map(|tx| tx.payee.clone())
                .collect()
        };
        assert_eq!(payees("Income"), ["Employer Payroll"]);
        // (date, id) order; the importer unescapes the full-width bar to "|".
        assert_eq!(
            payees("Housing"),
            ["A|B Hardware", "City Water", "Mortgage"]
        );
        assert_eq!(payees("Savings").len(), 5);
        assert_eq!(title("Housing"), "Housing — February 2026");
    }
}
