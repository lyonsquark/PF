//! Budget commands (T10c): budget item CRUD, Add to Budget, projection into scenarios and
//! Reproject. Rules R-BUD-1..2, R-TXN-11 and R-PRJ-6..9; the Swift sources are
//! `Views/BudgetItemEditorView.swift`, `Views/BudgetProjectionView.swift` and
//! `Views/TransactionListView.swift` (`addToBudget`).

use std::collections::HashSet;

use jiff::Span;
use jiff::civil::Date;
use pf_core::projector::{PROJECTION_MEMO, generate_dates, projected_amount, projected_payee};
use pf_model::{
    AccountId, BudgetFrequency, BudgetItem, BudgetItemId, BudgetItemType, CategoryId, Db, RowKey,
    Scenario, ScenarioId, Transaction, TransactionId, TransactionStatus,
};
use rust_decimal::Decimal;

use crate::engine::Engine;
use crate::error::{EngineError, Result};
use crate::writer::Writer;

/// Setting: name of the scenario of the last projection (R-PRJ-8).
const KEY_PROJ_SCENARIO: &str = "lastProjectionScenario";
/// Setting: first day of the last projection, `YYYY-MM-DD` (R-PRJ-8).
const KEY_PROJ_START: &str = "lastProjectionStartDate";
/// Setting: last day of the last projection, `YYYY-MM-DD` (R-PRJ-8).
const KEY_PROJ_END: &str = "lastProjectionEndDate";

/// Values of the budget item editor (Swift `BudgetItemEditorView`).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetItemInput {
    /// Bill, expense or income.
    pub item_type: BudgetItemType,
    /// Name; trimmed, required.
    pub name: String,
    /// Amount per occurrence, as typed (the sign is ignored by projections).
    pub amount: Decimal,
    /// First occurrence.
    pub start_date: Date,
    /// Last date to project, or none.
    pub end_date: Option<Date>,
    /// How often it recurs.
    pub frequency: BudgetFrequency,
    /// Months 1..=12, kept only for `SpecificMonths`.
    pub specific_months: Vec<u32>,
    /// Notes; whitespace-only is stored as none.
    pub notes: String,
    /// Category for analytical tracking.
    pub category: Option<CategoryId>,
    /// Originating account.
    pub account: Option<AccountId>,
}

/// Editor defaults (Swift `init`): start = Jan 1 of today's year; end = today + 1 year;
/// frequency Monthly; type = the tab's type.
pub fn default_budget_input(item_type: BudgetItemType, today: Date) -> BudgetItemInput {
    // Jan 1 exists in every year jiff can represent, so the fallback is never taken.
    let start_date = Date::new(today.year(), 1, 1).unwrap_or(today);
    let end_date = today.checked_add(Span::new().years(1)).ok();
    BudgetItemInput {
        item_type,
        name: String::new(),
        amount: Decimal::ZERO,
        start_date,
        end_date,
        frequency: BudgetFrequency::Monthly,
        specific_months: Vec::new(),
        notes: String::new(),
        category: None,
        account: None,
    }
}

/// The live budget item with this id, or `NotFound`.
fn live_budget_item(db: &Db, id: BudgetItemId) -> Result<&BudgetItem> {
    db.budget_item(id)
        .filter(|b| !b.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("budget item {}", id.raw())))
}

/// The live scenario with this id, or `NotFound`.
fn live_scenario(db: &Db, id: ScenarioId) -> Result<&Scenario> {
    db.scenario(id)
        .filter(|s| !s.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("scenario {}", id.raw())))
}

/// Validates the editor values and builds the stored row. Same normalisation as Swift `save()`:
/// the name is trimmed and required; months are sorted and deduplicated only for SpecificMonths;
/// notes that are blank become none (kept notes are not trimmed); an end before the start is
/// refused (R-BUD-2).
fn to_item(id: BudgetItemId, input: BudgetItemInput) -> Result<BudgetItem> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Validation("Name is required".into()));
    }
    if input.end_date.is_some_and(|end| end < input.start_date) {
        return Err(EngineError::Validation(
            "End date cannot be before start date".into(),
        ));
    }
    let specific_months = if input.frequency == BudgetFrequency::SpecificMonths {
        let mut months = input.specific_months.clone();
        months.sort_unstable();
        months.dedup();
        months
    } else {
        Vec::new()
    };
    let notes = (!input.notes.trim().is_empty()).then_some(input.notes);
    Ok(BudgetItem {
        id,
        item_type: input.item_type,
        name,
        amount: input.amount,
        start_date: input.start_date,
        end_date: input.end_date,
        frequency: input.frequency,
        specific_months,
        notes,
        category_id: input.category,
        account_id: input.account,
        is_deleted: false,
    })
}

