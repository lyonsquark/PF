//! Chart data for the nine Charts-screen cards (R-CHT-0 … R-CHT-10). Pure functions over `Db`
//! that return typed series holding `Decimal` values; `pf-ui` converts to `f64` only for drawing.

pub mod balance;
pub mod categories;
pub mod monthly;
pub mod sankey;
pub mod spending;
pub mod waterfall;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod golden;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;
use pf_model::{AccountId, Category, Db, ScenarioId, TimePeriod, Transaction};
use rust_decimal::Decimal;

use crate::balance::is_visible;
use crate::filter::period_start;
use crate::reports::{is_transfer, live_category};

/// Income green used for income bars and lines (`#34c759`).
pub const COLOR_INCOME: &str = "#34c759";
/// Expense red (`#ff3b30`).
pub const COLOR_EXPENSE: &str = "#ff3b30";
/// Accent blue for lines and savings (`#007aff`).
pub const COLOR_BLUE: &str = "#007aff";
/// Neutral grey for "Other", "Uncategorized" and dashed budget lines (`#8e8e93`).
pub const COLOR_GREY: &str = "#8e8e93";
/// Savings node in the money-flow chart (`#30d158`).
pub const COLOR_SAVINGS: &str = "#30d158";

/// Card titles in display order (R-CHT-0c).
pub const CHART_TITLES: [&str; 9] = [
    "Account Balances Over Time",
    "Cumulative Spending (Month vs Month)",
    "Budget Pacing vs Actuals",
    "Profit & Loss (Cashflow)",
    "Categorical Breakdown",
    "Monthly Cashflow Waterfall",
    "Expense Distribution",
    "Income vs Expense Trend",
    "Money Flow",
];
/// R-CHT-0c: shown in a card with no data.
pub const NOT_ENOUGH_DATA: &str = "Not enough data";
/// R-CHT-0c: budget pacing card with no budget in the month.
pub const NO_BUDGETS: &str = "No budgets set for this month.";

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTH_SHORT_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The inputs every chart shares (R-CHT-0).
#[derive(Debug, Clone)]
pub struct ChartFilter {
    /// Selected accounts; a transaction with no account (orphan) is never included.
    pub accounts: BTreeSet<AccountId>,
    /// Active scenario, `None` for Reality (R-SCN-1).
    pub scenario: Option<ScenarioId>,
    /// The "today" used for windows and for excluding future rows. Never read from the clock.
    pub today: Date,
}

/// A labelled amount with a colour: donut slices and distribution bars.
#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    /// Group name.
    pub name: String,
    /// Positive magnitude.
    pub amount: Decimal,
    /// `#rrggbb` colour (R-CHT-10).
    pub color: String,
}

/// R-CHT-0: live transactions of the selected accounts, visible under the active scenario
/// (R-SCN-1). `transfers: true` gives F; `transfers: false` gives F⁻ᵗ (Transfer-category rows
/// dropped). Sorted by (date, id) ascending.
pub fn filtered_transactions<'a>(
    db: &'a Db,
    f: &ChartFilter,
    transfers: bool,
) -> Vec<&'a Transaction> {
    let mut v: Vec<&Transaction> = db
        .live_transactions()
        .filter(|tx| tx.account_id.is_some_and(|a| f.accounts.contains(&a)))
        .filter(|tx| is_visible(tx, f.scenario))
        .filter(|tx| transfers || !is_transfer(db, tx.category_id))
        .collect();
    v.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    v
}

/// R-CHT-0: the transactions of F⁻ᵗ dated in the calendar month of `base`, both signs.
pub(crate) fn month_txs<'a>(db: &'a Db, f: &ChartFilter, base: Date) -> Vec<&'a Transaction> {
    let (first, last) = month_bounds(base);
    filtered_transactions(db, f, false)
        .into_iter()
        .filter(|tx| first <= tx.date && tx.date <= last)
        .collect()
}

/// R-CHT-0a: the account selection the first time the Charts screen opens: live, non-external.
pub fn default_selection(db: &Db) -> BTreeSet<AccountId> {
    db.live_accounts()
        .filter(|a| !a.is_external)
        .map(|a| a.id)
        .collect()
}

/// R-CHT-0a: true when every live account is selected (vacuously true with none).
fn all_selected(db: &Db, selected: &BTreeSet<AccountId>) -> bool {
    db.live_accounts().all(|a| selected.contains(&a.id))
}

/// R-CHT-0a: selector label: all ⇒ "All Accounts"; none ⇒ "No Accounts"; one ⇒ its name; else "{n} Accounts".
pub fn selection_label(db: &Db, selected: &BTreeSet<AccountId>) -> String {
    if all_selected(db, selected) {
        return "All Accounts".to_string();
    }
    if selected.is_empty() {
        return "No Accounts".to_string();
    }
    if selected.len() == 1 {
        return match db.live_accounts().find(|a| selected.contains(&a.id)) {
            Some(acc) => acc.name.clone(),
            None => "1 Account".to_string(),
        };
    }
    format!("{} Accounts", selected.len())
}

/// R-CHT-0a: "Deselect All" when every live account is selected, else "Select All".
pub fn select_all_button_label(db: &Db, selected: &BTreeSet<AccountId>) -> &'static str {
    if all_selected(db, selected) {
        "Deselect All"
    } else {
        "Select All"
    }
}

