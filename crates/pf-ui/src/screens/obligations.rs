//! Obligations screen (T18, Swift `ObligationsView`): the obligations grouped by status, a card per
//! obligation, a click to open its detail, and a right-click Delete. Balances are the Reality balance
//! (R-OBL-6), whatever scenario is active.

use egui::{Align, Frame, Layout, RichText, Sense, Ui};
use jiff::Timestamp;
use jiff::civil::Date;
use pf_core::format;
use pf_engine::commands::obligations::{
    delete_obligations, display_balance, obligations_by_status,
};
use pf_model::{Db, ObligationId, ObligationStatus};
use rust_decimal::Decimal;

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::dialogs::obligation_detail;
use crate::widgets::colors;

/// Per-screen UI state. The grouped rows are cached per engine revision.
#[derive(Default)]
pub struct State {
    rows: Cached<(), Vec<(ObligationStatus, Vec<ObligationRow>)>>,
}

/// One obligation card, fully resolved for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct ObligationRow {
    /// The obligation.
    pub id: ObligationId,
    /// Name.
    pub name: String,
    /// The live linked account's name, if any.
    pub account: Option<String>,
    /// The first line of the notes, if any.
    pub notes: Option<String>,
    /// Due date, if set.
    pub due: Option<Date>,
    /// Creation time, if known.
    pub created: Option<Timestamp>,
    /// The Reality computed balance (R-OBL-1, R-OBL-6).
    pub balance: Decimal,
    /// Target amount, if set.
    pub target: Option<Decimal>,
}

/// Groups the live obligations by status in `ObligationStatus::ALL` order (R-OBL-5). Empty groups
/// are left out, so their sections are hidden.
pub fn build_rows(db: &Db) -> Vec<(ObligationStatus, Vec<ObligationRow>)> {
    obligations_by_status(db)
        .into_iter()
        .map(|(status, ids)| {
            let rows: Vec<ObligationRow> = ids.into_iter().filter_map(|id| row(db, id)).collect();
            (status, rows)
        })
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

/// The card for one live obligation.
fn row(db: &Db, id: ObligationId) -> Option<ObligationRow> {
    let ob = db.obligation(id).filter(|o| !o.is_deleted)?;
    Some(ObligationRow {
        id,
        name: ob.name.clone(),
        account: ob
            .account_id
            .and_then(|a| db.account(a))
            .filter(|a| !a.is_deleted)
            .map(|a| a.name.clone()),
        notes: ob
            .notes
            .as_deref()
            .and_then(|n| n.lines().next())
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string),
        due: ob.due_date,
        created: ob.created_date,
        balance: display_balance(db, id),
        target: ob.target_amount,
    })
}

/// What the user asked for this frame, applied after the list is drawn.
enum Action {
    Add,
    Open(ObligationId),
    Delete(ObligationId),
}

/// Draws the Obligations screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let revision = cx.engine.revision();
    let mut action: Option<Action> = None;

    ui.horizontal(|ui| {
        ui.heading("Obligations");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("+ Add Obligation").clicked() {
                action = Some(Action::Add);
            }
        });
    });
    ui.separator();

    let groups = st.rows.get(revision, &(), || build_rows(cx.engine.db()));
    if groups.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No obligations yet").weak()));
    } else {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (status, rows) in groups {
                    ui.add_space(6.0);
                    ui.label(RichText::new(status.label()).strong());
                    for r in rows {
                        card(ui, r, &mut action);
                    }
                }
            });
    }

    match action {
        None => {}
        Some(Action::Add) => {
            cx.dialogs.open(Dialog::AddObligation(Box::default()));
        }
        Some(Action::Open(id)) => {
            if let Some(form) = obligation_detail::form_for(cx.engine.db(), id) {
                cx.dialogs.open(Dialog::ObligationDetail(Box::new(form)));
            }
        }
        Some(Action::Delete(id)) => {
            if let Err(e) = delete_obligations(cx.engine, &[id]) {
                cx.toasts.info(e.to_string());
            }
        }
    }
}

/// One card: name, account, notes, dates, and on the right the balance and the target. A click opens
/// the detail; a right-click offers Delete.
fn card(ui: &mut Ui, r: &ObligationRow, action: &mut Option<Action>) {
    let resp = Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&r.name).strong());
                    if let Some(account) = &r.account {
                        ui.label(RichText::new(account).small().weak());
                    }
                    if let Some(notes) = &r.notes {
                        ui.label(RichText::new(notes).small().weak());
                    }
                    let mut dates = Vec::new();
                    if let Some(due) = r.due {
                        dates.push(format!("Due: {}", format_date(due)));
                    }
                    if let Some(created) = r.created {
                        dates.push(format!("Created: {}", format_created(created)));
                    }
                    if !dates.is_empty() {
                        ui.label(RichText::new(dates.join("   ")).small().weak());
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let mut balance = RichText::new(format::format_currency(r.balance)).strong();
                    if r.balance < Decimal::ZERO {
                        balance = balance.color(colors::RED);
                    }
                    ui.label(balance);
                    if let Some(target) = r.target {
                        ui.label(
                            RichText::new(format!("of {}", format::format_currency(target)))
                                .small()
                                .weak(),
                        );
                    }
                });
            });
        })
        .response;
    let click = ui.interact(
        resp.rect,
        ui.scope_id().with(("obligation_card", r.id.raw())),
        Sense::click(),
    );
    if click.clicked() {
        *action = Some(Action::Open(r.id));
    }
    click.context_menu(|ui| {
        if ui.button("Delete").clicked() {
            *action = Some(Action::Delete(r.id));
            ui.close();
        }
    });
    ui.add_space(4.0);
}

/// "Mar 1, 2026" (the Swift date format).
pub fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}

/// "1/1/2026" for the created date on a card.
fn format_created(t: Timestamp) -> String {
    t.to_zoned(jiff::tz::TimeZone::system())
        .date()
        .strftime("%-m/%-d/%Y")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;
    use pf_model::{AccountType, ObligationStatus};

    #[test]
    /// Covers: R-OBL-5. Groups follow the status order; empty groups are left out; names are sorted.
    fn r_obl_5_groups_in_status_order_without_empty_sections() {
        let mut t = TestDb::new();
        t.obligation("Zoo fund")
            .status(ObligationStatus::Funding)
            .done();
        t.obligation("Alpha fund")
            .status(ObligationStatus::Funding)
            .done();
        t.obligation("Done deal")
            .status(ObligationStatus::Completed)
            .done();
        let db = t.build();
        let groups = build_rows(&db);
        let statuses: Vec<ObligationStatus> = groups.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            statuses,
            vec![ObligationStatus::Funding, ObligationStatus::Completed]
        );
        let names: Vec<&str> = groups[0].1.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha fund", "Zoo fund"]);
    }

    #[test]
    /// Covers: R-OBL-6. The card balance is the Reality computed balance: the funded amount plus the
    /// reality rows tagged with its context, not the scenario rows.
    fn card_balance_is_reality() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let plan = t.scenario("Plan A");
        let ctx = t.context("fund");
        t.obligation("Fund")
            .balance("100")
            .context(ctx)
            .account(chk)
            .done();
        t.tx(chk, "2026-03-01", "50", "Deposit")
            .contexts(&[ctx])
            .done();
        t.tx(chk, "2026-03-02", "900", "Scenario deposit")
            .contexts(&[ctx])
            .scenario(plan)
            .done();
        let db = t.build();
        let groups = build_rows(&db);
        let row = &groups[0].1[0];
        assert_eq!(row.balance.to_string(), "150");
    }
}
