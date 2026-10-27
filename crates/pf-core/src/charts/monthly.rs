//! R-CHT-4 and R-CHT-8: profit and loss by month, and the income vs expense trend.

use std::collections::BTreeMap;

use jiff::Span;
use jiff::civil::Date;
use pf_model::{Db, TimePeriod, Transaction};
use rust_decimal::Decimal;

use super::{ChartFilter, filtered_transactions, month_label_short_ym};
use crate::filter::period_start;

/// P&L month picker: label and number of months; 6M is the default (R-CHT-4).
pub const PL_MONTH_OPTIONS: [(&str, u8); 4] = [("3M", 3), ("6M", 6), ("9M", 9), ("12M", 12)];

/// Trend period picker, 1Y is the default (R-CHT-8).
pub const TREND_PERIODS: [(&str, TimePeriod); 2] =
    [("1Y", TimePeriod::PastYear), ("All", TimePeriod::All)];

/// One calendar month of income and expenses.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthSummary {
    /// Calendar year.
    pub year: i16,
    /// Calendar month, 1..=12.
    pub month: i8,
    /// "MMM yy" label.
    pub label: String,
    /// Σ of positive amounts.
    pub income: Decimal,
    /// Σ of |non-positive amounts|.
    pub expenses: Decimal,
}

impl MonthSummary {
    /// R-CHT-4: income minus expenses.
    pub fn net(&self) -> Decimal {
        self.income - self.expenses
    }
}

/// R-CHT-4: the last `months` months through today, F⁻ᵗ, one row per month that has transactions.
pub fn profit_loss_by_month(db: &Db, f: &ChartFilter, months: u8) -> Vec<MonthSummary> {
    // R-CHT-4: window [today − N months, today]
    let start = f
        .today
        .checked_sub(Span::new().months(i64::from(months)))
        .ok();
    summarize(db, f, start)
}

/// R-CHT-8: F⁻ᵗ in the period window, per calendar month, chronological.
pub fn income_expense_trend(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<MonthSummary> {
    summarize(db, f, period_start(period, f.today))
}

/// Groups F⁻ᵗ rows dated in `[start, today]` by calendar month. Amounts > 0 are income and
/// every other amount is expense (Swift's `else` branch, R-CHT-4).
fn summarize(db: &Db, f: &ChartFilter, start: Option<Date>) -> Vec<MonthSummary> {
    let rows: Vec<&Transaction> = filtered_transactions(db, f, false)
        .into_iter()
        .filter(|tx| tx.date <= f.today && start.is_none_or(|s| tx.date >= s))
        .collect();
    let mut months: BTreeMap<(i16, i8), (Decimal, Decimal)> = BTreeMap::new();
    for tx in rows {
        let entry = months.entry((tx.date.year(), tx.date.month())).or_default();
        if tx.amount > Decimal::ZERO {
            entry.0 += tx.amount;
        } else {
            entry.1 += tx.amount.abs();
        }
    }
    months
        .into_iter()
        .map(|((year, month), (income, expenses))| MonthSummary {
            year,
            month,
            label: month_label_short_ym(year, month),
            income,
            expenses,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pf_model::testing::m;

    use super::*;
    use crate::charts::fixtures::{small_db, small_filter};

    /// (label, income, expenses, net) of each month, for whole-list comparisons.
    fn rows(v: &[MonthSummary]) -> Vec<(&str, Decimal, Decimal, Decimal)> {
        v.iter()
            .map(|s| (s.label.as_str(), s.income, s.expenses, s.net()))
            .collect()
    }

    #[test]
    /// Covers: R-CHT-4
    fn r_cht_4_pl_by_month_small() {
        let db = small_db();
        let f = small_filter(&db);
        let months = profit_loss_by_month(&db, &f, 6);
        assert_eq!(
            rows(&months),
            vec![
                ("Jan 26", m("6500"), m("1500"), m("5000")),
                ("Feb 26", m("4000"), m("1741.09"), m("2258.91")),
                ("Mar 26", m("0"), m("719.50"), m("-719.50")),
            ]
        );
        assert_eq!((months[0].year, months[0].month), (2026, 1));
    }

    #[test]
    /// Covers: R-CHT-4
    fn r_cht_4_options() {
        let labels: Vec<&str> = PL_MONTH_OPTIONS.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, ["3M", "6M", "9M", "12M"]);
        let values: Vec<u8> = PL_MONTH_OPTIONS.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, [3, 6, 9, 12]);
        let trend: Vec<&str> = TREND_PERIODS.iter().map(|(l, _)| *l).collect();
        assert_eq!(trend, ["1Y", "All"]);
    }

    #[test]
    /// Covers: R-CHT-8
    fn r_cht_8_trend_small() {
        let db = small_db();
        let f = small_filter(&db);
        let trend = income_expense_trend(&db, &f, TimePeriod::PastYear);
        assert_eq!(
            rows(&trend),
            vec![
                ("Jan 26", m("6500"), m("1500"), m("5000")),
                ("Feb 26", m("4000"), m("1741.09"), m("2258.91")),
                ("Mar 26", m("0"), m("719.50"), m("-719.50")),
            ]
        );
        let labels: Vec<&str> = trend.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Jan 26", "Feb 26", "Mar 26"]);
    }
}
