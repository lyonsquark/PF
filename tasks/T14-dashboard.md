# T14: Dashboard screen

**Phase:** 3 · **Deps:** T13 · **Crate(s):** `pf-ui` · **Size:** medium (≈300 lines + tests)

## Goal

Implement `screens/dashboard.rs`: four summary cards, the "Show Available Balance" toggle, and the
account groups with Cleared / All balance columns (plus Reality sub-lines when a scenario is active).
Clicking an account opens its detail screen. This is the app's home screen, and it must stay instant with 100k transactions.

> If a command or pf-core function name differs from what is written here, use the actual one from
> `pf-core` / `pf-engine`; do not add new engine behaviour here.

## Read first

- `tasks/T13-ui-shell.md` (UiCtx, Cached, Dialog::Confirm, widgets::colors, money_label)
- Rules: R-BAL-1, R-BAL-2, R-BAL-3, R-BAL-5, R-BAL-6, R-BAL-9, R-OBL-2, R-ACC-1
- Swift: `../PFApp/Sources/PFApp/Views/DashboardView.swift` (all of it)
- pf-core balance and obligation APIs from T03 (`crates/pf-core/src/balance.rs`, `obligation.rs`); read their real names

## Deliverables

`crates/pf-ui/src/screens/dashboard.rs`:

```rust
#[derive(Default)]
pub struct State {
    pub show_available: bool,                         // the toggle; UI-only, not persisted
    data: Cached<(Option<ScenarioId>, bool), DashboardData>,
}

/// Everything the screen draws, computed in ONE pass (cache it).
pub struct DashboardData {
    pub net_worth: Decimal, pub assets: Decimal, pub credit_debt: Decimal, pub loan_debt: Decimal,   // R-BAL-5 (cleared, adjusted by R-BAL-6 if toggle on)
    pub groups: Vec<AccountGroup>,                    // R-BAL-9 order, empty groups omitted
}
pub struct AccountGroup { pub title: &'static str, pub rows: Vec<AccountRow> }
pub struct AccountRow {
    pub id: AccountId, pub name: String, pub is_external: bool,
    pub cleared: Decimal, pub all: Decimal,           // visible under active scenario, minus reserved if toggle on (R-BAL-6)
    pub reality_cleared: Decimal, pub reality_all: Decimal,   // R-BAL-3 (never adjusted)
}

/// Pure: builds DashboardData from the Db (unit-testable without egui).
pub fn build_data(db: &Db, active: Option<ScenarioId>, show_available: bool) -> DashboardData;

pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
```

`build_data` must call pf-core (e.g. one `pf_core::balance::…` pass over all transactions returning per-account
cleared/all/reality maps, and `pf_core::obligation::total_reserved(db, account, active)`), not re-implement the rules.
Deleted accounts are excluded. Rows are sorted by name.

## Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│ ┌ NET WORTH ───────────────────┐  ┌ ASSETS ───────────────────────┐ │
│ │ $12,345.67   (large, GREEN)  │  │ $20,000.00   (large, BLUE)    │ │
│ └──────────────────────────────┘  └───────────────────────────────┘ │
│ ┌ CREDIT DEBT ─────────────────┐  ┌ LOAN DEBT ────────────────────┐ │
│ │ -$1,200.00   (RED)           │  │ -$150,000.00 (ORANGE)         │ │
│ └──────────────────────────────┘  └───────────────────────────────┘ │
│ ┌──────────────────────────────────────────────────────────────────┐│
│ │ [ ] Show Available Balance (deduct obligations)                  ││
│ └──────────────────────────────────────────────────────────────────┘│
│ Cash & Checking                                                     │
│ ┌──────────────────────────────────────────────────────────────────┐│
│ │                                         Cleared          All     ││  small weak header, right-aligned, 100 px columns
│ │ Checking                             $1,000.00     $1,200.00     ││  "All" ORANGE when all ≠ cleared
│ │                                        $900.00     $1,100.00     ││  ← reality line, small weak text, ONLY when a scenario is active
│ │ 🌐 Joint (external)                    $50.00        $50.00      ││
│ └──────────────────────────────────────────────────────────────────┘│
│ Savings … Credit Cards … Loans … Investments …                      │
└─────────────────────────────────────────────────────────────────────┘
```

- Wrap in `egui::ScrollArea::vertical()`; padding 24 px; 16 px spacing between cards.
- Cards: `egui::Frame::group(ui.style())` inside `ui.columns(2, …)`. Title uppercase, weak, small; value
  `RichText::new(money).size(28.0).strong().color(…)`. Colours from `widgets::colors` (GREEN, BLUE, RED, ORANGE).
  Card titles exactly: "NET WORTH", "ASSETS", "CREDIT DEBT", "LOAN DEBT".
- Toggle: `ui.checkbox(&mut st.show_available, "Show Available Balance (deduct obligations)")` in a group frame.
- Group titles exactly: "Cash & Checking", "Savings", "Credit Cards", "Loans", "Investments" (R-BAL-9), strong + weak colour.
- Each account row is one clickable area: use `ui.add(egui::Button::new(name).frame(false))` for the name, or
  `ui.interact(row_rect, id, Sense::click())`. **Click ⇒ `*cx.screen = Screen::Account(id)`.** The name must be a findable label.
- External accounts prefixed with "🌐 " (same glyph rule as the sidebar).
- Row context menu: **"Delete"** ⇒ `Dialog::Confirm { title: "Delete Account", message: "Are you sure you want to delete '{name}'? This action cannot be undone and will delete all associated transactions.", confirm_label: "Delete", action: ConfirmAction::DeleteAccount(id) }`.
  (The text is Swift's exactly, although Rust can undo it.)
- Money via `widgets::money_label` / `pf_core` formatter: `$1,234.56`, `-$1,234.56`.
- Swift's "Updating…" capsule is cosmetic: **skip it**.

## Steps

1. Write `build_data` and its unit tests first (`#[cfg(test)] mod tests` in `dashboard.rs`), using `pf_model::TestDb`.
2. Write `show` using `st.data.get(cx.engine.revision(), &(active, st.show_available), || build_data(...))`.
   Clone what you need out of the cached value before mutating `cx` (or collect click actions into a local
   `Option<Action>` and apply them after drawing).
