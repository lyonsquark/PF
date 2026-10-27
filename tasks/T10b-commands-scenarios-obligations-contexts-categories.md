# T10b: Commands: scenarios, obligations, contexts, categories

**Phase:** 2 · **Deps:** T09, T04 · **Crate(s):** `pf-engine` · **Size:** medium-large (about 400 lines of code plus 450 lines of tests)

## Goal

CRUD and rule-enforcing commands for scenarios, obligations, contexts (tags) and categories,
each one undo step. After this, the Scenarios, Obligations, Contexts and Categories screens
(T18–T20) have everything they call.

## Read first

- `tasks/T09-engine-core.md` (Engine, Writer, World, world fixture facts)
- `docs/business-rules.md`: R-SCN-2, R-SCN-3, §2 (R-OBL-1…5), §5 (R-CAT-1…8), §6 (R-CTX-1…4)
- `docs/testing.md`: L3 rows BEH-SCN-*, BEH-OBL-*, BEH-CTX-*, BEH-CAT-*
- Swift:
  - `Views/ScenariosView.swift` (create with default colour `#800080`, `EditScenarioSheet`, `deleteScenarioSafely`)
  - `Views/ObligationsView.swift` (Save button: new context with notes `"{name} obligation"`; `generateContextName`; `deleteObligations`)
  - `Views/ObligationDetailView.swift` (`lastUpdatedDate = now` when status, target or balance change)
  - `Views/ContextsView.swift` (save with default colour `#0000FF`; `deleteContext`)
  - `Views/CategoriesView.swift` (`saveCategory`, `applyRulesToUncategorized`, delete)
  - `Managers/ObligationManager.swift`
- pf-core from T04: pattern validation (R-CAT-2), auto-categorize matcher (R-CAT-1), context-name generator (R-OBL-4)
- pf-core from T03: obligation computed balance / reserved (R-OBL-1…3)

## Deliverables

`crates/pf-engine/src/commands/{scenarios,obligations,contexts,categories}.rs`, and tests in
`crates/pf-engine/tests/behaviour_{scenarios,obligations,contexts,categories}.rs`.

### scenarios.rs

```rust
pub struct ScenarioInput { pub name: String, pub color_hex: Option<String>, pub notes: Option<String> }
/// "Add Scenario". Trimmed name; empty ⇒ Validation("Scenario name is required").
/// color None ⇒ "#800080". Empty notes ⇒ None.
pub fn create_scenario(e: &mut Engine, input: ScenarioInput) -> Result<ScenarioId>;
/// "Edit Scenario". Same validation.
pub fn update_scenario(e: &mut Engine, id: ScenarioId, input: ScenarioInput) -> Result<()>;
/// "Delete Scenario". R-SCN-3: if active ⇒ e.set_active_scenario(None) first; soft-delete the
/// scenario + all its live transactions in one step.
pub fn delete_scenario(e: &mut Engine, id: ScenarioId) -> Result<()>;
```

### obligations.rs

```rust
pub enum ContextChoice { None, Existing(ContextId), New(String) }   // New(name): trimmed; empty ⇒ treated as None
pub struct NewObligation {
    pub name: String, pub target_amount: Option<Decimal>, pub current_balance: Decimal,
    pub account: AccountId,              // required (Swift disables Save without one)
    pub context: ContextChoice,
}
/// "Add Obligation". name trimmed, empty ⇒ Validation("Obligation name is required").
/// status Funding; created_date = last_updated_date = now (Timestamp::now() is allowed here;
/// make it injectable via Engine if you prefer, but tests must not compare it to a fixed value).
/// ContextChoice::New(n) ⇒ insert Context { name: n, notes: Some("{obligation name} obligation"),
/// color_hex: None } in the same step.
pub fn create_obligation(e: &mut Engine, input: NewObligation) -> Result<ObligationId>;
/// R-OBL-4 suggestion for the "new context" field (delegates to the pf-core T04 function over live contexts).
pub fn suggest_context_name(db: &Db, obligation_name: &str) -> String;

pub enum ObligationField {
    Name(String), Status(ObligationStatus), Target(Option<Decimal>), Balance(Decimal),
    DueDate(Option<Date>), Notes(Option<String>), Account(Option<AccountId>), Context(Option<ContextId>),
}
/// "Edit Obligation". Status/Target/Balance changes also set last_updated_date = now (Swift onChange).
pub fn edit_obligation(e: &mut Engine, id: ObligationId, field: ObligationField) -> Result<()>;
/// "Delete Obligation" / "Delete Obligations".
pub fn delete_obligations(e: &mut Engine, ids: &[ObligationId]) -> Result<()>;
/// R-OBL-5: live obligations grouped by status in ObligationStatus::ALL order, each group sorted by name.
pub fn obligations_by_status(db: &Db) -> Vec<(ObligationStatus, Vec<ObligationId>)>;
```

