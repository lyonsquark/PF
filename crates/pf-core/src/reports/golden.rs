//! Golden snapshots of the three reports for `small.md` and `SampleData2.md`. The snapshot
//! files live in `reports/snapshots/` and were reviewed by hand before they were accepted.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use pf_model::Db;
use pf_model::testing::d;
use rust_decimal::Decimal;

use super::{
    BalanceSheet, MONTH_SHORT, MonthlyMatrix, PlRow, balance_sheet, monthly_matrix, profit_and_loss,
};
use crate::markdown::import_markdown;

#[cfg(test)]
/// Imports a file from the repo-level `fixtures/` directory with the `America/Chicago` zone.
pub(super) fn fixture_db(name: &str) -> Db {
    let path = format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("fixture exists");
    let tz = TimeZone::get("America/Chicago").expect("tzdb");
    let now: Timestamp = "2026-03-15T12:00:00Z".parse().expect("timestamp");
    import_markdown(&text, &tz, now, &mut |_| {}).db
}

/// Fixed-width text of a P&L report.
fn render_pnl(rows: &[PlRow]) -> String {
    let mut out = format!(
        "{:<24}{:>14}{:>14}{:>14}\n",
        "Category", "Budgeted", "Actual", "Difference"
    );
    for r in rows {
        out.push_str(&format!(
            "{:<24}{:>14}{:>14}{:>14}\n",
            r.label,
            r.budgeted.to_string(),
            r.actual.to_string(),
            r.difference().to_string()
        ));
    }
    out
}

/// Fixed-width text of a balance sheet.
fn render_balance(bs: &BalanceSheet) -> String {
    let mut out = format!("As of {}\n", bs.as_of);
    out.push_str(&format!(
        "{:<32}{:>14}{:>14}\n",
        "Account", "Balance", "Pending"
    ));
    for a in &bs.accounts {
        let name = if a.institution.is_empty() {
            a.name.clone()
        } else {
            format!("{} ({})", a.name, a.institution)
        };
        out.push_str(&format!(
            "{:<32}{:>14}{:>14}\n",
            name,
            a.total.to_string(),
            a.pending.to_string()
        ));
    }
    out.push_str(&format!(
        "{:<32}{:>14}{:>14}\n",
        "Total",
        bs.total.to_string(),
        bs.pending_total.to_string()
    ));
    if !bs.obligations.is_empty() {
        out.push('\n');
        out.push_str(&format!("{:<32}{:>14}\n", "Obligation", "Balance"));
        for o in &bs.obligations {
            let status = if o.is_funding { "Funding" } else { "Spending" };
            out.push_str(&format!(
                "{:<32}{:>14}\n",
                format!("{} ({status})", o.name),
                o.balance.to_string()
            ));
        }
        out.push_str(&format!(
            "{:<32}{:>14}\n",
            "Total Obligations",
            bs.total_obligations.to_string()
        ));
    }
    out.push('\n');
    out.push_str(&format!(
        "{:<32}{:>14}\n",
        "Net Worth",
        bs.net_worth.to_string()
    ));
    out.push_str(&format!(
        "{:<32}{:>14}\n",
        "Available",
        bs.available.to_string()
    ));
    out
}

/// One line of a month grid: label, twelve months, then the row total.
fn grid_line(out: &mut String, label: &str, months: &[Decimal; 12]) {
    out.push_str(&format!("{label:<24}"));
    for v in months {
        out.push_str(&format!("{:>10}", v.to_string()));
    }
    let total: Decimal = months.iter().sum();
    out.push_str(&format!("{:>10}\n", total.to_string()));
}

/// One grid (actual or budget): header, category rows, then NET.
fn render_grid(out: &mut String, title: &str, rows: &[(&str, [Decimal; 12])], net: &[Decimal; 12]) {
    out.push_str(&format!("{title}\n"));
    out.push_str(&format!("{:<24}", "Category"));
    for m in MONTH_SHORT {
        out.push_str(&format!("{m:>10}"));
    }
    out.push_str(&format!("{:>10}\n", "Total"));
    for (label, months) in rows {
        grid_line(out, label, months);
    }
    grid_line(out, "NET", net);
}

/// Fixed-width text of the monthly matrix: an actual grid, then a budget grid.
fn render_matrix(m: &MonthlyMatrix) -> String {
    let mut out = format!("Monthly Matrix {}\n\n", m.year);
    let actual: Vec<(&str, [Decimal; 12])> = m
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.actual))
        .collect();
    render_grid(&mut out, "Actual", &actual, &m.net_actual);
    out.push('\n');
    let budget: Vec<(&str, [Decimal; 12])> = m
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.budget))
        .collect();
    render_grid(&mut out, "Budget", &budget, &m.net_budget);
    out
}

/// Covers: R-RPT-1..5 (small.md: P&L Q1, balance sheet 2026-03-15, matrix 2026)
#[test]
fn golden_reports_small() {
    let db = fixture_db("small.md");
    let pnl = profit_and_loss(&db, d("2026-01-01"), d("2026-03-31"), None);
    insta::assert_snapshot!("golden_reports_small_pnl", render_pnl(&pnl));
    let bs = balance_sheet(&db, d("2026-03-15"), None);
    insta::assert_snapshot!("golden_reports_small_balance_sheet", render_balance(&bs));
    let matrix = monthly_matrix(&db, 2026, None);
    insta::assert_snapshot!("golden_reports_small_matrix", render_matrix(&matrix));
}

/// Covers: R-RPT-1..5 (SampleData2.md: P&L 2025, balance sheet 2025-12-31, matrix 2025)
#[test]
fn golden_reports_sampledata2() {
    let db = fixture_db("SampleData2.md");
    let pnl = profit_and_loss(&db, d("2025-01-01"), d("2025-12-31"), None);
    insta::assert_snapshot!("golden_reports_sampledata2_pnl", render_pnl(&pnl));
    let bs = balance_sheet(&db, d("2025-12-31"), None);
    insta::assert_snapshot!(
        "golden_reports_sampledata2_balance_sheet",
        render_balance(&bs)
    );
    let matrix = monthly_matrix(&db, 2025, None);
    insta::assert_snapshot!("golden_reports_sampledata2_matrix", render_matrix(&matrix));
}
