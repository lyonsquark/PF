//! Account detail screen (Swift `AccountDetailView`): the header with the editable name and the
//! balances, the scenario label and time-period picker, the action row with the search toggle
//! (T15c), the search bar, and the transaction table. Rows come from `rows::build_rows`, cached per
//! engine revision and `RowsKey`.

mod edit;
mod menu;
mod rows;
mod search;
mod table;

use std::collections::{BTreeSet, HashSet};

use egui::{Align, ComboBox, Frame, Layout, RichText, TextEdit, TextStyle, Ui};
use jiff::civil::Date;
use pf_core::filter::{SearchState, in_window};
use pf_core::{balance, format};
use pf_engine::commands::accounts::{self, AccountInput};
use pf_model::{AccountId, Db, ScenarioId, TimePeriod, TransactionId};
use rust_decimal::Decimal;

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::screen::Screen;
use crate::widgets::colors;

use edit::{
    CellEdit, Edits, Pending, add_to_budget_toast, attach, commit, commit_status, delete,
    duplicate, open_attachment,
};
use menu::MenuAction;
pub use rows::{Rows, RowsKey, TxRow, build_rows};

/// Width of the account name field in points, so the balance keeps its room on the right.
const NAME_WIDTH: f32 = 260.0;

/// Per-screen UI state. The selection and anchor belong to the account on screen (`for_account`).
#[derive(Default)]
pub struct State {
    /// Table rows, cached per engine revision and `RowsKey`.
    pub(crate) rows: Cached<RowsKey, Rows>,
    /// (balance R-BAL-1, cleared R-BAL-2) for the header, per account and scenario.
    pub(crate) header: Cached<(AccountId, Option<ScenarioId>), (Decimal, Decimal)>,
    /// Selected transaction ids (click, ⌘-click, ⇧-click).
    pub selection: BTreeSet<TransactionId>,
    /// Anchor row for ⇧-click ranges.
    pub(crate) anchor: Option<TransactionId>,
    /// The account the selection belongs to; the selection clears when this changes.
    pub(crate) for_account: Option<AccountId>,
    /// Header name buffer while the name field has focus.
    pub(crate) name_edit: Option<String>,
    /// The text cell (Payee or Memo) being edited, if any (T15b).
    pub(crate) cell: Option<CellEdit>,
    /// The amount cell being edited: its row and the edit text (T15b).
    pub(crate) amount: Option<(TransactionId, String)>,
    /// The table's sideways scroll (clamped at both ends).
    pub(crate) hscroll: crate::widgets::hscroll::HScroll,
    /// The row whose tag "+" menu is open, so its button stays shown while the menu is (T16).
    pub(crate) tag_open: Option<TransactionId>,
    /// The search bar's state; `Some` once it has been opened (T15c).
    pub(crate) search: Option<SearchState>,
    /// Ask the search field for focus on the next frame (set when the bar opens).
    pub(crate) focus_search: bool,
    /// The (revision, key) for which the selection was last pruned to the visible rows.
    pruned: Option<(u64, RowsKey)>,
    /// A transaction to scroll to on the next frame (set by `reveal`); taken once drawn.
    pub(crate) reveal_scroll: Option<TransactionId>,
}

/// Shows `tx` in account `id` with it selected, from a drill-down window. The search is closed, and
/// the time period widens to All when it leaves the transaction out.
pub fn reveal(cx: &mut UiCtx, st: &mut State, id: AccountId, tx: TransactionId) {
    let period = cx.engine.state.time_period;
    let today = cx.engine.today();
    let shown = cx
        .engine
        .db()
        .transaction(tx)
        .is_some_and(|t| in_window(t, period, today));
    if !shown && let Err(e) = cx.engine.set_time_period(TimePeriod::All) {
        cx.toasts.info(e.to_string());
    }
    *cx.screen = Screen::Account(id);
    st.selection.clear();
    st.selection.insert(tx);
    st.anchor = Some(tx);
    st.for_account = Some(id);
    st.name_edit = None;
    st.cell = None;
    st.amount = None;
    st.search = None;
    st.focus_search = false;
    st.tag_open = None;
    st.reveal_scroll = Some(tx);
}

/// ⌘F on the account screen: opens the search bar, or closes it when it is open (T15c).
pub fn toggle_search(st: &mut State, today: Date) {
    search::toggle(&mut st.search, today, &mut st.focus_search);
}

