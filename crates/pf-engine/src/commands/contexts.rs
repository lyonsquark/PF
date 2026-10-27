//! Context (tag) commands (T10b): create, edit, archive and delete. Rules R-CTX-1..4 and
//! BEH-CTX-01..02; the Swift source is `Views/ContextsView.swift` (`deleteContext`, save).

use pf_model::{Context, ContextId, Db, RowKey, TransactionId};

use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// Colour used when none is given (Swift `?? "#0000FF"`), R-CTX-4.
const DEFAULT_COLOR: &str = "#0000FF";

/// Form values for Add and Edit Context.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextInput {
    /// Name; trimmed before saving, must not be empty.
    pub name: String,
    /// `#RRGGBB`; `None` or blank gives the default colour.
    pub color_hex: Option<String>,
    /// Notes; blank is stored as `None`.
    pub notes: Option<String>,
}

/// Trimmed name (required), colour with the default applied, and trimmed notes.
fn normalize(input: ContextInput) -> Result<(String, String, Option<String>)> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Validation("Context name is required".into()));
    }
    let color = input
        .color_hex
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| DEFAULT_COLOR.to_string());
    let notes = input
        .notes
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    Ok((name, color, notes))
}

/// The live context with this id, or `NotFound`.
fn live_context(db: &Db, id: ContextId) -> Result<&Context> {
    db.context(id)
        .filter(|c| !c.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("context {}", id.raw())))
}

/// "Add Context". The name is trimmed and required; a missing colour becomes `#0000FF`.
pub fn create_context(e: &mut Engine, input: ContextInput) -> Result<ContextId> {
    let (name, color, notes) = normalize(input)?;
    e.apply("Add Context", |w| {
        let mut c = Context::new(ContextId(0), &name);
        c.color_hex = Some(color);
        c.notes = notes;
        Ok(w.insert_context(c))
    })
}

/// "Edit Context". Same validation as `create_context`. `NotFound` if the context is not live.
pub fn update_context(e: &mut Engine, id: ContextId, input: ContextInput) -> Result<()> {
    live_context(e.db(), id)?;
    let (name, color, notes) = normalize(input)?;
    e.apply("Edit Context", |w| {
        w.update_context(id, |c| {
            c.name = name;
            c.color_hex = Some(color);
            c.notes = notes;
        })
    })
}

/// "Archive Context" / "Unarchive Context". Setting the flag it already has makes no step.
pub fn set_context_archived(e: &mut Engine, id: ContextId, archived: bool) -> Result<()> {
    if live_context(e.db(), id)?.is_archived == archived {
        return Ok(());
    }
    let step = if archived {
        "Archive Context"
    } else {
        "Unarchive Context"
    };
    e.apply(step, |w| w.update_context(id, |c| c.is_archived = archived))
}

/// "Delete Context". R-CTX-1: if any live obligation uses it, the delete is refused with the
/// message naming the first such obligation by name. R-CTX-2 otherwise: the context is
/// soft-deleted and its id removed from every live transaction, as one step. Undo restores both,
/// keeping each transaction's list order (whole-row before-images).
pub fn delete_context(e: &mut Engine, id: ContextId) -> Result<()> {
    let ctx_name = live_context(e.db(), id)?.name.clone();
    let blocker = e
        .db()
        .obligations_by_name()
        .into_iter()
        .find(|o| o.context_id == Some(id))
        .map(|o| o.name.clone());
    if let Some(obligation) = blocker {
        return Err(EngineError::Validation(format!(
            "Cannot delete context '#{ctx_name}' because it is currently assigned to the obligation '{obligation}'. Please change the obligation's context or delete the obligation first."
        )));
    }
    let affected: Vec<TransactionId> = e.db().transactions_with_context(id).map(|t| t.id).collect();
    e.apply("Delete Context", |w| {
        w.set_deleted(RowKey::Context(id), true)?;
        for t in affected {
            w.update_transaction(t, |tx| tx.context_ids.retain(|c| *c != id))?;
        }
        Ok(())
    })
}

/// R-CTX-3: the live contexts as (active, archived), each sorted by name (`Db::contexts_by_name`).
pub fn contexts_by_section(db: &Db) -> (Vec<ContextId>, Vec<ContextId>) {
    let (mut active, mut archived) = (Vec::new(), Vec::new());
    for c in db.contexts_by_name() {
        if c.is_archived {
            archived.push(c.id);
        } else {
            active.push(c.id);
        }
    }
    (active, archived)
}
