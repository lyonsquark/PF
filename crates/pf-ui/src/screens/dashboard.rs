//! Dashboard screen (Swift `DashboardView`, T14): four summary cards, the "Show Available Balance"
//! toggle and the account groups with Cleared / All columns. Pure data comes from `build_data`,
//! which is cached per engine revision so the home screen stays instant with 100k transactions.

use std::collections::HashMap;

use egui::{Align, Button, Color32, Frame, Layout, RichText, ScrollArea, Ui, vec2};
use egui_extras::{Column, TableBuilder};
use pf_core::{balance, format};
use pf_model::{AccountId, Db, ScenarioId};
use rust_decimal::Decimal;

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::{ConfirmAction, Dialog};
use crate::screen::Screen;
use crate::widgets::col_fit::fit;
use crate::widgets::colors;

/// Per-screen UI state. `show_available` is UI-only and not persisted.
#[derive(Default)]
pub struct State {
    /// The "Show Available Balance (deduct obligations)" toggle (R-BAL-6).
    pub show_available: bool,
    data: Cached<(Option<ScenarioId>, bool), DashboardData>,
}

/// Everything the screen draws, computed in ONE pass (cache it).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DashboardData {
    /// R-BAL-5: non-external; cleared in Reality, all visible when a scenario is active; adjusted by
    /// R-BAL-6 when the toggle is on.
    pub net_worth: Decimal,
    /// R-BAL-5: checking, savings, cash and investment.
    pub assets: Decimal,
    /// R-BAL-5: credit accounts (a negative number when owed).
    pub credit_debt: Decimal,
    /// R-BAL-5: loan accounts.
    pub loan_debt: Decimal,
    /// R-BAL-9 order; empty groups omitted.
    pub groups: Vec<AccountGroup>,
}

/// One titled group of account rows on the dashboard (R-BAL-9).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountGroup {
    /// Group heading, e.g. "Cash & Checking".
    pub title: &'static str,
    /// Accounts sorted by name.
    pub rows: Vec<AccountRow>,
}

/// One account line on the dashboard.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountRow {
    /// Account id; a click opens its detail screen.
    pub id: AccountId,
    /// Display name.
    pub name: String,
    /// External accounts are shown with a text marker and excluded from the cards.
    pub is_external: bool,
    /// Cleared balance under the active scenario, minus reserved if the toggle is on (R-BAL-6).
    pub cleared: Decimal,
    /// All-status balance under the active scenario, minus reserved if the toggle is on (R-BAL-6).
    pub all: Decimal,
    /// Reality cleared balance (R-BAL-3). Never adjusted.
    pub reality_cleared: Decimal,
    /// Reality all-status balance (R-BAL-3). Never adjusted.
    pub reality_all: Decimal,
}

/// The widest the dashboard gets, in points; a wider window keeps it in the middle.
const MAX_WIDTH: f32 = 880.0;
/// Height of an account row, in points.
const ROW_HEIGHT: f32 = 36.0;
/// Height of an account row when a scenario shows its reality line, in points.
const ROW_HEIGHT_REALITY: f32 = 50.0;
/// Height of the Cleared / All header row, in points.
const HEADER_HEIGHT: f32 = 24.0;

/// Pure: builds `DashboardData` from the `Db` (unit-testable without egui). Balances come from
/// pf-core in one pass over live transactions (R-BAL-1..3); the reserved amounts of R-BAL-6 are
/// subtracted only when `show_available` is on.
pub fn build_data(db: &Db, active: Option<ScenarioId>, show_available: bool) -> DashboardData {
    let maps = balance::balance_maps(db, active);
    let (cleared, all) = balance::displayed_balances(db, &maps, show_available);
    // R-BAL-5: the cards add up the cleared balances in Reality, and all visible balances (the scenario's
    // pending and future rows too) when a scenario is active. External accounts are excluded.
    let card_basis = if active.is_some() { &all } else { &cleared };
    let totals = balance::dashboard_totals(db, card_basis);
    // R-BAL-9: group order, empty groups omitted, rows sorted by name.
    let groups = balance::dashboard_groups(db)
        .into_iter()
        .map(|group| AccountGroup {
            title: group.title,
            rows: group
                .accounts
                .iter()
                .filter_map(|&id| account_row(db, id, &cleared, &all, &maps))
                .collect(),
        })
        .collect();
    DashboardData {
        net_worth: totals.net_worth,
        assets: totals.assets,
        credit_debt: totals.credit_debt,
        loan_debt: totals.loan_debt,
        groups,
    }
}

