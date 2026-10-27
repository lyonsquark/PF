//! R-CHT-5 and R-CHT-7: the category donut and the expense distribution bars.

use pf_model::{Db, TimePeriod, Transaction};
use rust_decimal::Decimal;

use super::{
    COLOR_GREY, ChartFilter, Slice, category_hex, category_name_or_uncategorized,
    filtered_transactions, group_name, sum_abs_by,
};
use crate::filter::period_start;

/// Donut period buttons, 1M is the default (R-CHT-5).
pub const BREAKDOWN_PERIODS: [(&str, TimePeriod); 4] = [
    ("1M", TimePeriod::PastMonth),
    ("YTD", TimePeriod::YearToDate),
    ("1Y", TimePeriod::PastYear),
    ("All", TimePeriod::All),
];

/// Distribution period buttons, 1Y is the default (R-CHT-7).
pub const DISTRIBUTION_PERIODS: [(&str, TimePeriod); 5] = [
    ("1M", TimePeriod::PastMonth),
    ("60D", TimePeriod::Past60Days),
    ("YTD", TimePeriod::YearToDate),
    ("1Y", TimePeriod::PastYear),
    ("All", TimePeriod::All),
];

/// R-CHT-5: donut slices kept before the rest is folded into "Other".
const MAX_BREAKDOWN_SLICES: usize = 6;
/// R-CHT-7: bars shown in the expense distribution.
const MAX_DISTRIBUTION_ROWS: usize = 15;

/// R-CHT-5: spending F⁻ᵗ in the window, grouped by [`group_name`]; the top 6 by amount, and the
/// rest summed into "Other" when that sum is above zero.
pub fn category_breakdown(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<Slice> {
    let rows = spending_in_window(db, f, period);
    let groups = sum_abs_by(&rows, |tx| group_name(db, tx));
    let mut slices: Vec<Slice> = Vec::new();
    let mut other = Decimal::ZERO;
    for (i, (name, amount)) in groups.into_iter().enumerate() {
        if i < MAX_BREAKDOWN_SLICES {
            slices.push(slice(name, amount));
        } else {
            other += amount;
        }
    }
    if other > Decimal::ZERO {
        slices.push(Slice {
            name: "Other".to_string(),
            amount: other,
            color: COLOR_GREY.to_string(),
        });
    }
    slices
}

/// R-CHT-7: spending F⁻ᵗ in the window, grouped by category name (not parent), largest first,
/// at most 15 rows.
pub fn expense_distribution(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<Slice> {
    let rows = spending_in_window(db, f, period);
    sum_abs_by(&rows, |tx| category_name_or_uncategorized(db, tx))
        .into_iter()
        .take(MAX_DISTRIBUTION_ROWS)
        .map(|(name, amount)| slice(name, amount))
        .collect()
}

/// R-CHT-7: card height for `rows` bars, 34 px each plus 40 px.
pub fn distribution_height(rows: usize) -> f32 {
    34.0 * (rows as f32) + 40.0
}

/// A slice coloured by [`category_hex`].
fn slice(name: String, amount: Decimal) -> Slice {
    let color = category_hex(&name);
    Slice {
        name,
        amount,
        color,
    }
}

/// Spending (F⁻ᵗ with `amount < 0`) dated in `[start or −∞, today]` for the period.
fn spending_in_window<'a>(db: &'a Db, f: &ChartFilter, period: TimePeriod) -> Vec<&'a Transaction> {
    let start = period_start(period, f.today);
    filtered_transactions(db, f, false)
        .into_iter()
        .filter(|tx| tx.amount < Decimal::ZERO)
        .filter(|tx| tx.date <= f.today && start.is_none_or(|s| tx.date >= s))
        .collect()
}

#[cfg(test)]
mod tests {
    use pf_model::testing::{TestDb, m};
    use pf_model::{AccountType, CategoryType};

    use super::*;
    use crate::charts::fixtures::{small_db, small_filter};

    #[test]
    /// Covers: R-CHT-5
    fn r_cht_5_breakdown_groups_top6_other() {
        let db = small_db();
        let f = small_filter(&db);
        let slices = category_breakdown(&db, &f, TimePeriod::PastMonth);
        let got: Vec<(&str, Decimal)> =
            slices.iter().map(|s| (s.name.as_str(), s.amount)).collect();
        assert_eq!(
            got,
            vec![
                ("Housing", m("1708.55")),
                ("Uncategorized", m("450")),
                ("Groceries", m("142.30")),
                ("Dining", m("6.75")),
            ]
        );
        assert_eq!(slices[0].color, category_hex("Housing"));

        // Eight groups: six kept, the two smallest summed into Other.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let amounts = ["100", "90", "80", "70", "60", "50", "40", "30"];
        for (i, amt) in amounts.iter().enumerate() {
            let cat = t.category(&format!("Cat{}", i + 1), CategoryType::Expense);
            t.tx(chk, "2026-03-05", &format!("-{amt}"), "Shop")
                .category(cat)
                .done();
        }
        let synthetic = t.build();
        let f2 = ChartFilter {
            accounts: [crate::charts::fixtures::account_id(&synthetic, "Checking")]
                .into_iter()
                .collect(),
            scenario: None,
            today: crate::charts::fixtures::today(),
        };
        let slices = category_breakdown(&synthetic, &f2, TimePeriod::PastMonth);
        assert_eq!(slices.len(), 7);
        assert_eq!(slices[0].name, "Cat1");
        assert_eq!(slices[5].name, "Cat6");
        assert_eq!(slices[6].name, "Other");
        assert_eq!(slices[6].amount, m("70"));
        assert_eq!(slices[6].color, COLOR_GREY);
        let rest: Decimal = slices[..6].iter().map(|s| s.amount).sum();
        assert_eq!(rest, m("450"));
    }

    #[test]
    /// Covers: R-CHT-7
    fn r_cht_7_distribution_by_category_name() {
        let db = small_db();
        let f = small_filter(&db);
        let rows = expense_distribution(&db, &f, TimePeriod::PastYear);
        let got: Vec<(&str, Decimal)> = rows.iter().map(|s| (s.name.as_str(), s.amount)).collect();
        assert_eq!(
            got,
            vec![
                ("Housing", m("3064.99")),
                ("Uncategorized", m("450")),
                ("Utilities", m("208.55")),
                ("Groceries", m("142.30")),
                ("Dining", m("94.75")),
            ]
        );
        assert_eq!(distribution_height(5), 210.0);

        // Sixteen categories: only the top 15 are kept.
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        for i in 0..16 {
            let cat = t.category(&format!("Cat{i:02}"), CategoryType::Expense);
            t.tx(chk, "2026-03-05", &format!("-{}", 100 - i), "Shop")
                .category(cat)
                .done();
        }
        let synthetic = t.build();
        let f2 = ChartFilter {
            accounts: [crate::charts::fixtures::account_id(&synthetic, "Checking")]
                .into_iter()
                .collect(),
            scenario: None,
            today: crate::charts::fixtures::today(),
        };
        let capped = expense_distribution(&synthetic, &f2, TimePeriod::All);
        assert_eq!(capped.len(), 15);
        assert_eq!(capped[0].name, "Cat00");
        assert_eq!(capped[14].name, "Cat14");
    }
}
