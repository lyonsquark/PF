# T29: Reconciliation commands (`pf-engine::commands::reconcile`)

**Phase:** 5 · **Deps:** T28 · **Crate(s):** `pf-engine` · **Size:** medium (≈ 400 lines incl. tests)

## Goal

Apply a reconciliation the owner accepted: one undo step that marks the chosen transactions Reconciled,
adds the missing statement lines, optionally adds an adjustment, marks older transactions on a first
reconciliation, saves payee aliases and remembers the sign reading. **No schema change**: reconciliation is
recorded only as the Reconciled status (plan, "No new data").

New feature, no Swift source. This file and `docs/reconciliation-plan.md` are the truth.

## Read first

- `docs/reconciliation-plan.md`: "Applying", "No new data", "Decisions".
- `tasks/T28-reconcile-core.md` and the code it produced in `crates/pf-core/src/reconcile/`.
- `crates/pf-engine/src/engine.rs` (`apply`, `today`), `writer.rs` (`update_transaction`,
  `insert_transaction`, `insert_import_mapping`, `update_import_mapping`, `set_setting`).
- `crates/pf-engine/src/commands/transactions.rs`: `is_locked`, `change_status` (status rules R-TXN-2..5).
- `crates/pf-core/src/categorize.rs`: `auto_categorize` (R-CAT-1).
- `docs/data-model.md`: settings keys and `ImportMapping.mapping_type`.

## Deliverables

```rust
// crates/pf-engine/src/commands/reconcile.rs  (add `pub mod reconcile;` to commands/mod.rs)
pub struct Apply<'a> {
    pub account: AccountId,
    pub statement: &'a Statement,
    pub report: &'a Report,
    pub choice: &'a Choice,
    pub mark_before: bool,
}
/// "Reconcile Statement". Returns how many transactions became Reconciled (pairs + added + marked).
pub fn apply_reconciliation(e: &mut Engine, a: &Apply<'_>) -> Result<usize>;
/// The aliases R-REC-11 uses for this account (statementPayee + payee mappings).
pub fn payee_aliases(db: &Db, account: AccountId) -> Vec<(String, String)>;
/// The remembered reading for the account (setting R-REC-16), if any.
pub fn preferred_reading(db: &Db, account: AccountId) -> Option<Reading>;
```

## Rules (add to section 14 of `docs/business-rules.md`)

| ID | Rule |
|---|---|
| R-REC-14 | **Apply** ("Reconcile Statement", one step): refused with `Switch to Reality to reconcile.` while a scenario is active. Each chosen pair's transaction must be live, in Reality and in the account, else `Transaction {id} cannot be reconciled.` and nothing changes. Its status becomes Reconciled; with "use the statement amount" its amount becomes the line's amount under the report's reading. Each added line becomes a new Reality transaction in the account: date = line date, amount = line amount under the reading, payee = the alias target for the line's normalized description if one exists, else the description trimmed (or `Statement line` when empty), memo = the description when the payee differs from it, category = R-CAT-1 on the payee, status Reconciled. An adjustment of A ≠ 0 adds a Reconciled transaction dated period end, payee `Reconciliation adjustment`, memo `Statement {start} to {end}` (`YYYY-MM-DD`), amount A, no category. With `mark_before`, every live Reality transaction of the account dated before period start and not Reconciled becomes Reconciled. |
| R-REC-15 | **Aliases**: for each chosen pair whose payee score was below 1.0 and whose transaction payee is not empty, save an `ImportMapping` (type `statementPayee`, source = the line's normalized description, target = the transaction's payee, account = the account), in the same step. If a `statementPayee` mapping with that source already exists for the account, its target is updated instead. Empty normalized descriptions are not saved. |
| R-REC-16 | **Remembered reading**: the step sets setting `reconcileReading.{account id}` to `asPrinted` or `flipped` (the report's reading). `preferred_reading` reads it; any other value is ignored. |

## Steps

1. Add the rows above to `docs/business-rules.md`, section 14. Add the setting key row to `docs/data-model.md`
   (Settings keys table) and `statementPayee` to the list of mapping types there and in the `ImportMapping`
   doc comment in `crates/pf-model/src/entities.rs`.
2. Write the tests (below) with `pf_engine::testing::World` and `TestDb`, then the command.
3. Validate everything before `e.apply` so a refused apply creates no step. Inside one `e.apply` closure:
   statuses and amounts, inserts, `mark_before`, aliases, setting.
4. `payee_aliases`: `statementPayee` mappings of the account (source as stored) plus all `payee` mappings
   (source passed through `normalize_description`). Order: statementPayee first, then payee, each by id.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_rec_14_marks_pairs_and_adds_lines` | R-REC-14 | statuses, the use-statement-amount pair, an added line's fields (payee from alias and from description, memo, category, status) |
| `r_rec_14_one_undo_step` | R-REC-14, R-UND-1 | undo restores every row, mapping and setting; redo re-applies; undo name is `Reconcile Statement` |
| `r_rec_14_refusals` | R-REC-14 | scenario active, a deleted pair, a pair in another account: exact messages and no step |
| `r_rec_14_adjustment_and_mark_before` | R-REC-14 | adjustment fields; `mark_before` marks only older, live, Reality, unreconciled rows of the account |
| `r_rec_15_aliases_saved_and_updated` | R-REC-15 | new alias; existing alias updated; score 1.0 and empty payee not saved |
| `r_rec_16_reading_remembered` | R-REC-16 | the setting is written; `preferred_reading` reads it; garbage ignored |
| `beh_rec_01_reconcile_then_next_statement` | R-REC-13, R-REC-14 | after reconciling September, October's opening difference is 0 and `first_time` is false |

Add BEH-REC-01 to the behaviour scenarios table in `docs/testing.md` (task T29).

## Pitfalls

- Use the report's `amounts` (already read with the chosen reading), never the raw line amounts.
- `change_status` is per transaction and makes one step each: do not call it in a loop. Write statuses
  directly with `w.update_transaction` inside the single `apply`.
- New transactions need `account_id`, `scenario_id: None`; get ids from `insert_transaction`.
- `auto_categorize` takes `&Db`; compute categories before entering the closure (the writer borrows the Db).

## Done when

```bash
cargo test -p pf-engine reconcile
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- UI and the warning on editing a Reconciled transaction (T30). API and proposals (T31).