3. kittest tests in `crates/pf-ui/tests/ui_dashboard.rs`.
4. Manual check with `cargo run -p pf` on an imported backup.

## Tests (required)

Unit (`build_data`):

| Test name | Covers | What it checks |
|---|---|---|
| `r_bal_5_summary_cards_use_cleared_non_external` | R-BAL-5 | pending tx excluded; external account excluded from all four cards; assets = checking+savings+cash+investment |
| `r_bal_9_groups_order_and_empty_hidden` | R-BAL-9 | only non-empty groups, in the fixed order; cash goes in "Cash & Checking" |
| `r_bal_3_reality_columns_ignore_scenario` | R-BAL-3 | with an active scenario tx, `all` changes but `reality_all` does not |
| `r_bal_6_available_subtracts_reserved` | R-BAL-6, R-OBL-2 | Funding obligation with balance 300 ⇒ cleared and all reduced by 300, cards too; Completed obligation ignored |
| `dashboard_excludes_deleted_accounts` | R-ACC-1 | soft-deleted account absent |

kittest (`tests/ui_dashboard.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `ui_nav_02_click_account_opens_detail` | UI-NAV-02 | click the account name in the dashboard (not the sidebar: use `query_all_by_label` and pick the central one, or give the dashboard row a distinct hover text) ⇒ `Screen::Account(id)` |
| `ui_dsh_01_available_toggle_changes_balances` | UI-DSH-01 | label for the reduced amount absent before; tick "Show Available Balance (deduct obligations)" ⇒ the reduced money text is present |
| `ui_scn_01_dashboard_numbers_follow_scenario` | UI-SCN-01 | switch to a scenario via the scenario menu ⇒ the "All" money text for the account changes and a reality line appears; back to Reality ⇒ reality line gone |
| `ui_dashboard_delete_account_via_confirm` | R-ACC-1 | open the confirm (context menu or test hook) ⇒ "Delete" ⇒ account gone; Edit → Undo restores |
| `ui_dashboard_empty_db_renders` | – | empty engine renders four cards showing "$0.00" and no groups, no panic |

**Accessible labels:** card titles and values (plain labels), the toggle (checkbox text), account names (button text).

## Pitfalls

- Do **not** iterate transactions inside `show` every frame; use the `Cached` value keyed on `(revision, (active, toggle))`.
- `ui.columns` gives each closure its own `Ui`; keep card drawing in a small helper `fn card(ui, title, value, color)`.
- Money is `Decimal`; only `money_label` formats it. No `f64`.
- Account names may repeat in sidebar and dashboard: kittest `get_by_label` panics on multiple matches. Use `query_all_by_label` or a distinct hover text.

## Done when

```bash
cargo test -p pf-ui dashboard
cargo test -p pf-ui --test ui_dashboard
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- Account detail (T15a). Editing accounts (T16). Charts (T24a).