### contexts.rs

```rust
pub struct ContextInput { pub name: String, pub color_hex: Option<String>, pub notes: Option<String> }
/// "Add Context". Trimmed; empty ⇒ Validation("Context name is required"). color None ⇒ "#0000FF".
pub fn create_context(e: &mut Engine, input: ContextInput) -> Result<ContextId>;
/// "Edit Context".
pub fn update_context(e: &mut Engine, id: ContextId, input: ContextInput) -> Result<()>;
/// "Archive Context" / "Unarchive Context".
pub fn set_context_archived(e: &mut Engine, id: ContextId, archived: bool) -> Result<()>;
/// "Delete Context". R-CTX-1: if any LIVE obligation uses it ⇒ Validation(exact):
/// "Cannot delete context '#{ctx}' because it is currently assigned to the obligation '{ob}'. Please change the obligation's context or delete the obligation first."
/// ({ob} = the first such obligation by name.) R-CTX-2: else soft-delete it AND remove its id from
/// context_ids of every live transaction, one step. Undo puts it back in the same position.
pub fn delete_context(e: &mut Engine, id: ContextId) -> Result<()>;
/// R-CTX-3: (active, archived) live contexts, each sorted by name.
pub fn contexts_by_section(db: &Db) -> (Vec<ContextId>, Vec<ContextId>);
```

### categories.rs

```rust
pub struct CategoryInput {
    pub name: String, pub category_type: CategoryType, pub icon: String, pub notes: String,
    pub patterns_csv: String,            // the comma-separated text field (R-CAT-2)
    pub parent: Option<CategoryId>,
}
pub struct SaveCategoryResult { pub id: CategoryId, pub rejected_patterns: Vec<String> }
/// "Add Category" / "Edit Category". Trim name (empty ⇒ Validation("Category name is required")),
/// notes and icon (empty ⇒ None). Patterns per R-CAT-2 using the T04 validator; invalid ones are
/// dropped and returned in rejected_patterns (the UI shows "• p" lines). parent == self ⇒
/// Validation("A category cannot be its own parent"); a parent that is a descendant ⇒
/// Validation("A category cannot be moved under its own subcategory") (fix: Swift did not check).
pub fn save_category(e: &mut Engine, id: Option<CategoryId>, input: CategoryInput) -> Result<SaveCategoryResult>;
/// "Apply Rules". R-CAT-3: uses the patterns from `patterns_csv` (valid ones only), applied to
/// live, uncategorized transactions with non-empty payee; match = case-insensitive substring OR regex.
/// Returns the count; the message the UI shows is `Applied to {n} uncategorized transaction(s).`
pub fn apply_rules_to_uncategorized(e: &mut Engine, category: CategoryId, patterns_csv: &str) -> Result<usize>;
/// "Delete Category". R-CAT-7: soft-delete the category and all descendants (recursive), and set
/// `category_id = None` on every live transaction pointing at any of them. One undo step restores both.
pub fn delete_category(e: &mut Engine, id: CategoryId) -> Result<()>;
/// R-CAT-8: tree order for the list: (id, depth) pairs, parents before children, siblings by name.
pub fn category_tree(db: &Db) -> Vec<(CategoryId, usize)>;
/// R-CAT-4: live categories sorted by full name, case-insensitive (for pickers).
pub fn categories_for_picker(db: &Db) -> Vec<(CategoryId, String)>;
```