/// Draws the account detail for `id` into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui, id: AccountId) {
    if st.for_account != Some(id) {
        st.selection.clear();
        st.anchor = None;
        st.name_edit = None;
        st.cell = None;
        st.amount = None;
        st.search = None;
        st.focus_search = false;
        st.tag_open = None;
        st.hscroll = Default::default();
        st.for_account = Some(id);
    }
    let Some((name, institution)) = cx
        .engine
        .db()
        .account(id)
        .map(|a| (a.name.clone(), a.institution.clone()))
    else {
        ui.label("This account no longer exists.");
        return;
    };

    let active = cx.engine.state.active_scenario;
    let revision = cx.engine.revision();
    let key = RowsKey {
        account: id,
        active,
        period: cx.engine.state.time_period,
        today: cx.engine.today(),
        search: st.search.as_ref().filter(|s| s.is_active()).cloned(),
    };
    // The result count for the search bar. The table below reads the same cached rows.
    let count = st
        .rows
        .get(revision, &key, || {
            build_rows(cx.engine.db(), &cx.engine.state, &key)
        })
        .rows
        .len();
    let (balance, cleared) = *st.header.get(revision, &(id, active), || {
        header_totals(cx.engine.db(), id, active)
    });
    let head = Header {
        name: &name,
        institution: &institution,
        balance,
        cleared,
    };
    header(cx, st, ui, id, &head);
    ui.separator();
    period_row(cx, ui, active);
    action_row(cx, st, ui, id, count);

    let State {
        rows,
        selection,
        anchor,
        pruned,
        cell,
        amount,
        tag_open,
        hscroll,
        reveal_scroll,
        ..
    } = st;
    let scroll_to = reveal_scroll.take();
    let view = rows.get(revision, &key, || {
        build_rows(cx.engine.db(), &cx.engine.state, &key)
    });
    // Drop selected ids that are no longer visible (after deletes or a period change). Done only
    // when the rows change, so a frame without changes does no work.
    let fresh = pruned
        .as_ref()
        .is_some_and(|(r, k)| *r == revision && *k == key);
    if !fresh {
        if !selection.is_empty() {
            let live: HashSet<TransactionId> = view.rows.iter().map(|r| r.id).collect();
            selection.retain(|id| live.contains(id));
            if anchor.is_some_and(|a| !live.contains(&a)) {
                *anchor = None;
            }
        }
        *pruned = Some((revision, key));
    }
    // The table only records edits; they are applied below, once the rows are no longer borrowed.
    let pending = {
        let lookups = cx.caches.lookups(cx.engine);
        let mut edits = Edits {
            cell,
            amount,
            categories: &lookups.categories,
            contexts: &lookups.contexts,
            searching: view.searching,
            tag_open,
            pending: Vec::new(),
        };
        table::show(
            ui, &view.rows, selection, anchor, hscroll, scroll_to, &mut edits,
        );
        edits.pending
    };
    for change in pending {
        match change {
            Pending::Field(row, field) => commit(cx, row, field),
            Pending::Status(row, status) => commit_status(cx, row, status),
            Pending::Attach(row) => attach(cx, row),
            Pending::OpenAttachment(path) => open_attachment(cx, &path),
            Pending::Menu(MenuAction::AddToBudget(row)) => add_to_budget_toast(cx, row),
            Pending::Menu(MenuAction::Duplicate(ids)) => {
                // The copies are selected, so the user can act on them (T15c).
                if let Some(new) = duplicate(cx, &ids) {
                    selection.clear();
                    selection.extend(new);
                    *anchor = None;
                }
            }
            Pending::Menu(MenuAction::Delete(ids)) => delete(cx, &ids),
        }
    }
}

/// R-BAL-1 and R-BAL-2 for one account under the active scenario.
fn header_totals(db: &Db, id: AccountId, active: Option<ScenarioId>) -> (Decimal, Decimal) {
    (
        balance::account_balance(db, id, active),
        balance::account_cleared_balance(db, id, active),
    )
}

/// What the header shows for one account.
struct Header<'a> {
    name: &'a str,
    institution: &'a str,
    /// R-BAL-1.
    balance: Decimal,
    /// R-BAL-2.
    cleared: Decimal,
}

