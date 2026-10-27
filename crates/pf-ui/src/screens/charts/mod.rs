//! Charts dashboard (T24a, Swift `ChartsDashboardView`): the scenario and account filters, then nine
//! cards in the Swift order. The data comes from `pf_core::charts` (cached per engine revision); the
//! charts are `egui_plot` plots (T24a) or custom painting (T24b).
//!
//! R-CHT-11: the six plot charts open no drill-down. Their plots have no click handler, so a click does
//! nothing; only the custom charts of T24b (donut, waterfall, Sankey) respond to clicks.

mod distribution;
mod monthly;
mod networth;
mod spending;
mod style;

use std::collections::BTreeSet;

use egui::{Frame, RichText, Ui};
use jiff::civil::Date;
use pf_core::charts::{CHART_TITLES, ChartFilter, default_selection};
use pf_model::{AccountId, ScenarioId, TimePeriod};

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::scenario_menu;
use crate::widgets::account_multiselect::account_multiselect;
use crate::widgets::colors;

/// Height of a plot in points.
const PLOT_HEIGHT: f32 = 250.0;

/// The filter key used by every cache: the accounts, the scenario and today.
type FilterKey = (BTreeSet<AccountId>, Option<ScenarioId>, Date);

/// Chart state. The selections live here (not in the engine); the caches are keyed by the filter.
pub struct State {
    /// The accounts charted. `None` until the screen is first shown, then the non-external accounts.
    pub selected: Option<BTreeSet<AccountId>>,
    /// The balance chart's period.
    pub networth_period: TimePeriod,
    /// Month A of the comparison (`None` is this month).
    pub compare_a: Option<Date>,
    /// Month B of the comparison (`None` is the month before A).
    pub compare_b: Option<Date>,
    /// The date the budget pacing is measured at (`None` is today).
    pub pacing_date: Option<Date>,
    /// Months shown by the profit and loss chart (`0` is 6).
    pub pl_months: u8,
    /// The expense distribution's period.
    pub distribution_period: TimePeriod,
    /// The income and expense trend's period.
    pub trend_period: TimePeriod,
    networth: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::balance::BalancePoint>>,
    comparison: Cached<(FilterKey, Date, Date), pf_core::charts::spending::SpendingComparison>,
    pacing: Cached<(FilterKey, Date), pf_core::charts::spending::BudgetPacing>,
    profit_loss: Cached<(FilterKey, u8), Vec<pf_core::charts::monthly::MonthSummary>>,
    distribution: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::Slice>>,
    trend: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::monthly::MonthSummary>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            selected: None,
            networth_period: TimePeriod::PastYear,
            compare_a: None,
            compare_b: None,
            pacing_date: None,
            pl_months: 6,
            distribution_period: TimePeriod::PastYear,
            trend_period: TimePeriod::PastYear,
            networth: Cached::default(),
            comparison: Cached::default(),
            pacing: Cached::default(),
            profit_loss: Cached::default(),
            distribution: Cached::default(),
            trend: Cached::default(),
        }
    }
}

/// Draws the Charts screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let accounts: Vec<(AccountId, String)> = cx.caches.lookups(cx.engine).accounts.clone();
    if st.selected.is_none() {
        st.selected = Some(default_selection(cx.engine.db()));
    }
    ui.heading("Charts Dashboard");
    ui.horizontal(|ui| {
        ui.label(RichText::new("Scenario").weak());
        scenario_menu::show(cx, ui);
        ui.label(RichText::new("Accounts").weak());
        let selected = st.selected.get_or_insert_with(BTreeSet::new);
        account_multiselect(ui, cx.engine.db(), &accounts, selected);
    });
    ui.add_space(8.0);
    let filter = ChartFilter {
        accounts: st.selected.clone().unwrap_or_default(),
        scenario: cx.engine.state.active_scenario,
        today: cx.engine.today(),
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let cards: [fn(&mut Ui, &UiCtx, &mut State, &ChartFilter); 6] = [
                networth::card,
                spending::comparison_card,
                spending::pacing_card,
                monthly::profit_loss_card,
                distribution::card,
                monthly::trend_card,
            ];
            let mut cards = cards.into_iter();
            for (i, title) in CHART_TITLES.iter().enumerate() {
                ui.add_space(8.0);
                match i {
                    4 | 5 | 8 => chart_card(ui, title, placeholder),
                    _ => {
                        let draw = cards.next();
                        if let Some(draw) = draw {
                            chart_card(ui, title, |ui| draw(ui, &*cx, st, &filter));
                        }
                    }
                }
            }
        });
}

/// One card: a framed group with the title as its heading.
pub(crate) fn chart_card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    Frame::group(ui.style())
        .corner_radius(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(16.0));
            ui.add_space(4.0);
            add(ui);
        });
}

/// The text of a card that T24b fills in.
fn placeholder(ui: &mut Ui) {
    ui.label(RichText::new("Coming in T24b").color(colors::BLUE));
}