/// "Add Budget Item" / "Edit Budget Item". Validation as in `to_item`; an edit of a row that is
/// not live is `NotFound`.
pub fn save_budget_item(
    e: &mut Engine,
    id: Option<BudgetItemId>,
    input: BudgetItemInput,
) -> Result<BudgetItemId> {
    match id {
        Some(existing) => {
            live_budget_item(e.db(), existing)?;
            let item = to_item(existing, input)?;
            e.apply("Edit Budget Item", |w| {
                w.update_budget_item(existing, move |row| *row = item)?;
                Ok(existing)
            })
        }
        None => {
            let item = to_item(BudgetItemId(0), input)?;
            e.apply("Add Budget Item", |w| Ok(w.insert_budget_item(item)))
        }
    }
}

/// "Delete Budget Item" / "Delete Budget Items" (soft delete, by count). Duplicate ids are
/// ignored; an empty list makes no step. `NotFound` if any id is not a live budget item.
pub fn delete_budget_items(e: &mut Engine, ids: &[BudgetItemId]) -> Result<()> {
    let mut unique: Vec<BudgetItemId> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(*id);
        }
    }
    if unique.is_empty() {
        return Ok(());
    }
    for id in &unique {
        live_budget_item(e.db(), *id)?;
    }
    let step = if unique.len() > 1 {
        "Delete Budget Items"
    } else {
        "Delete Budget Item"
    };
    e.apply(step, |w| {
        for id in unique {
            w.set_deleted(RowKey::BudgetItem(id), true)?;
        }
        Ok(())
    })
}

/// R-BUD-1: live items of a type, sorted by name (ties by id).
pub fn budget_items_by_type(db: &Db, t: BudgetItemType) -> Vec<BudgetItemId> {
    db.budget_items_by_name()
        .into_iter()
        .filter(|b| b.item_type == t)
        .map(|b| b.id)
        .collect()
}

/// R-TXN-11. "Add to Budget" creates a Bill with the payee as name, the transaction's signed
/// amount, date and category and account, and Monthly frequency. Returns the toast text
/// `Added "{payee}" to Bills`. `NotFound` if the transaction is not live.
pub fn add_to_budget(e: &mut Engine, tx: TransactionId) -> Result<String> {
    let item = {
        let t = e
            .db()
            .transaction(tx)
            .filter(|t| !t.is_deleted)
            .ok_or_else(|| EngineError::NotFound(format!("transaction {}", tx.raw())))?;
        BudgetItem {
            category_id: t.category_id,
            account_id: t.account_id,
            ..BudgetItem::new(
                BudgetItemId(0),
                BudgetItemType::Bill,
                &t.payee,
                t.amount,
                t.date,
                BudgetFrequency::Monthly,
            )
        }
    };
    let payee = item.name.clone();
    e.apply("Add to Budget", |w| {
        w.insert_budget_item(item);
        Ok(())
    })?;
    Ok(format!("Added \"{payee}\" to Bills"))
}

/// Where a projection goes.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectionTarget {
    /// A live scenario that already exists.
    Existing(ScenarioId),
    /// A new scenario with this name (trimmed; created in the same step).
    New(String),
}

/// Inputs of Generate Projection.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionRequest {
    /// Existing or new scenario.
    pub target: ProjectionTarget,
    /// First day of the window (inclusive).
    pub start: Date,
    /// Last day of the window (inclusive).
    pub end: Date,
    /// Budget items to project; unknown and deleted ids are skipped.
    pub items: Vec<BudgetItemId>,
}

/// Result of Generate Projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionResult {
    /// The scenario the rows went into.
    pub scenario: ScenarioId,
    /// Number of transactions inserted.
    pub count: usize,
}

/// One projected transaction (R-PRJ-6): status Future, memo "Auto-generated from Budget", the
/// item's account and category, and the scenario.
fn projected_tx(item: &BudgetItem, date: Date, scenario: ScenarioId) -> Transaction {
    Transaction {
        memo: Some(PROJECTION_MEMO.to_string()),
        status: TransactionStatus::Future,
        account_id: item.account_id,
        scenario_id: Some(scenario),
        category_id: item.category_id,
        ..Transaction::new(
            TransactionId(0),
            date,
            projected_amount(item),
            &projected_payee(item),
        )
    }
}