/// One dashboard row for a live account. `None` if the account is missing.
fn account_row(
    db: &Db,
    id: AccountId,
    cleared: &HashMap<AccountId, Decimal>,
    all: &HashMap<AccountId, Decimal>,
    maps: &balance::BalanceMaps,
) -> Option<AccountRow> {
    let account = db.account(id)?;
    let value = |map: &HashMap<AccountId, Decimal>| map.get(&id).copied().unwrap_or(Decimal::ZERO);
    Some(AccountRow {
        id,
        name: account.name.clone(),
        is_external: account.is_external,
        cleared: value(cleared),
        all: value(all),
        reality_cleared: value(&maps.reality_cleared),
        reality_all: value(&maps.reality_all),
    })
}

/// Draws the dashboard into the central panel. The data is cached per (revision, scenario,
/// toggle), so frames that change nothing do no balance work.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let active = cx.engine.state.active_scenario;
    let show_available = st.show_available;
    let revision = cx.engine.revision();
    let data = st.data.get(revision, &(active, show_available), || {
        build_data(cx.engine.db(), active, show_available)
    });
    let reality_lines = active.is_some();
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(24.0);
            // The content is one column of at most MAX_WIDTH, centred in the window.
            ui.horizontal(|ui| {
                let margin = ((ui.available_width() - MAX_WIDTH) / 2.0).max(0.0);
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width().min(MAX_WIDTH));
                    ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
                    ui.columns_const(|[left, right]| {
                        card(left, "NET WORTH", data.net_worth, colors::GREEN);
                        card(right, "ASSETS", data.assets, colors::BLUE);
                    });
                    ui.columns_const(|[left, right]| {
                        card(left, "CREDIT DEBT", data.credit_debt, colors::RED);
                        card(right, "LOAN DEBT", data.loan_debt, colors::ORANGE);
                    });
                    ui.label(
                        RichText::new(card_explanation(reality_lines))
                            .small()
                            .weak(),
                    );
                    Frame::group(ui.style())
                        .inner_margin(12.0)
                        .corner_radius(10.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let toggle = ui.checkbox(
                                &mut st.show_available,
                                "Show Available Balance (deduct obligations)",
                            );
                            if toggle.changed() {
                                // The cached data above is for the old toggle value; draw again at once.
                                ui.ctx().request_repaint();
                            }
                        });
                    for group in &data.groups {
                        group_section(cx, ui, group, reality_lines);
                    }
                    ui.add_space(24.0);
                });
            });
        });
}

/// The line under the summary cards: which transactions they add up.
pub(crate) fn card_explanation(scenario_active: bool) -> &'static str {
    if scenario_active {
        "The cards show all transactions in the scenario (pending and future included), excluding external accounts."
    } else {
        "The cards show cleared transactions only, excluding external accounts."
    }
}

/// One summary card: a small uppercase title and a large coloured amount (R-BAL-5).
fn card(ui: &mut Ui, title: &str, value: Decimal, color: Color32) {
    Frame::group(ui.style())
        .inner_margin(16.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(12.0).weak());
            ui.add_space(4.0);
            ui.label(
                RichText::new(format::format_currency(value))
                    .size(30.0)
                    .strong()
                    .color(color),
            );
        });
}

/// One account group: its title, then a table with a Cleared and an All column (R-BAL-9). The name
/// column takes the rest of the width, so the header and every row share the same columns.
fn group_section(cx: &mut UiCtx, ui: &mut Ui, group: &AccountGroup, reality_lines: bool) {
    // Each group gets its own ID scope: its table and rows would otherwise share IDs with the
    // other groups' (egui reports that as a duplicate ID in debug builds).
    ui.push_id(group.title, |ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(group.title).size(16.0).strong());
        ui.add_space(4.0);
        let row_height = if reality_lines {
            ROW_HEIGHT_REALITY
        } else {
            ROW_HEIGHT
        };
        group_table(cx, ui, group, row_height, reality_lines);
    });
}

