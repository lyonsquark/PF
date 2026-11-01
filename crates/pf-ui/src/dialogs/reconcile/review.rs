//! The Review step: balances, the six sections, manual pairing, Finish (R-REC-12, R-REC-13, R-REC-14).

use std::collections::BTreeMap;

use egui::{CollapsingHeader, ComboBox, Grid, RichText, Role, ScrollArea, Ui, WidgetInfo};
use pf_core::format::format_currency;
use pf_core::reconcile::{
    BalanceStyle, Choice, Confidence, Reading, ReconcileInput, Report, Statement, difference,
    reconcile_with,
};
use pf_engine::commands::reconcile::{Apply, apply_reconciliation, preferred_reading};
use pf_model::{AccountId, Db, TransactionId};
use rust_decimal::Decimal;

use super::Action;
use crate::app::UiCtx;
use crate::widgets::colors;

/// Height of the scrolling list of sections, in points.
const LIST_HEIGHT: f32 = 380.0;

/// One pair the owner can accept or drop.
#[derive(Debug, Clone, PartialEq)]
pub struct PairState {
    /// Index into `statement.lines`.
    pub line: usize,
    /// The paired transaction.
    pub tx: TransactionId,
    /// How it was found; `None` for a pair made by hand.
    pub confidence: Option<Confidence>,
    /// Accepted pairs are marked Reconciled on Finish.
    pub accepted: bool,
    /// Replace the transaction's amount with the statement's.
    pub use_statement: bool,
}

/// The Review step's state.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    /// The statement under review.
    pub statement: Statement,
    /// The report for the current reading.
    pub report: Report,
    /// Pairs from the report and by hand.
    pub pairs: Vec<PairState>,
    /// Statement-only lines and whether each is added.
    pub add: BTreeMap<usize, bool>,
    /// First reconciliation: also mark older transactions.
    pub mark_before: bool,
    /// Showing the "add an adjustment?" question.
    pub confirm_adjust: bool,
    /// The last apply error.
    pub error: Option<String>,
}

/// A change requested while drawing, applied afterwards.
enum Op {
    Pair(usize, TransactionId),
    Unpair(usize),
    Flip,
}

impl Review {
    /// Builds the report with the remembered reading, or `forced` when given.
    pub fn new(
        db: &Db,
        account: AccountId,
        statement: Statement,
        forced: Option<Reading>,
    ) -> Result<Self, String> {
        let preferred = preferred_reading(db, account);
        let report = reconcile_with(
            &ReconcileInput {
                db,
                account,
                statement: &statement,
                preferred,
            },
            forced,
        )?;
        let pairs = report
            .matched
            .iter()
            .chain(&report.amount_differs)
            .map(|p| PairState {
                line: p.line,
                tx: p.transaction,
                confidence: Some(p.confidence),
                accepted: true,
                use_statement: false,
            })
            .collect();
        let add = report.statement_only.iter().map(|&i| (i, true)).collect();
        Ok(Self {
            statement,
            report,
            pairs,
            add,
            mark_before: false,
            confirm_adjust: false,
            error: None,
        })
    }

    /// Statement-only lines not paired by hand.
    fn open_lines(&self) -> Vec<usize> {
        self.report
            .statement_only
            .iter()
            .copied()
            .filter(|i| !self.pairs.iter().any(|p| p.line == *i))
            .collect()
    }

    /// PF-only transactions in the period not paired by hand.
    fn open_txs(&self) -> Vec<TransactionId> {
        self.report
            .pf_only_in_period
            .iter()
            .copied()
            .filter(|t| !self.pairs.iter().any(|p| p.tx == *t))
            .collect()
    }

    /// The owner's decisions so far (no adjustment).
    pub fn choice(&self) -> Choice {
        Choice {
            pairs: self
                .pairs
                .iter()
                .filter(|p| p.accepted)
                .map(|p| (p.line, p.tx, p.use_statement))
                .collect(),
            add_lines: self
                .open_lines()
                .into_iter()
                .filter(|i| self.add.get(i).copied().unwrap_or(false))
                .collect(),
            adjustment: None,
        }
    }

