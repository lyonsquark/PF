//! Scenario commands (T10b): create, edit and delete. Rules R-SCN-2..4 and BEH-SCN-01..03; the
//! Swift source is `Views/ScenariosView.swift` (`deleteScenarioSafely`, `EditScenarioSheet`).

use pf_model::{Db, RowKey, Scenario, ScenarioId, TransactionId};

use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// Colour used when none is given (Swift `?? "#800080"`).
const DEFAULT_COLOR: &str = "#800080";

/// Form values for Add and Edit Scenario.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioInput {
    /// Display name; trimmed before saving.
    pub name: String,
    /// `#RRGGBB`; `None` or blank gives the default colour.
    pub color_hex: Option<String>,
    /// Free-form notes; blank is stored as `None`.
    pub notes: Option<String>,
}

/// Trimmed name, colour with the default applied, and trimmed notes (`None` when empty). R-SCN-4.
fn normalize(input: ScenarioInput) -> Result<(String, String, Option<String>)> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Validation("Scenario name is required".into()));
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

/// The live scenario with this id, or `NotFound`.
fn live_scenario(db: &Db, id: ScenarioId) -> Result<&Scenario> {
    db.scenario(id)
        .filter(|s| !s.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("scenario {}", id.raw())))
}

/// "Add Scenario". R-SCN-4: the name is trimmed and must not be empty; a missing colour becomes
/// `#800080`; blank notes become `None`.
pub fn create_scenario(e: &mut Engine, input: ScenarioInput) -> Result<ScenarioId> {
    let (name, color, notes) = normalize(input)?;
    e.apply("Add Scenario", |w| {
        let mut s = Scenario::new(ScenarioId(0), &name);
        s.color_hex = Some(color);
        s.notes = notes;
        Ok(w.insert_scenario(s))
    })
}

/// "Edit Scenario". Same validation as `create_scenario`. `NotFound` if the scenario is not live.
pub fn update_scenario(e: &mut Engine, id: ScenarioId, input: ScenarioInput) -> Result<()> {
    live_scenario(e.db(), id)?;
    let (name, color, notes) = normalize(input)?;
    e.apply("Edit Scenario", |w| {
        w.update_scenario(id, |s| {
            s.name = name;
            s.color_hex = Some(color);
            s.notes = notes;
        })
    })
}

/// "Delete Scenario". R-SCN-3: an active scenario goes back to Reality first (active state is not
/// undoable); then the scenario and its live transactions are soft-deleted in one step. Undo
/// restores them and leaves the scenario inactive. `NotFound` if the scenario is not live.
pub fn delete_scenario(e: &mut Engine, id: ScenarioId) -> Result<()> {
    live_scenario(e.db(), id)?;
    if e.state.active_scenario == Some(id) {
        e.set_active_scenario(None);
    }
    let rows: Vec<TransactionId> = e.db().transactions_for_scenario(id).map(|t| t.id).collect();
    e.apply("Delete Scenario", |w| {
        w.set_deleted(RowKey::Scenario(id), true)?;
        for t in rows {
            w.set_deleted(RowKey::Transaction(t), true)?;
        }
        Ok(())
    })
}
