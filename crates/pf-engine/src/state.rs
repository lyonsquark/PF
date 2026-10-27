//! UI-level state: not undoable and not part of `Db`.

use pf_model::{ScenarioId, TimePeriod};

/// The active scenario and the reporting window. Not undoable.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    /// `None` means Reality.
    pub active_scenario: Option<ScenarioId>,
    /// Account-detail time window (persisted as setting `selectedTimePeriod`).
    pub time_period: TimePeriod,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_scenario: None,
            time_period: TimePeriod::PastMonth,
        }
    }
}
