//! Contexts screen (T20, Swift `ContextsView`): the active contexts, then the archived ones (dimmed).
//! Each row shows its colour, an "archived" pill, notes, and how many transactions carry it. The
//! scenario menu chooses the scenario; the counts are the transactions visible under it (R-SCN-1).
//! Click edits; right-click archives or unarchives, and deletes (refused while an obligation uses it).

use std::collections::BTreeMap;

use egui::{Align, Frame, Layout, RichText, Role, Sense, Ui, WidgetInfo};
use pf_core::balance::is_visible;
use pf_engine::commands::contexts::{contexts_by_section, delete_context, set_context_archived};
use pf_model::{ContextId, Db, ScenarioId, TransactionId};

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::dialogs::context_edit::ContextForm;
use crate::widgets::color::{color_dot, parse_hex};
use crate::widgets::colors;

/// Colour of the dot when a context has none or an invalid one (R-CTX-4, blue).
const FALLBACK: egui::Color32 = egui::Color32::from_rgb(0, 0, 255);

/// Per-screen UI state. The per-context counts are cached per engine revision.
#[derive(Default)]
pub struct State {
    counts: Cached<Option<ScenarioId>, BTreeMap<ContextId, usize>>,
}

/// One context row, resolved for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextRow {
    /// The context.
    pub id: ContextId,
    /// Its name.
    pub name: String,
    /// Its colour, if valid.
    pub color: Option<egui::Color32>,
    /// Notes, if any.
    pub notes: Option<String>,
    /// Whether it is archived.
    pub archived: bool,
}

/// Transactions per context that are visible under `active` (R-SCN-1), in one pass.
pub fn count_by_context(db: &Db, active: Option<ScenarioId>) -> BTreeMap<ContextId, usize> {
    let mut counts = BTreeMap::new();
    for tx in db.live_transactions().filter(|tx| is_visible(tx, active)) {
        for ctx in &tx.context_ids {
            *counts.entry(*ctx).or_insert(0) += 1;
        }
    }
    counts
}

/// The rows of the Active and Archived sections, each sorted by name (R-CTX-3).
pub fn build_sections(db: &Db) -> (Vec<ContextRow>, Vec<ContextRow>) {
    let (active, archived) = contexts_by_section(db);
    let row = |id: ContextId, archived: bool| {
        db.context(id).map(|c| ContextRow {
            id,
            name: c.name.clone(),
            color: c.color_hex.as_deref().and_then(parse_hex),
            notes: c.notes.clone().filter(|n| !n.trim().is_empty()),
            archived,
        })
    };
    (
        active.into_iter().filter_map(|id| row(id, false)).collect(),
        archived
            .into_iter()
            .filter_map(|id| row(id, true))
            .collect(),
    )
}

/// The live transactions tagged with `ctx` that are visible under `active` (R-SCN-1): the ones the
/// count shows and the context's transactions window lists.
pub fn context_transaction_ids(
    db: &Db,
    ctx: ContextId,
    active: Option<ScenarioId>,
) -> Vec<TransactionId> {
    db.live_transactions()
        .filter(|tx| is_visible(tx, active) && tx.context_ids.contains(&ctx))
        .map(|tx| tx.id)
        .collect()
}

/// What the user asked for this frame, applied after the list is drawn.
enum Action {
    Add,
    Edit(ContextId),
    Transactions(ContextId),
    SetArchived(ContextId, bool),
    Delete(ContextId),
}

/// Draws the Contexts screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let revision = cx.engine.revision();
    let mut action: Option<Action> = None;

    ui.horizontal(|ui| {
        ui.heading("Contexts");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("+ Add Context").clicked() {
                action = Some(Action::Add);
            }
        });
    });
    ui.separator();

    let active = cx.engine.state.active_scenario;
    let counts = st.counts.get(revision, &active, || {
        count_by_context(cx.engine.db(), active)
    });
    let (active, archived) = build_sections(cx.engine.db());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(RichText::new("Active").strong());
            if active.is_empty() {
                ui.label(RichText::new("No contexts").weak());
            }
            for row in &active {
                context_row(
                    ui,
                    row,
                    counts.get(&row.id).copied().unwrap_or(0),
                    &mut action,
                );
            }
            if !archived.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("Archived").strong());
                for row in &archived {
                    context_row(
                        ui,
                        row,
                        counts.get(&row.id).copied().unwrap_or(0),
                        &mut action,
                    );
                }
            }
        });

    match action {
        None => {}
        Some(Action::Add) => cx.dialogs.open(Dialog::ContextEdit(Box::default())),
        Some(Action::Transactions(id)) => {
            let active = cx.engine.state.active_scenario;
            let name = cx
                .engine
                .db()
                .context(id)
                .map_or_else(String::new, |c| c.name.clone());
            let ids = context_transaction_ids(cx.engine.db(), id, active);
            cx.dialogs.open(Dialog::ContextTransactions {
                title: format!("Transactions: {name}"),
                ids,
            });
        }
        Some(Action::Edit(id)) => {
            if let Some(form) = ContextForm::edit(cx.engine.db(), id) {
                cx.dialogs.open(Dialog::ContextEdit(Box::new(form)));
            }
        }
        Some(Action::SetArchived(id, archived)) => {
            if let Err(e) = set_context_archived(cx.engine, id, archived) {
                cx.toasts.info(e.to_string());
            }
        }
        Some(Action::Delete(id)) => {
            if let Err(e) = delete_context(cx.engine, id) {
                cx.dialogs.alert("Cannot Delete", e.to_string());
            }
        }
    }
}

