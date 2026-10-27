//! R-CHT-1: account balances over time (the Net Worth card).

use jiff::civil::Date;
use pf_model::{Db, TimePeriod, Transaction};
use rust_decimal::Decimal;

use super::{ChartFilter, filtered_transactions};
use crate::filter::period_start;

/// Period buttons of the balance card, in display order; 1Y is the default (R-CHT-1).
pub const BALANCE_PERIODS: [(&str, TimePeriod); 5] = [
    ("1M", TimePeriod::PastMonth),
    ("60D", TimePeriod::Past60Days),
    ("YTD", TimePeriod::YearToDate),
    ("1Y", TimePeriod::PastYear),
    ("All", TimePeriod::All),
];

/// One point of the balance line.
#[derive(Debug, Clone, PartialEq)]
pub struct BalancePoint {
    /// Day of the point.
    pub date: Date,
    /// Running balance after that day.
    pub balance: Decimal,
}

/// R-CHT-1, R-CHT-1a, R-CHT-1b: running balance of F (transfers included) over the window.
/// The header value is the last point's balance (0 when empty).
pub fn balance_over_time(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<BalancePoint> {
    // R-CHT-1a: transactions dated after today are excluded entirely (fix)
    let txs: Vec<&Transaction> = filtered_transactions(db, f, true)
        .into_iter()
        .filter(|tx| tx.date <= f.today)
        .collect();
    // R-CHT-1a: the window start for All is the first transaction's date (fix)
    let window_start = match period_start(period, f.today) {
        Some(start) => start,
        None => match txs.first() {
            Some(tx) => tx.date,
            None => return Vec::new(),
        },
    };

    let mut running = Decimal::ZERO;
    let mut points: Vec<BalancePoint> = Vec::new();
    let mut last_seen: Option<Date> = None;
    for tx in &txs {
        running += tx.amount;
        if tx.date < window_start {
            continue;
        }
        // R-CHT-1a: the first point inside the window is the balance before it
        if points.is_empty() {
            points.push(BalancePoint {
                date: window_start,
                balance: running - tx.amount,
            });
        }
        // R-CHT-1a: one point per day, holding the balance after the day's last transaction
        if last_seen == Some(tx.date) {
            if let Some(last) = points.last_mut() {
                last.balance = running;
            }
        } else {
            points.push(BalancePoint {
                date: tx.date,
                balance: running,
            });
            last_seen = Some(tx.date);
        }
    }

    // R-CHT-1b: no points but a non-zero running sum gives two points across the window
    if points.is_empty() {
        if running != Decimal::ZERO {
            points.push(BalancePoint {
                date: window_start,
                balance: running,
            });
            points.push(BalancePoint {
                date: f.today,
                balance: running,
            });
        }
    } else if points.last().is_some_and(|p| p.date < f.today) {
        points.push(BalancePoint {
            date: f.today,
            balance: running,
        });
    }
    points
}

#[cfg(test)]
mod tests {
    use pf_model::testing::{TestDb, d, m};
    use pf_model::{AccountType, TransactionStatus};

    use super::*;
    use crate::charts::fixtures::{account_id, small_db, small_filter};

    /// (date, balance) pairs of a series, for comparing whole series.
    fn pairs(points: &[BalancePoint]) -> Vec<(Date, Decimal)> {
        points.iter().map(|p| (p.date, p.balance)).collect()
    }

    #[test]
    /// Covers: R-CHT-1, R-CHT-1a
    fn r_cht_1a_balance_points_small() {
        let db = small_db();
        let f = small_filter(&db);
        let points = balance_over_time(&db, &f, TimePeriod::PastYear);
        let expected = vec![
            (d("2025-03-15"), m("0")),
            (d("2026-01-24"), m("2500")),
            (d("2026-01-25"), m("6500")),
            (d("2026-01-31"), m("5000")),
            (d("2026-02-03"), m("4935.01")),
            (d("2026-02-14"), m("4847.01")),
            (d("2026-02-17"), m("4758.91")),
            (d("2026-02-25"), m("8758.91")),
            (d("2026-02-28"), m("7258.91")),
            (d("2026-03-01"), m("7138.46")),
            (d("2026-03-02"), m("6996.16")),
            (d("2026-03-05"), m("6996.16")),
            (d("2026-03-10"), m("6546.16")),
            (d("2026-03-12"), m("6539.41")),
            (d("2026-03-15"), m("6539.41")),
        ];
        assert_eq!(pairs(&points), expected);
    }

    #[test]
    /// Covers: R-CHT-1a
    fn r_cht_1a_all_period_starts_at_first_tx() {
        let db = small_db();
        let f = small_filter(&db);
        let points = balance_over_time(&db, &f, TimePeriod::All);
        assert_eq!(points[0].date, d("2026-01-24"));
        assert_eq!(points[0].balance, m("0"));
        assert_eq!(points.last().map(|p| p.balance), Some(m("6539.41")));
    }

    #[test]
    /// Covers: R-CHT-1a
    fn r_cht_1a_future_rows_excluded() {
        let db = small_db();
        let plan_a = db.scenario_by_name("Plan A").expect("Plan A").id;
        let f = ChartFilter {
            scenario: Some(plan_a),
            ..small_filter(&db)
        };
        let points = balance_over_time(&db, &f, TimePeriod::PastYear);
        let reality = balance_over_time(&db, &small_filter(&db), TimePeriod::PastYear);
        assert_eq!(points, reality);
        let last = points.last().expect("points");
        assert_eq!((last.date, last.balance), (d("2026-03-15"), m("6539.41")));
    }

    #[test]
    /// Covers: R-CHT-1b
    fn r_cht_1b_edge_cases() {
        // No transactions at all: empty.
        let empty = Db::default();
        let f = ChartFilter {
            accounts: Default::default(),
            scenario: None,
            today: d("2026-03-15"),
        };
        assert!(balance_over_time(&empty, &f, TimePeriod::PastYear).is_empty());

        // Every transaction is before the window: two points at the window edges.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2020-01-01", "100", "Old")
            .status(TransactionStatus::Cleared)
            .done();
        let db = t.build();
        let f = ChartFilter {
            accounts: [account_id(&db, "Checking")].into_iter().collect(),
            scenario: None,
            today: d("2026-03-15"),
        };
        let points = balance_over_time(&db, &f, TimePeriod::PastMonth);
        assert_eq!(
            pairs(&points),
            vec![(d("2026-02-15"), m("100")), (d("2026-03-15"), m("100"))]
        );
        assert_eq!(points.last().map(|p| p.balance), Some(m("100")));

        // Balance zero before the window: empty.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2020-01-01", "100", "In").done();
        t.tx(chk, "2020-01-02", "-100", "Out").done();
        let db = t.build();
        let f = ChartFilter {
            accounts: [account_id(&db, "Checking")].into_iter().collect(),
            ..f
        };
        assert!(balance_over_time(&db, &f, TimePeriod::PastMonth).is_empty());
    }
}
