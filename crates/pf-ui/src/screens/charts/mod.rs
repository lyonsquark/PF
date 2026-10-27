//! Charts dashboard (T24a, Swift `ChartsDashboardView`): the scenario and account filters, then nine
//! cards in the Swift order. The data comes from `pf_core::charts` (cached per engine revision); the
//! charts are `egui_plot` plots (T24a) or custom painting (T24b).
//!
//! R-CHT-11: the six plot charts open no drill-down. Their plots have no click handler, so a click does
//! nothing; only the custom charts of T24b (donut, waterfall, Sankey) respond to clicks.

mod breakdown;
mod distribution;
mod monthly;
mod networth;
mod sankey;
mod spending;
mod style;
mod waterfall;

use std::collections::BTreeSet;

use egui::{Frame, RichText, Ui};
use jiff::civil::Date;
use pf_core::charts::sankey::Sankey;
use pf_core::charts::waterfall::Waterfall;
use pf_core::charts::{CHART_TITLES, ChartFilter, default_selection};
use pf_model::{AccountId, ScenarioId, TimePeriod, TransactionId};

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::report_drilldown;
use crate::scenario_menu;
use crate::widgets::account_multiselect::account_multiselect;

/// Height of a plot in points.
const PLOT_HEIGHT: f32 = 250.0;

/// The filter key used by every cache: the accounts, the scenario and today.
type FilterKey = (BTreeSet<AccountId>, Option<ScenarioId>, Date);

/// A drill-down a click asked for: the heading and the transactions (opened after the cards are drawn).
pub(crate) type Drill = Option<(String, Vec<TransactionId>)>;

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
    /// The categorical breakdown's period (T24b).
    pub breakdown_period: TimePeriod,
    /// The waterfall's month (`None` is this month).
    pub waterfall_date: Option<Date>,
    /// The money flow's month (`None` is this month).
    pub sankey_date: Option<Date>,
    networth: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::balance::BalancePoint>>,
    comparison: Cached<(FilterKey, Date, Date), pf_core::charts::spending::SpendingComparison>,
    pacing: Cached<(FilterKey, Date), pf_core::charts::spending::BudgetPacing>,
    profit_loss: Cached<(FilterKey, u8), Vec<pf_core::charts::monthly::MonthSummary>>,
    distribution: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::Slice>>,
    trend: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::monthly::MonthSummary>>,
    breakdown: Cached<(FilterKey, TimePeriod), Vec<pf_core::charts::Slice>>,
    waterfall: Cached<(FilterKey, Date), Waterfall>,
    sankey: Cached<(FilterKey, Date), Sankey>,
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
            breakdown_period: TimePeriod::PastMonth,
            waterfall_date: None,
            sankey_date: None,
            networth: Cached::default(),
            comparison: Cached::default(),
            pacing: Cached::default(),
            profit_loss: Cached::default(),
            distribution: Cached::default(),
            trend: Cached::default(),
            breakdown: Cached::default(),
            waterfall: Cached::default(),
            sankey: Cached::default(),
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
    let mut drill: Drill = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, title) in CHART_TITLES.iter().enumerate() {
                ui.add_space(8.0);
                chart_card(ui, title, |ui| match i {
                    0 => networth::card(ui, &*cx, st, &filter),
                    1 => spending::comparison_card(ui, &*cx, st, &filter),
                    2 => spending::pacing_card(ui, &*cx, st, &filter),
                    3 => monthly::profit_loss_card(ui, &*cx, st, &filter),
                    4 => breakdown::card(ui, &*cx, st, &filter),
                    5 => waterfall::card(ui, &*cx, st, &filter, &mut drill),
                    6 => distribution::card(ui, &*cx, st, &filter),
                    7 => monthly::trend_card(ui, &*cx, st, &filter),
                    _ => sankey::card(ui, &*cx, st, &filter, &mut drill),
                });
            }
        });
    // A click asked for a drill-down: open it now that the cards are drawn (R-CHT-5, R-CHT-9).
    if let Some((title, ids)) = drill {
        report_drilldown::open_transactions(cx, title, ids);
    }
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