    /// R-REC-13: what is left after the current choice.
    pub fn difference(&self) -> Decimal {
        difference(&self.report, &self.choice())
    }
}

fn money(d: Decimal) -> String {
    format_currency(d)
}

/// A checkbox whose accessible name is `name` (the visible text may repeat on many rows).
fn check(ui: &mut Ui, value: &mut bool, text: &str, name: String) {
    let r = ui.checkbox(value, text);
    let on = *value;
    r.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, name.clone()));
}

/// Draws the Review step.
pub(super) fn show(
    cx: &mut UiCtx,
    ui: &mut Ui,
    account: AccountId,
    r: &mut Review,
    action: &mut Action,
) {
    let mut ops = Vec::new();
    header(ui, r, &mut ops);
    balances(ui, r);
    ScrollArea::vertical()
        .id_salt("rec_sections")
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| sections(cx.engine.db(), ui, r, &mut ops));
    if let Some(err) = &r.error {
        ui.label(RichText::new(err).color(colors::RED));
    }
    buttons(cx, ui, account, r, action);
    for op in ops {
        match op {
            Op::Pair(line, tx) => r.pairs.push(PairState {
                line,
                tx,
                confidence: None,
                accepted: true,
                use_statement: false,
            }),
            Op::Unpair(i) => {
                r.pairs.remove(i);
            }
            Op::Flip => {
                let other = match r.report.reading {
                    Reading::AsPrinted => Reading::Flipped,
                    Reading::Flipped => Reading::AsPrinted,
                };
                let st = r.statement.clone();
                match Review::new(cx.engine.db(), account, st, Some(other)) {
                    Ok(next) => *r = next,
                    Err(e) => r.error = Some(e),
                }
            }
        }
    }
}

/// Institution, period and the reading line with Flip signs.
fn header(ui: &mut Ui, r: &Review, ops: &mut Vec<Op>) {
    let st = &r.statement;
    let who = format!("{} {}", st.institution, st.account_hint);
    if !who.trim().is_empty() {
        ui.label(RichText::new(who.trim()).strong());
    }
    ui.label(format!(
        "Statement {} to {}",
        st.period_start, st.period_end
    ));
    let style = match r.report.style {
        BalanceStyle::Owed => "owed",
        BalanceStyle::Asset => "asset",
    };
    let signs = match r.report.reading {
        Reading::AsPrinted => "as printed",
        Reading::Flipped => "flipped",
    };
    let n = match r.report.reading {
        Reading::AsPrinted => r.report.matches_as_printed,
        Reading::Flipped => r.report.matches_flipped,
    };
    ui.horizontal(|ui| {
        ui.label(format!(
            "Read as: {style} balances, statement signs {signs}"
        ));
        if ui.button("Flip signs").clicked() {
            ops.push(Op::Flip);
        }
        ui.label(format!("({n} of {} lines match)", st.lines.len()));
    });
}

/// Opening, closing and the live Difference.
fn balances(ui: &mut Ui, r: &mut Review) {
    ui.separator();
    ui.label(format!("Opening balance: {}", money(r.report.opening)));
    if !r.report.opening_difference.is_zero() {
        let text = format!(
            "PF's reconciled balance is {} (off by {}).",
            money(r.report.reconciled_balance),
            money(r.report.opening_difference)
        );
        if r.report.first_time {
            ui.label(text);
        } else {
            ui.label(RichText::new(text).color(colors::RED));
        }
    }
    if r.report.first_time {
        ui.label("This is the first reconciliation for this account.");
        ui.checkbox(
            &mut r.mark_before,
            "Mark earlier transactions in this account as Reconciled",
        );
    }
    ui.label(format!("Closing balance: {}", money(r.report.closing)));
    let diff = r.difference();
    let text = RichText::new(format!("Difference: {}", money(diff))).strong();
    ui.label(text.color(if diff.is_zero() {
        colors::GREEN
    } else {
        colors::RED
    }));
    ui.separator();
}