/// R-CHT-0b: first and last day of the month containing `d`.
pub fn month_bounds(d: Date) -> (Date, Date) {
    (d.first_of_month(), d.last_of_month())
}

/// R-CHT-0b: "MMM yy" for a year and month, e.g. "Mar 26" (English, no locale).
pub(crate) fn month_label_short_ym(year: i16, month: i8) -> String {
    let name = MONTH_SHORT_NAMES[usize::from(month_index(month))];
    format!("{} {:02}", name, year.rem_euclid(100))
}

/// R-CHT-0b: "MMM yy", e.g. "Mar 26".
pub fn month_label_short(d: Date) -> String {
    month_label_short_ym(d.year(), d.month())
}

/// R-CHT-0b: "MMMM yyyy", e.g. "March 2026".
pub fn month_label_long(d: Date) -> String {
    format!(
        "{} {}",
        MONTH_NAMES[usize::from(month_index(d.month()))],
        d.year()
    )
}

/// Zero-based index of a month number (1..=12). Out-of-range input is clamped to January.
fn month_index(month: i8) -> u8 {
    u8::try_from(month.clamp(1, 12) - 1).unwrap_or(0)
}

/// R-CHT-0b: a period window is `[start or −∞, today]`; the start comes from R-FLT-1.
pub fn period_window(period: TimePeriod, today: Date) -> (Option<Date>, Date) {
    (period_start(period, today), today)
}

/// R-CHT-0b: true when both dates fall in the same calendar month.
pub(crate) fn same_month(a: Date, b: Date) -> bool {
    a.year() == b.year() && a.month() == b.month()
}

/// R-CHT-5 grouping: the parent's name if the live category has a live parent, else the
/// category's name, else "Uncategorized" (R-RPT-0).
pub fn group_name(db: &Db, tx: &Transaction) -> String {
    let Some(cat) = live_category(db, tx.category_id) else {
        return "Uncategorized".to_string();
    };
    match live_category(db, cat.parent_id) {
        Some(parent) => parent.name.clone(),
        None => cat.name.clone(),
    }
}

/// R-CHT-7 grouping: the live category's name, else "Uncategorized".
pub fn category_name_or_uncategorized(db: &Db, tx: &Transaction) -> String {
    live_category(db, tx.category_id)
        .map_or_else(|| "Uncategorized".to_string(), |c| c.name.clone())
}

/// R-CHT-9: the live subcategory of a transaction (its category, when the category and its
/// parent are both live). `None` for top-level categories and uncategorized rows.
pub(crate) fn live_subcategory<'a>(db: &'a Db, tx: &Transaction) -> Option<&'a Category> {
    let cat = live_category(db, tx.category_id)?;
    live_category(db, cat.parent_id)?;
    Some(cat)
}

/// R-CHT-10: stable `#rrggbb` colour for a category name. "Other" and "Uncategorized" are grey.
/// Otherwise a Java-style hash of the UTF-8 bytes picks the hue, and HSB(hue, 0.7, 0.9) gives the RGB.
pub fn category_hex(name: &str) -> String {
    if name == "Other" || name == "Uncategorized" {
        return COLOR_GREY.to_string();
    }
    let mut h: i64 = 0;
    for b in name.bytes() {
        h = i64::from(b).wrapping_add(h.wrapping_shl(5)).wrapping_sub(h);
    }
    let hue = (h.unsigned_abs() % 360) as f64 / 360.0;
    let (r, g, b) = hsb_to_rgb(hue, 0.7, 0.9);
    format!(
        "#{:02x}{:02x}{:02x}",
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8
    )
}

/// R-CHT-10: the six-sector HSB→RGB conversion from `WebChart.swift` (channels in 0..=1).
fn hsb_to_rgb(hue: f64, s: f64, b: f64) -> (f64, f64, f64) {
    let h6 = hue * 6.0;
    let i = (h6 as i64) % 6;
    let f = h6 - (h6 as i64) as f64;
    let p = b * (1.0 - s);
    let q = b * (1.0 - s * f);
    let t = b * (1.0 - s * (1.0 - f));
    match i {
        0 => (b, t, p),
        1 => (q, b, p),
        2 => (p, b, t),
        3 => (p, q, b),
        4 => (t, p, b),
        _ => (b, p, q),
    }
}

/// Sorts a name → amount map by amount descending, then name ascending (ties).
pub(crate) fn sorted_by_amount(map: BTreeMap<String, Decimal>) -> Vec<(String, Decimal)> {
    let mut v: Vec<(String, Decimal)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// Sums `|amount|` per key, sorted by [`sorted_by_amount`].
pub(crate) fn sum_abs_by(
    txs: &[&Transaction],
    key: impl Fn(&Transaction) -> String,
) -> Vec<(String, Decimal)> {
    let mut map: BTreeMap<String, Decimal> = BTreeMap::new();
    for tx in txs {
        *map.entry(key(tx)).or_default() += tx.amount.abs();
    }
    sorted_by_amount(map)
}

/// Transactions of `txs` dated within `[first, last]` (both inclusive).
pub(crate) fn within<'a>(txs: &[&'a Transaction], first: Date, last: Date) -> Vec<&'a Transaction> {
    txs.iter()
        .copied()
        .filter(|tx| first <= tx.date && tx.date <= last)
        .collect()
}
