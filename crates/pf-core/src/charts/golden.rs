//! Golden snapshot of every chart's data for `fixtures/small.md`. The snapshot file lives in
//! `charts/snapshots/` and was reviewed by hand before it was accepted.

use pf_model::TimePeriod;
use pf_model::testing::d;

use super::balance::balance_over_time;
use super::categories::{category_breakdown, expense_distribution};
use super::fixtures::{small_db, small_filter};
use super::monthly::{income_expense_trend, profit_loss_by_month};
use super::sankey::sankey;
use super::spending::{budget_pacing, spending_comparison};
use super::waterfall::waterfall;

#[test]
/// Covers: R-CHT-1, R-CHT-2, R-CHT-3, R-CHT-4, R-CHT-5, R-CHT-6, R-CHT-7, R-CHT-8, R-CHT-9
fn golden_charts_small() {
    let db = small_db();
    let f = small_filter(&db);
    let today = d("2026-03-15");
    let mut out = String::new();
    let mut section = |title: &str, body: String| {
        out.push_str(&format!("== {title} ==\n{body}\n"));
    };
    section(
        "balance 1Y",
        format!("{:#?}", balance_over_time(&db, &f, TimePeriod::PastYear)),
    );
    section(
        "spending Mar vs Feb",
        format!(
            "{:#?}",
            spending_comparison(&db, &f, d("2026-03-15"), d("2026-02-15"))
        ),
    );
    section(
        "budget pacing Mar",
        format!("{:#?}", budget_pacing(&db, &f, today)),
    );
    section(
        "profit and loss 6M",
        format!("{:#?}", profit_loss_by_month(&db, &f, 6)),
    );
    section(
        "breakdown 1M",
        format!("{:#?}", category_breakdown(&db, &f, TimePeriod::PastMonth)),
    );
    section(
        "waterfall Feb",
        format!("{:#?}", waterfall(&db, &f, d("2026-02-15"))),
    );
    section("waterfall Mar", format!("{:#?}", waterfall(&db, &f, today)));
    section(
        "distribution 1Y",
        format!("{:#?}", expense_distribution(&db, &f, TimePeriod::PastYear)),
    );
    section(
        "trend 1Y",
        format!("{:#?}", income_expense_trend(&db, &f, TimePeriod::PastYear)),
    );
    section(
        "sankey Feb",
        format!("{:#?}", sankey(&db, &f, d("2026-02-15"))),
    );
    section("sankey Mar", format!("{:#?}", sankey(&db, &f, today)));
    insta::assert_snapshot!(out);
}