/// The header: the editable name and institution on the left; the balance on the right, with the
/// cleared balance under it when it differs (Swift `AccountBalanceView`).
fn header(cx: &mut UiCtx, st: &mut State, ui: &mut Ui, id: AccountId, head: &Header<'_>) {
    let Header {
        name,
        institution,
        balance,
        cleared,
    } = *head;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            name_field(cx, st, ui, id, name);
            ui.label(RichText::new(institution).size(16.0).weak());
        });
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            let mut big = RichText::new(format::format_currency(balance))
                .size(22.0)
                .strong();
            if balance < Decimal::ZERO {
                big = big.color(colors::RED);
            }
            ui.label(big);
            if cleared != balance {
                // Right to left: the amount sits at the right edge, under the balance, and the
                // label reads "Cleared:" just before it.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let text = RichText::new(format::format_currency(cleared)).size(16.0);
                    if cleared < Decimal::ZERO {
                        ui.label(text.color(colors::RED));
                    } else {
                        ui.label(text.weak());
                    }
                    ui.label(RichText::new("Cleared:").size(16.0).weak());
                });
            }
        });
    });
}

/// The account name, large and borderless. Its buffer lives in `st.name_edit` while focused; the
/// rename commits when the field loses focus (Enter or click away), as one undo step (T15a).
fn name_field(cx: &mut UiCtx, st: &mut State, ui: &mut Ui, id: AccountId, name: &str) {
    let mut text = st.name_edit.clone().unwrap_or_else(|| name.to_string());
    let enabled = ui.is_enabled();
    let resp = ui.add(
        TextEdit::singleline(&mut text)
            .font(TextStyle::Heading)
            .frame(Frame::NONE)
            .desired_width(NAME_WIDTH),
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, "Account Name"));
    if resp.lost_focus() {
        commit_name(cx, id, &text, name);
    }
    st.name_edit = resp.has_focus().then_some(text);
}

/// Saves a changed, non-empty, trimmed name. Other edits are dropped; an engine error is a toast.
fn commit_name(cx: &mut UiCtx, id: AccountId, typed: &str, current: &str) {
    let trimmed = typed.trim();
    if trimmed.is_empty() || trimmed == current {
        return;
    }
    let Some(account) = cx.engine.db().account(id).cloned() else {
        return;
    };
    let input = AccountInput {
        name: trimmed.to_string(),
        account_type: account.account_type,
        institution: account.institution,
        currency: account.currency,
        is_external: account.is_external,
    };
    if let Err(e) = accounts::update_account(cx.engine, id, input) {
        cx.toasts.info(e.to_string());
    }
}

/// The scenario label (only under an active scenario) and the Time Period picker (R-FLT-1).
fn period_row(cx: &mut UiCtx, ui: &mut Ui, active: Option<ScenarioId>) {
    ui.horizontal(|ui| {
        if let Some(scenario) = active.and_then(|s| cx.engine.db().scenario(s)) {
            ui.label(
                RichText::new(format!("Scenario: {}", scenario.name))
                    .color(colors::ORANGE)
                    .strong(),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let current = cx.engine.state.time_period;
            let mut selected = current;
            ComboBox::from_label("Time Period")
                .selected_text(current.label())
                .show_ui(ui, |ui| {
                    for period in TimePeriod::ALL {
                        ui.selectable_value(&mut selected, period, period.label());
                    }
                });
            if selected != current
                && let Err(e) = cx.engine.set_time_period(selected)
            {
                cx.toasts.info(e.to_string());
            }
        });
    });
}

/// The action row: the "Find" button toggles the search bar (T15c), which opens under the row.
/// T16 (Add Transaction) and T22 (Import CSV, Reality only) add their buttons here.
fn action_row(cx: &mut UiCtx, st: &mut State, ui: &mut Ui, id: AccountId, count: usize) {
    ui.horizontal(|ui| {
        let add = egui::Button::new(
            RichText::new("+ Add Transaction")
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .fill(colors::BLUE);
        if ui.add(add).on_hover_text("Add a new transaction").clicked() {
            let today = cx.engine.today();
            let scenario = cx.engine.state.active_scenario;
            cx.dialogs.open(Dialog::AddTransaction(Box::new(
                crate::dialogs::add_transaction::TxForm::new(Some(id), scenario, today),
            )));
        }
        // Import is for Reality only (T16 note).
        if cx.engine.state.active_scenario.is_none()
            && ui
                .button("Import CSV…")
                .on_hover_text("Import transactions from a bank CSV")
                .clicked()
        {
            crate::dialogs::csv_wizard::pick_bank_csv(cx, id);
        }
        let open = st.search.as_ref().is_some_and(|s| s.open);
        let hover = if open {
            "Hide search"
        } else {
            "Search transactions"
        };
        if ui.button("Find").on_hover_text(hover).clicked() {
            search::toggle(&mut st.search, cx.engine.today(), &mut st.focus_search);
        }
    });
    if let Some(search) = st.search.as_mut().filter(|s| s.open) {
        search::bar(ui, search, count, &mut st.focus_search);
    }
}
