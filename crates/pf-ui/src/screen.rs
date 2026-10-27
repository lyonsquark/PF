//! Which screen the central panel shows (Swift `AppScreen`) and the per-screen UI state.

use pf_model::AccountId;

use crate::screens;

/// Which screen the central panel shows. Mirrors Swift `AppScreen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Overview of all accounts.
    Dashboard,
    /// One account's register.
    Account(AccountId),
    /// Budget items and projections.
    Budget,
    /// Obligations (bills, debts, goals).
    Obligations,
    /// Hypothetical plans.
    Scenarios,
    /// Contexts (tags).
    Contexts,
    /// Category tree.
    Categories,
    /// Reports.
    Reports,
    /// Charts dashboard.
    Charts,
    /// Preferences, backup and import.
    Settings,
}

impl Screen {
    /// Title shown in the toolbar. Account screens show the account name; the caller resolves it,
    /// so this returns a generic title for `Account`.
    pub fn title(&self) -> &'static str {
        match self {
            Screen::Dashboard => "Dashboard",
            Screen::Account(_) => "Account",
            Screen::Budget => "Budget",
            Screen::Obligations => "Obligations",
            Screen::Scenarios => "Scenarios",
            Screen::Contexts => "Contexts",
            Screen::Categories => "Categories",
            Screen::Reports => "Reports",
            Screen::Charts => "Charts",
            Screen::Settings => "Settings",
        }
    }

    /// R-SCN UI rule: the scenario menu is hidden on these screens (Swift ContentView toolbar).
    pub fn shows_scenario_menu(&self) -> bool {
        !matches!(
            self,
            Screen::Budget
                | Screen::Categories
                | Screen::Contexts
                | Screen::Settings
                | Screen::Obligations
                | Screen::Charts
        )
    }
}

/// Per-screen UI state. Each screen module owns its `State`.
#[derive(Default)]
pub struct ScreenStates {
    /// Dashboard state.
    pub dashboard: screens::dashboard::State,
    /// Account detail state.
    pub account: screens::account::State,
    /// Budget state.
    pub budget: screens::budget::State,
    /// Obligations state.
    pub obligations: screens::obligations::State,
    /// Scenarios state.
    pub scenarios: screens::scenarios::State,
    /// Contexts state.
    pub contexts: screens::contexts::State,
    /// Categories state.
    pub categories: screens::categories::State,
    /// Reports state.
    pub reports: screens::reports::State,
    /// Charts state.
    pub charts: screens::charts::State,
    /// Settings state.
    pub settings: screens::settings::State,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: R-SCN-2 (UI). The scenario menu is hidden exactly on the six listed screens.
    fn screen_scenario_menu_visibility() {
        let hidden = [
            Screen::Budget,
            Screen::Categories,
            Screen::Contexts,
            Screen::Settings,
            Screen::Obligations,
            Screen::Charts,
        ];
        for s in hidden {
            assert!(!s.shows_scenario_menu(), "{s:?} must hide the menu");
        }
        let shown = [
            Screen::Dashboard,
            Screen::Account(AccountId(1)),
            Screen::Scenarios,
            Screen::Reports,
        ];
        for s in shown {
            assert!(s.shows_scenario_menu(), "{s:?} must show the menu");
        }
    }
}