/// One row: the dot, the name, an "archived" pill, notes, the count, and a weak "Edit" hint. Archived
/// rows are drawn dimmed. A click edits; a right-click offers Archive or Unarchive, and Delete.
fn context_row(ui: &mut Ui, row: &ContextRow, count: usize, action: &mut Option<Action>) {
    let dim = |c: egui::Color32| {
        if row.archived {
            c.gamma_multiply(0.6)
        } else {
            c
        }
    };
    // Where the count was drawn, so a click there opens the transactions rather than the editor.
    let mut count_rect = egui::Rect::NOTHING;
    let resp = ui
        .horizontal(|ui| {
            color_dot(ui, dim(row.color.unwrap_or(FALLBACK)));
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&row.name)
                            .strong()
                            .color(dim(ui.visuals().text_color())),
                    );
                    if row.archived {
                        Frame::new()
                            .fill(ui.visuals().faint_bg_color)
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(4, 1))
                            .show(ui, |ui| {
                                ui.label(RichText::new("archived").small().weak());
                            });
                    }
                });
                if let Some(notes) = &row.notes {
                    ui.label(RichText::new(notes).small().weak());
                }
            });
            ui.add_space(12.0);
            let count_label = ui
                .label(
                    RichText::new(count.to_string())
                        .color(colors::BLUE)
                        .strong(),
                )
                .on_hover_text("Show these transactions");
            count_label.widget_info(|| {
                WidgetInfo::labeled(
                    Role::Button,
                    true,
                    format!("Transactions {} ({count})", row.name),
                )
            });
            count_rect = count_label.rect;
            ui.label(RichText::new("Edit").small().weak());
        })
        .response;
    let click = ui.interact(
        resp.rect,
        ui.scope_id().with(("context_row", row.id.raw())),
        Sense::click(),
    );
    if click.clicked() {
        let on_count = ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| count_rect.contains(p));
        *action = Some(if on_count {
            Action::Transactions(row.id)
        } else {
            Action::Edit(row.id)
        });
    }
    click.context_menu(|ui| {
        let (label, archive) = if row.archived {
            ("Unarchive", false)
        } else {
            ("Archive", true)
        };
        if ui.button(label).clicked() {
            *action = Some(Action::SetArchived(row.id, archive));
            ui.close();
        }
        ui.separator();
        if ui.button("Delete").clicked() {
            *action = Some(Action::Delete(row.id));
            ui.close();
        }
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;
    use pf_model::{AccountType, TransactionStatus};

    #[test]
    /// Covers: R-SCN-1. A context's transactions window lists the tagged transactions visible under
    /// the active scenario; the count is the same set.
    fn r_scn_1_context_transactions_follow_active_scenario() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let trip = t.context("Trip");
        let plan = t.scenario("Plan A");
        let hotel = t
            .tx(chk, "2026-03-01", "-40", "Hotel")
            .contexts(&[trip])
            .status(TransactionStatus::Cleared)
            .done();
        let taxi = t
            .tx(chk, "2026-03-02", "-9", "Plan taxi")
            .contexts(&[trip])
            .scenario(plan)
            .status(TransactionStatus::Cleared)
            .done();
        let db = t.build();
        assert_eq!(context_transaction_ids(&db, trip, None), vec![hotel]);
        assert_eq!(
            context_transaction_ids(&db, trip, Some(plan)),
            vec![hotel, taxi]
        );
        assert_eq!(count_by_context(&db, Some(plan)).get(&trip), Some(&2));
    }

    #[test]
    /// Covers: R-CTX-3. Archived contexts are in their own list; each list is sorted by name.
    fn r_ctx_3_sections_split_active_and_archived() {
        let mut t = TestDb::new();
        let vac = t.context("Vacation");
        let tax = t.context("Tax Prep 2025");
        let mut db = t.build();
        if let Some(c) = db.contexts.get_mut(&tax) {
            c.is_archived = true;
        }
        let (active, archived) = build_sections(&db);
        assert_eq!(active.iter().map(|r| r.id).collect::<Vec<_>>(), vec![vac]);
        assert_eq!(archived.iter().map(|r| r.id).collect::<Vec<_>>(), vec![tax]);
    }

    #[test]
    /// Covers: R-SCN-1. The counts follow the active scenario: Reality leaves out scenario rows,
    /// and a scenario adds its own rows to the Reality ones.
    fn r_scn_1_counts_follow_active_scenario() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", pf_model::AccountType::Checking);
        let trip = t.context("Trip");
        let plan = t.scenario("Plan A");
        t.tx(chk, "2026-03-01", "-40", "Hotel")
            .contexts(&[trip])
            .done();
        t.tx(chk, "2026-03-02", "-9", "Plan hotel")
            .contexts(&[trip])
            .scenario(plan)
            .done();
        let db = t.build();
        assert_eq!(count_by_context(&db, None).get(&trip), Some(&1));
        assert_eq!(count_by_context(&db, Some(plan)).get(&trip), Some(&2));
    }
}