## Steps

1. Write the behaviour tests (table) against `World::with_fixture("world")`.
2. Write the unit tests for the pure helpers (`obligations_by_status`, `contexts_by_section`,
   `category_tree`, `categories_for_picker`, `suggest_context_name`) using `TestDb`.
3. Implement the four modules. One `e.apply` per command.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_scn_3_delete_active_scenario_deactivates` | R-SCN-3 | active None afterwards |
| `r_scn_3_delete_scenario_hides_its_transactions` | R-SCN-3 | Plan A Landlord row is_deleted |
| `r_obl_1_computed_balance_world` | R-OBL-1 | Property Tax == 700 (Reality) |
| `r_obl_1_scenario_tagged_tx_counts_only_when_active` | R-OBL-1 | add a Plan A tx tagged Taxes −100 ⇒ 700 in Reality, 600 with Plan A active |
| `r_scn_4_create_scenario_trims_name_and_empty_notes_none` | R-SCN-4 | create("  Plan B ", notes "") ⇒ name "Plan B", notes None; create("   ") ⇒ Validation, nothing added |
| `r_obl_6_list_balance_uses_reality` | R-OBL-6 | with Plan A active, the obligations-list helper still returns the Reality computed balance (700) |
| `r_obl_2_only_funding_and_spending_reserved` | R-OBL-2 | Checking reserved 700; set Vacation Fund to Spending ⇒ Savings reserved 1350 |
| `r_obl_3_available_balance` | R-OBL-3 | Checking all-status 2051.95 − 700 = 1351.95 |
| `r_obl_4_suggest_context_name_dedupes` | R-OBL-4 | "Property Tax" ⇒ "property-tax"; after creating that context ⇒ "property-tax-2"; "Taxes" ⇒ "taxes-2" (existing "Taxes", case-insensitive) |
| `r_obl_5_grouped_in_status_order` | R-OBL-5 | Funding group before Completed |
| `r_cat_2_invalid_regex_dropped_and_reported` | R-CAT-2 | `"Coffee, [abc, ^Star.*"` ⇒ patterns ["Coffee", "^Star.*"], rejected ["[abc"] |
| `r_cat_3_apply_rules_counts_only_uncategorized` | R-CAT-3 | Dining with "Cafe" ⇒ 1 ("Cafe Luna"); Starbucks unchanged |
| `r_cat_4_picker_sorted_by_full_name_ci` | R-CAT-4 | world: Dining, Groceries, Rent, Rent/Utilities, Salary, Transfer |
| `r_cat_7_delete_parent_cascades_to_children` | R-CAT-7 | |
| `r_cat_8_tree_order_and_depth` | R-CAT-8 | Rent (0) followed by Utilities (1) |
| `category_cannot_be_own_parent_or_descendant` | – | both messages |
| `r_ctx_1_context_used_by_obligation_cannot_be_deleted` | R-CTX-1 | exact message for Taxes / Property Tax |
| `r_ctx_1_completed_but_live_obligation_still_blocks` | R-CTX-1 | Vacation used by the Completed "Vacation Fund" ⇒ still refused (any live obligation) |
| `r_ctx_2_delete_removes_from_transactions` | R-CTX-2 | (delete Vacation Fund first) delete Vacation ⇒ Hotel Maui has no contexts |
| `r_ctx_3_sections` | R-CTX-3 | archive Taxes ⇒ in the archived section |
| `defaults_colors` | – | scenario #800080, context #0000FF when none is given |

Behaviour:

| Test name | ID | Given / When / Then |
|---|---|---|
| `beh_scn_01_toggle_twice_back_to_reality` | BEH-SCN-01, R-SCN-2 | **Given** world · **When** toggle Plan A twice · **Then** active None and Checking balance 2051.95 |
| `beh_scn_02_scenario_does_not_change_reality` | BEH-SCN-02, R-SCN-1, R-BAL-1, R-BAL-3 | The example in docs/testing.md L3: Plan A active, withdrawal 500 on 2026-04-01 ⇒ balance 51.95, reality 2051.95; Reality ⇒ 2051.95; reload ⇒ same |
| `beh_scn_03_delete_active_scenario_undo` | BEH-SCN-03 | **Given** Plan A active · **When** delete Plan A · **Then** active None, Checking balance 2051.95, the Plan A row is hidden · **When** undo · **Then** the scenario and its row are live (active stays None) |
| `beh_obl_01_reserved_reduces_available` | BEH-OBL-01 | **Given** world · **When** tag "Landlord" 2026-02-01 with Taxes · **Then** Property Tax computed 700 − 1500 = −800; available = 2051.95 − (−800) |
| `beh_obl_02_completed_and_deferred_not_reserved` | BEH-OBL-02 | set Property Tax Deferred ⇒ Checking reserved 0; Completed ⇒ 0; Spending ⇒ 700 |
| `beh_obl_03_new_obligation_creates_unique_context` | BEH-OBL-03 | create "Taxes" obligation with New(suggest("Taxes")) ⇒ context "taxes-2" with notes "Taxes obligation"; one undo removes both |
| `beh_ctx_01_delete_context_used_by_obligation_refused` | BEH-CTX-01 | exact message; nothing changed; no step |
| `beh_ctx_02_delete_context_undo_restores_links` | BEH-CTX-02 | delete Vacation Fund, delete Vacation ⇒ Hotel Maui contexts [] ⇒ undo ⇒ [Vacation] |
| `beh_cat_01_invalid_pattern_dropped_valid_kept` | BEH-CAT-01 | save Dining with `"Starbucks, Cafe, (unclosed"` ⇒ patterns [Starbucks, Cafe], rejected ["(unclosed"] |
| `beh_cat_02_apply_rules_one_undo` | BEH-CAT-02 | apply "Cafe" to Dining ⇒ 1; only Cafe Luna changes; undo ⇒ uncategorized |
| `beh_cat_03_delete_parent_hides_children_undo_restores` | BEH-CAT-03 | delete Rent ⇒ Rent + Utilities hidden; picker lacks both; their transactions now uncategorized; undo ⇒ tree and transaction categories restored |

## Pitfalls

- R-CTX-2: keep the **position** of the context within `context_ids` on undo. Because the Writer
  stores whole-row before-images, this is automatic; do not hand-roll the inverse.
- Deleting a category **does** clear `category_id` on the affected live transactions (R-CAT-7,
  matching Swift `CategoriesView` lines 110–121). Undo must restore each transaction's category, which
  is automatic because the Writer keeps whole-row before-images.
- `delete_scenario` changes `e.state` *before* `apply`. That is fine because state is not undoable;
  undo leaves the scenario inactive.
- The regex crate syntax differs slightly from Swift `Regex`. Use `regex::Regex::new`; the
  "looks like regex" test decides whether a compile failure matters (T04).

## Done when

```bash
cargo test -p pf-engine
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules      # R-SCN-3, R-OBL-*, R-CAT-2,3,4,7,8, R-CTX-*, BEH-SCN/OBL/CTX/CAT-* present
```

## Out of scope

- Budget, projection, CSV, Markdown, backup (T10c). UI (T18–T20).


## Addendum (plan owner)

- R-SCN-4: `create_scenario` trims the name (empty ⇒ `Validation("Scenario name is required")`) and stores empty notes as `None`.
- R-OBL-6: the obligations list/detail balance is `computed_balance(db, ob, None)` (Reality) regardless of the active scenario; expose it as `obligations::display_balance(db, id) -> Decimal`.
