//! Obligation commands (T10b): create, edit, delete, grouping and the context-name suggestion.
//! Rules R-OBL-1..6 and BEH-OBL-01..03; the Swift sources are `Views/ObligationsView.swift`,
//! `Views/ObligationDetailView.swift` and `Managers/ObligationManager.swift`.

use jiff::Timestamp;
use jiff::civil::Date;
use pf_core::names::generate_context_name;
use pf_core::obligation as core;
use pf_model::{
    AccountId, Context, ContextId, Db, Obligation, ObligationId, ObligationStatus, RowKey,
};
use rust_decimal::Decimal;

use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// The context choice on the Add Obligation sheet.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextChoice {
    /// No context.
    None,
    /// An existing live context.
    Existing(ContextId),
    /// A new context with this name; blank means no context.
    New(String),
}

/// Form values for Add Obligation.
#[derive(Debug, Clone, PartialEq)]
pub struct NewObligation {
    /// Name; trimmed, must not be empty.
    pub name: String,
    /// Optional target amount.
    pub target_amount: Option<Decimal>,
    /// Initial funded amount.
    pub current_balance: Decimal,
    /// Linked account (required by the UI).
    pub account: AccountId,
    /// Context to link, or to create.
    pub context: ContextChoice,
}

/// One field edited on the obligation detail screen.
#[derive(Debug, Clone, PartialEq)]
pub enum ObligationField {
    /// Name; trimmed, must not be empty.
    Name(String),
    /// Lifecycle status.
    Status(ObligationStatus),
    /// Target amount, or none.
    Target(Option<Decimal>),
    /// Funded balance.
    Balance(Decimal),
    /// Due date, or none.
    DueDate(Option<Date>),
    /// Notes; blank is stored as `None`.
    Notes(Option<String>),
    /// Account, or none.
    Account(Option<AccountId>),
    /// Context, or none.
    Context(Option<ContextId>),
}

/// The live account with this id, or `NotFound`.
fn live_account(db: &Db, id: AccountId) -> Result<()> {
    match db.account(id) {
        Some(a) if !a.is_deleted => Ok(()),
        _ => Err(EngineError::NotFound(format!("account {}", id.raw()))),
    }
}

/// The live context with this id, or `NotFound`.
fn live_context(db: &Db, id: ContextId) -> Result<()> {
    match db.context(id) {
        Some(c) if !c.is_deleted => Ok(()),
        _ => Err(EngineError::NotFound(format!("context {}", id.raw()))),
    }
}

/// The live obligation with this id, or `NotFound`.
fn live_obligation(db: &Db, id: ObligationId) -> Result<&Obligation> {
    db.obligation(id)
        .filter(|o| !o.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("obligation {}", id.raw())))
}

/// "Add Obligation". The name is trimmed and required. Status is Funding and both timestamps are
/// now. `ContextChoice::New(n)` inserts a context named `n` (notes "{name} obligation") in the same
/// step; a blank `n` means no context.
pub fn create_obligation(e: &mut Engine, input: NewObligation) -> Result<ObligationId> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Validation(
            "Obligation name is required".into(),
        ));
    }
    live_account(e.db(), input.account)?;
    let new_context = match &input.context {
        ContextChoice::New(n) => Some(n.trim().to_string()).filter(|n| !n.is_empty()),
        _ => None,
    };
    let existing = match input.context {
        ContextChoice::Existing(id) => {
            live_context(e.db(), id)?;
            Some(id)
        }
        _ => None,
    };
    let now = Timestamp::now();
    e.apply("Add Obligation", |w| {
        let context_id = match &new_context {
            Some(context_name) => {
                let mut c = Context::new(ContextId(0), context_name);
                c.notes = Some(format!("{name} obligation"));
                Some(w.insert_context(c))
            }
            None => existing,
        };
        let mut o = Obligation::new(ObligationId(0), &name);
        o.target_amount = input.target_amount;
        o.current_balance = input.current_balance;
        o.account_id = Some(input.account);
        o.context_id = context_id;
        o.created_date = Some(now);
        o.last_updated_date = Some(now);
        Ok(w.insert_obligation(o))
    })
}