/// R-PRJ-8: saves the three projection settings through the writer, so they undo with the step.
fn save_projection_settings(w: &mut Writer<'_>, scenario: &str, start: Date, end: Date) {
    w.set_setting(KEY_PROJ_SCENARIO, scenario);
    w.set_setting(KEY_PROJ_START, &start.to_string());
    w.set_setting(KEY_PROJ_END, &end.to_string());
}

/// "Generate Projection". R-PRJ-6/7/8: one undo step that creates the scenario if asked, inserts
/// one Future transaction per generated date per item, and saves the last-projection settings.
/// The name is trimmed and must not be empty (`Validation("Scenario name is required")`). An
/// existing target that is not live is `NotFound`.
pub fn generate_projection(e: &mut Engine, req: ProjectionRequest) -> Result<ProjectionResult> {
    let db = e.db();
    let mut seen = HashSet::new();
    let items: Vec<BudgetItem> = req
        .items
        .iter()
        .filter(|id| seen.insert(**id))
        .filter_map(|id| db.budget_item(*id))
        .filter(|b| !b.is_deleted)
        .cloned()
        .collect();
    let (name, existing) = match &req.target {
        ProjectionTarget::Existing(id) => (live_scenario(db, *id)?.name.clone(), Some(*id)),
        ProjectionTarget::New(raw) => {
            let name = raw.trim().to_string();
            if name.is_empty() {
                return Err(EngineError::Validation("Scenario name is required".into()));
            }
            (name, None)
        }
    };
    e.apply("Generate Projection", |w| {
        let scenario = match existing {
            Some(id) => id,
            None => w.insert_scenario(Scenario::new(ScenarioId(0), &name)),
        };
        let mut count = 0;
        for item in &items {
            for date in generate_dates(item, req.start, req.end) {
                w.insert_transaction(projected_tx(item, date, scenario));
                count += 1;
            }
        }
        save_projection_settings(w, &name, req.start, req.end);
        Ok(ProjectionResult { scenario, count })
    })
}

/// R-PRJ-8 prefill: the scenario whose exact name is saved (if a live one has it), and the saved
/// start and end dates. Unparseable or missing settings give `None`.
pub fn last_projection(db: &Db) -> (Option<ScenarioId>, Option<Date>, Option<Date>) {
    let scenario = db
        .setting(KEY_PROJ_SCENARIO)
        .and_then(|name| db.scenario_by_name(name))
        .map(|s| s.id);
    let start = db.setting(KEY_PROJ_START).and_then(|s| s.parse().ok());
    let end = db.setting(KEY_PROJ_END).and_then(|s| s.parse().ok());
    (scenario, start, end)
}

/// "Reproject" (R-PRJ-9). One undo step: saves the editor's values into the item, then adds the
/// item's rows for the last projection's window to the last projection's scenario, exactly as
/// `generate_projection` builds them. Existing rows are kept. Returns the toast
/// `Added {n} transaction{s} to {scenario}`.
/// Precondition: the item is live, all three `lastProjection*` settings exist, and a live
/// scenario has the saved name. Otherwise `Validation("Run a projection first")`.
pub fn reproject_item(e: &mut Engine, id: BudgetItemId, input: BudgetItemInput) -> Result<String> {
    let db = e.db();
    let item_exists = live_budget_item(db, id).is_ok();
    let (scenario, start, end) = match (item_exists, last_projection(db)) {
        (true, (Some(scenario), Some(start), Some(end))) => (scenario, start, end),
        _ => return Err(EngineError::Validation("Run a projection first".into())),
    };
    let scenario_name = live_scenario(db, scenario)?.name.clone();
    let item = to_item(id, input)?;
    let count = e.apply("Reproject", |w| {
        w.update_budget_item(id, |row| *row = item.clone())?;
        let dates = generate_dates(&item, start, end);
        for date in &dates {
            w.insert_transaction(projected_tx(&item, *date, scenario));
        }
        Ok(dates.len())
    })?;
    let plural = if count == 1 { "" } else { "s" };
    Ok(format!(
        "Added {count} transaction{plural} to {scenario_name}"
    ))
}