/// The framed table of one group's accounts.
fn group_table(
    cx: &mut UiCtx,
    ui: &mut Ui,
    group: &AccountGroup,
    row_height: f32,
    reality_lines: bool,
) {
    // Each Cleared / All column is as wide as its widest figure (see `widgets::col_fit`).
    let amount_w = fit(ui, "Cleared", &["-$999,999.99"]).max(fit(ui, "All", &["-$999,999.99"]));
    Frame::group(ui.style())
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            TableBuilder::new(ui)
                .striped(true)
                .auto_shrink([false, true])
                .column(Column::remainder().at_least(160.0))
                .column(Column::exact(amount_w))
                .column(Column::exact(amount_w))
                .header(HEADER_HEIGHT, |mut header| {
                    header.col(|_| {});
                    header.col(|ui| amount_header(ui, "Cleared"));
                    header.col(|ui| amount_header(ui, "All"));
                })
                .body(|body| {
                    body.rows(row_height, group.rows.len(), |mut row| {
                        let account = &group.rows[row.index()];
                        row.col(|ui| account_name(cx, ui, account));
                        row.col(|ui| {
                            amount_cell(
                                ui,
                                account.cleared,
                                account.reality_cleared,
                                reality_lines,
                                false,
                            );
                        });
                        row.col(|ui| {
                            let differs = account.all != account.cleared;
                            amount_cell(
                                ui,
                                account.all,
                                account.reality_all,
                                reality_lines,
                                differs,
                            );
                        });
                    });
                });
        });
}

/// A Cleared / All column heading, right-aligned like its figures.
fn amount_header(ui: &mut Ui, text: &str) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(RichText::new(text).size(13.0).weak());
    });
}

/// The account's name, a clickable button, with an EXTERNAL badge for external accounts. Click opens
/// the account (UI-NAV-02); the context menu offers Delete with the confirmation of R-ACC-1.
fn account_name(cx: &mut UiCtx, ui: &mut Ui, row: &AccountRow) {
    ui.horizontal(|ui| {
        let name = ui.add(Button::new(RichText::new(&row.name).size(15.0)).frame(false));
        if name.clicked() {
            *cx.screen = Screen::Account(row.id);
        }
        name.context_menu(|ui| {
            if ui.button("Delete").clicked() {
                cx.dialogs.open(Dialog::Confirm {
                    title: "Delete Account".to_string(),
                    message: format!(
                        "Are you sure you want to delete '{}'? This action cannot be undone and will delete all associated transactions.",
                        row.name
                    ),
                    confirm_label: "Delete".to_string(),
                    action: ConfirmAction::DeleteAccount(row.id),
                });
                ui.close();
            }
        });
        if row.is_external {
            external_badge(ui);
        }
    });
}

/// A small grey badge marking an external account. Its figures stay out of the cards.
fn external_badge(ui: &mut Ui) {
    Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(4.0)
        .inner_margin(vec2(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new("EXTERNAL").size(10.0).weak());
        })
        .response
        .on_hover_text("External account: left out of the summary cards.");
}