/// The six collapsible groups, in order, each only when non-empty.
fn sections(db: &Db, ui: &mut Ui, r: &mut Review, ops: &mut Vec<Op>) {
    let is_diff = |p: &PairState| p.confidence == Some(Confidence::AmountDiffers);
    let matched: Vec<usize> = (0..r.pairs.len())
        .filter(|&i| !is_diff(&r.pairs[i]))
        .collect();
    let differs: Vec<usize> = (0..r.pairs.len())
        .filter(|&i| is_diff(&r.pairs[i]))
        .collect();
    if !matched.is_empty() {
        group(ui, "Matched", matched.len(), true, |ui| {
            pair_rows(db, ui, r, &matched, ops, false);
        });
    }
    if !differs.is_empty() {
        group(ui, "Amount differs", differs.len(), true, |ui| {
            pair_rows(db, ui, r, &differs, ops, true);
        });
    }
    let lines = r.open_lines();
    if !lines.is_empty() {
        group(ui, "On statement only", lines.len(), true, |ui| {
            statement_only_rows(ui, r, &lines);
        });
    }
    let txs = r.open_txs();
    if !txs.is_empty() {
        group(ui, "In PF only, in this period", txs.len(), true, |ui| {
            pf_only_rows(db, ui, r, &txs, ops);
        });
    }
    if !r.report.already_reconciled.is_empty() {
        let list = r.report.already_reconciled.clone();
        group(ui, "Already reconciled", list.len(), true, |ui| {
            Grid::new("rec_already").striped(true).show(ui, |ui| {
                for (line, tx) in &list {
                    line_cells(ui, r, *line);
                    tx_cells(db, ui, *tx);
                    ui.end_row();
                }
            });
        });
    }
    if !r.report.pf_only_outside.is_empty() {
        let list = r.report.pf_only_outside.clone();
        group(
            ui,
            "In PF only, outside this period",
            list.len(),
            false,
            |ui| {
                Grid::new("rec_outside").striped(true).show(ui, |ui| {
                    for tx in &list {
                        tx_cells(db, ui, *tx);
                        ui.end_row();
                    }
                });
            },
        );
    }
}

fn group(ui: &mut Ui, title: &str, n: usize, open: bool, body: impl FnOnce(&mut Ui)) {
    CollapsingHeader::new(format!("{title} ({n})"))
        .id_salt(title)
        .default_open(open)
        .show(ui, body);
}

/// Date, description and amount of a statement line.
fn line_cells(ui: &mut Ui, r: &Review, line: usize) {
    let l = &r.statement.lines[line];
    ui.label(l.date.to_string());
    ui.label(l.description.as_str());
    ui.label(money(r.report.amounts[line]));
}

/// Date, payee and amount of a PF transaction.
fn tx_cells(db: &Db, ui: &mut Ui, id: TransactionId) {
    match db.transaction(id) {
        Some(t) => {
            ui.label(t.date.to_string());
            ui.label(t.payee.as_str());
            ui.label(money(t.amount));
        }
        None => {
            ui.label("");
            ui.label("(missing)");
            ui.label("");
        }
    }
}

fn pair_rows(
    db: &Db,
    ui: &mut Ui,
    r: &mut Review,
    idx: &[usize],
    ops: &mut Vec<Op>,
    differs: bool,
) {
    Grid::new(if differs {
        "rec_differs"
    } else {
        "rec_matched"
    })
    .striped(true)
    .show(ui, |ui| {
        for &i in idx {
            let (line, tx, manual) = {
                let p = &r.pairs[i];
                (p.line, p.tx, p.confidence.is_none())
            };
            line_cells(ui, r, line);
            tx_cells(db, ui, tx);
            let name = r.statement.lines[line].description.clone();
            let p = &mut r.pairs[i];
            check(ui, &mut p.accepted, "Accept", format!("Accept {name}"));
            if differs {
                check(
                    ui,
                    &mut p.use_statement,
                    "Use statement amount",
                    format!("Use statement amount {name}"),
                );
            }
            if manual {
                ui.label("Manual");
                if ui.button("Unpair").clicked() {
                    ops.push(Op::Unpair(i));
                }
            }
            ui.end_row();
        }
    });
}