/// R-OBL-4 suggestion for the new-context field: the obligation name turned into a context name,
/// de-duplicated against the live contexts.
pub fn suggest_context_name(db: &Db, obligation_name: &str) -> String {
    let names: Vec<&str> = db.live_contexts().map(|c| c.name.as_str()).collect();
    generate_context_name(obligation_name, &names)
}

/// "Edit Obligation". Status, target and balance changes also set `last_updated_date` to now
/// (Swift `onChange`). Setting a field to the value it already has makes no step. Name and notes
/// are trimmed as in the create form. `NotFound` if the obligation, account or context is missing.
pub fn edit_obligation(e: &mut Engine, id: ObligationId, field: ObligationField) -> Result<()> {
    let current = live_obligation(e.db(), id)?;
    let stamps = matches!(
        field,
        ObligationField::Status(_) | ObligationField::Target(_) | ObligationField::Balance(_)
    );
    let unchanged = match &field {
        ObligationField::Status(s) => current.status == *s,
        ObligationField::Target(t) => current.target_amount == *t,
        ObligationField::Balance(b) => current.current_balance == *b,
        _ => false,
    };
    if unchanged {
        return Ok(());
    }
    let field = match field {
        ObligationField::Name(n) => {
            let n = n.trim().to_string();
            if n.is_empty() {
                return Err(EngineError::Validation(
                    "Obligation name is required".into(),
                ));
            }
            ObligationField::Name(n)
        }
        ObligationField::Notes(n) => {
            ObligationField::Notes(n.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
        }
        ObligationField::Account(Some(a)) => {
            live_account(e.db(), a)?;
            ObligationField::Account(Some(a))
        }
        ObligationField::Context(Some(c)) => {
            live_context(e.db(), c)?;
            ObligationField::Context(Some(c))
        }
        other => other,
    };
    let now = Timestamp::now();
    e.apply("Edit Obligation", |w| {
        w.update_obligation(id, |o| {
            match &field {
                ObligationField::Name(n) => o.name = n.clone(),
                ObligationField::Status(s) => o.status = *s,
                ObligationField::Target(t) => o.target_amount = *t,
                ObligationField::Balance(b) => o.current_balance = *b,
                ObligationField::DueDate(d) => o.due_date = *d,
                ObligationField::Notes(n) => o.notes = n.clone(),
                ObligationField::Account(a) => o.account_id = *a,
                ObligationField::Context(c) => o.context_id = *c,
            }
            if stamps {
                o.last_updated_date = Some(now);
            }
        })
    })
}

/// "Delete Obligation" / "Delete Obligations" (by count). Duplicate ids are ignored; an empty list
/// makes no step. `NotFound` if any id is not a live obligation.
pub fn delete_obligations(e: &mut Engine, ids: &[ObligationId]) -> Result<()> {
    let mut unique: Vec<ObligationId> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(*id);
        }
    }
    if unique.is_empty() {
        return Ok(());
    }
    for id in &unique {
        live_obligation(e.db(), *id)?;
    }
    let step = if unique.len() > 1 {
        "Delete Obligations"
    } else {
        "Delete Obligation"
    };
    e.apply(step, |w| {
        for id in unique {
            w.set_deleted(RowKey::Obligation(id), true)?;
        }
        Ok(())
    })
}

/// R-OBL-5: live obligations grouped by status in `ObligationStatus::ALL` order, names sorted.
pub fn obligations_by_status(db: &Db) -> Vec<(ObligationStatus, Vec<ObligationId>)> {
    core::obligations_by_status(db)
}

/// R-OBL-6: the balance the obligations list and detail show, which is Reality (no scenario)
/// regardless of the active scenario. Zero for an unknown or deleted obligation.
pub fn display_balance(db: &Db, id: ObligationId) -> Decimal {
    db.obligation(id)
        .filter(|o| !o.is_deleted)
        .map_or(Decimal::ZERO, |o| core::computed_balance(db, o, None))
}