/// A right-aligned amount with an optional reality line below it (R-BAL-3). `orange` marks an
/// All value that differs from Cleared.
fn amount_cell(ui: &mut Ui, value: Decimal, reality: Decimal, reality_line: bool, orange: bool) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            let text = RichText::new(format::format_currency(value))
                .size(15.0)
                .strong();
            let text = if orange {
                text.color(colors::ORANGE)
            } else {
                text
            };
            ui.label(text);
            if reality_line {
                ui.label(
                    RichText::new(format::format_currency(reality))
                        .size(12.0)
                        .weak(),
                );
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, m};
    use pf_model::{AccountType, ObligationStatus, RowKey, TransactionStatus};

    /// The dashboard row named `name`. Panics when missing (test helper).
    fn row<'a>(d: &'a DashboardData, name: &str) -> &'a AccountRow {
        d.groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .find(|r| r.name == name)
            .expect("row present")
    }

    #[test]
    /// Covers: R-BAL-5. With a scenario active the cards add up all visible rows, pending and future
    /// included; in Reality they add up cleared rows only.
    fn r_bal_5_cards_use_all_rows_under_a_scenario() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let plan = t.scenario("Plan");
        t.tx(chk, "2026-01-05", "100", "Cleared")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(chk, "2026-01-06", "50", "Pending")
            .status(TransactionStatus::Pending)
            .done();
        t.tx(chk, "2026-04-06", "-30", "Future plan")
            .status(TransactionStatus::Future)
            .scenario(plan)
            .done();
        let db = t.build();
        assert_eq!(build_data(&db, None, false).net_worth, m("100"));
        assert_eq!(build_data(&db, Some(plan), false).net_worth, m("120"));
    }

    #[test]
    /// Covers: R-BAL-5. Pending rows are excluded; external accounts are excluded from all four
    /// cards; assets are checking + savings + cash + investment.
    fn r_bal_5_summary_cards_use_cleared_non_external() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let sav = t.account("Savings", AccountType::Savings);
        let cash = t.account("Wallet", AccountType::Cash);
        let inv = t.account("Brokerage", AccountType::Investment);
        let visa = t.account("Visa", AccountType::Credit);
        let loan = t.account("Car Loan", AccountType::Loan);
        let joint = t.external_account("Joint", AccountType::Checking);
        t.tx(chk, "2026-01-05", "1000", "Pay")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(chk, "2026-01-06", "200", "Pending")
            .status(TransactionStatus::Pending)
            .done();
        t.tx(sav, "2026-01-05", "500", "Save")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(cash, "2026-01-05", "50", "Cash")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(inv, "2026-01-05", "2000", "Buy")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(visa, "2026-01-05", "-1200", "Spend")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(loan, "2026-01-05", "-150000", "Loan")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(joint, "2026-01-05", "50000", "Joint")
            .status(TransactionStatus::Cleared)
            .done();
        let db = t.build();
        let d = build_data(&db, None, false);
        assert_eq!(d.assets, m("3550"));
        assert_eq!(d.credit_debt, m("-1200"));
        assert_eq!(d.loan_debt, m("-150000"));
        assert_eq!(d.net_worth, m("-147650"));
    }

    #[test]
    /// Covers: R-BAL-9. Only non-empty groups, in the fixed order; cash goes in "Cash & Checking";
    /// rows sorted by name.
    fn r_bal_9_groups_order_and_empty_hidden() {
        let mut t = TestDb::new();
        t.account("Brokerage", AccountType::Investment);
        t.account("Zeta", AccountType::Checking);
        t.account("Alpha", AccountType::Checking);
        t.account("Wallet", AccountType::Cash);
        t.account("Visa", AccountType::Credit);
        t.account("Savings", AccountType::Savings);
        let db = t.build();
        let d = build_data(&db, None, false);
        let titles: Vec<&str> = d.groups.iter().map(|g| g.title).collect();
        assert_eq!(
            titles,
            vec!["Cash & Checking", "Savings", "Credit Cards", "Investments"]
        );
        let cash_names: Vec<&str> = d.groups[0].rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(cash_names, vec!["Alpha", "Wallet", "Zeta"]);
    }

    #[test]
    /// Covers: R-BAL-3. With an active scenario, `all` changes but `reality_all` does not.
    fn r_bal_3_reality_columns_ignore_scenario() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        t.tx(chk, "2026-01-05", "100", "Real")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(chk, "2026-01-06", "-40", "Plan")
            .status(TransactionStatus::Cleared)
            .scenario(s)
            .done();
        let db = t.build();
        let plan = build_data(&db, Some(s), false);
        let r = row(&plan, "Checking");
        assert_eq!(r.all, m("60"));
        assert_eq!(r.cleared, m("60"));
        assert_eq!(r.reality_all, m("100"));
        assert_eq!(r.reality_cleared, m("100"));

        let reality = build_data(&db, None, false);
        let r = row(&reality, "Checking");
        assert_eq!(r.all, m("100"));
        assert_eq!(r.reality_all, m("100"));
    }

    #[test]
    /// Covers: R-BAL-6, R-OBL-2. A Funding obligation of 300 lowers cleared and all by 300 and
    /// the cards with them; a Completed obligation is ignored; reality columns are not adjusted.
    fn r_bal_6_available_subtracts_reserved() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-01-05", "1000", "Pay")
            .status(TransactionStatus::Cleared)
            .done();
        t.obligation("Trip").account(chk).balance("300").done();
        t.obligation("Old")
            .account(chk)
            .balance("500")
            .status(ObligationStatus::Completed)
            .done();
        let db = t.build();

        let off = build_data(&db, None, false);
        assert_eq!(row(&off, "Checking").cleared, m("1000"));

        let on = build_data(&db, None, true);
        let r = row(&on, "Checking");
        assert_eq!(r.cleared, m("700"));
        assert_eq!(r.all, m("700"));
        assert_eq!(r.reality_cleared, m("1000"));
        assert_eq!(r.reality_all, m("1000"));
        assert_eq!(on.net_worth, m("700"));
        assert_eq!(on.assets, m("700"));
    }

    #[test]
    /// Covers: R-ACC-1. A soft-deleted account is absent from the rows and the cards.
    fn dashboard_excludes_deleted_accounts() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let gone = t.account("Gone", AccountType::Savings);
        t.tx(gone, "2026-01-05", "900", "Old")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(chk, "2026-01-05", "10", "Kept")
            .status(TransactionStatus::Cleared)
            .done();
        t.delete(RowKey::Account(gone));
        let db = t.build();
        let d = build_data(&db, None, false);
        assert!(
            d.groups
                .iter()
                .flat_map(|g| g.rows.iter())
                .all(|r| r.id != gone),
            "deleted account must not appear"
        );
        assert_eq!(d.assets, m("10"));
        assert_eq!(d.net_worth, m("10"));
    }
}