fn statement_only_rows(ui: &mut Ui, r: &mut Review, lines: &[usize]) {
    Grid::new("rec_statement_only")
        .striped(true)
        .show(ui, |ui| {
            for &i in lines {
                line_cells(ui, r, i);
                let name = r.statement.lines[i].description.clone();
                let on = r.add.entry(i).or_insert(true);
                check(ui, on, "Add", format!("Add {name}"));
                ui.end_row();
            }
        });
}

fn pf_only_rows(db: &Db, ui: &mut Ui, r: &Review, txs: &[TransactionId], ops: &mut Vec<Op>) {
    let free = r.open_lines();
    Grid::new("rec_pf_only").striped(true).show(ui, |ui| {
        for &tx in txs {
            tx_cells(db, ui, tx);
            let (payee, amount) = db
                .transaction(tx)
                .map(|t| (t.payee.clone(), t.amount))
                .unwrap_or_default();
            let resp = ComboBox::from_id_salt(("rec_pair", tx.raw()))
                .selected_text("Pair with…")
                .show_ui(ui, |ui| {
                    for &line in &free {
                        if r.report.amounts[line].is_sign_negative() != amount.is_sign_negative() {
                            continue;
                        }
                        let l = &r.statement.lines[line];
                        let text = format!(
                            "{} {} {}",
                            l.date,
                            l.description,
                            money(r.report.amounts[line])
                        );
                        if ui.selectable_label(false, text).clicked() {
                            ops.push(Op::Pair(line, tx));
                            ui.close();
                        }
                    }
                });
            resp.response.widget_info(|| {
                WidgetInfo::labeled(Role::ComboBox, true, format!("Pair with {payee}"))
            });
            ui.end_row();
        }
    });
}

/// Cancel, Finish and the adjustment question.
fn buttons(cx: &mut UiCtx, ui: &mut Ui, account: AccountId, r: &mut Review, action: &mut Action) {
    ui.add_space(6.0);
    let diff = r.difference();
    if r.confirm_adjust {
        ui.label(format!(
            "The statement and PF differ by {}. Add a \"Reconciliation adjustment\" transaction for {}?",
            money(diff),
            money(diff)
        ));
        ui.horizontal(|ui| {
            if ui.button("Go Back").clicked() {
                r.confirm_adjust = false;
            }
            if ui.button("Add Adjustment").clicked() {
                finish(cx, account, r, Some(diff), action);
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *action = Action::Cancel;
        }
        let label = if diff.is_zero() {
            "Finish"
        } else {
            "Finish with Adjustment…"
        };
        if ui.button(label).clicked() {
            if diff.is_zero() {
                finish(cx, account, r, None, action);
            } else {
                r.confirm_adjust = true;
            }
        }
    });
}

/// Applies the reconciliation (R-REC-14). An error stays in the dialog.
fn finish(
    cx: &mut UiCtx,
    account: AccountId,
    r: &mut Review,
    adjustment: Option<Decimal>,
    action: &mut Action,
) {
    let mut choice = r.choice();
    choice.adjustment = adjustment;
    let mark_before = r.report.first_time && r.mark_before;
    let result = apply_reconciliation(
        cx.engine,
        &Apply {
            account,
            statement: &r.statement,
            report: &r.report,
            choice: &choice,
            mark_before,
        },
    );
    match result {
        Ok(n) => {
            cx.toasts.success(format!("Reconciled {n} transactions."));
            *action = Action::Done;
        }
        Err(e) => {
            r.error = Some(e.to_string());
            r.confirm_adjust = false;
        }
    }
}
