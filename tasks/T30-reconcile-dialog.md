# T30: Reconcile dialog and the reconciled-edit warning (`pf-ui`)

**Phase:** 5 · **Deps:** T29 · **Crate(s):** `pf-ui` · **Size:** large (≈ 800 lines incl. tests)

## Goal

An account's **Reconcile…** button loads a statement file (JSON, or CSV plus typed balances), shows the
T28 report for review, lets the owner change pairs, and finishes with the T29 command. Editing or deleting a
Reconciled transaction asks first. After this task the owner can reconcile without any AI (Option A); T31
feeds the same dialog from the skill.

New feature, no Swift source. This file and `docs/reconciliation-plan.md` are the truth.

## Read first

- `docs/reconciliation-plan.md`: "Result sections", "Balance checks", "Signs and statement layouts",
  "Decisions".
- `tasks/T28-reconcile-core.md`, `tasks/T29-reconcile-engine.md` and their code.
- `docs/egui-cheatsheet.md` (egui 0.37 API). Do not guess API names.
- `crates/pf-ui/src/dialogs/mod.rs` (`Dialog`, `show_active`, `ConfirmAction`, `run_confirm`),
  `dialogs/report_drilldown.rs` (table style, widths, `Modal`), `dialogs/csv_wizard.rs` (multi-step dialog
  and the fake file dialog in tests).
- `crates/pf-ui/src/screens/account/mod.rs` (`action_row`: copy how "Import CSV…" is shown in Reality only),
  `screens/account/edit.rs` (`commit`, `commit_status`, delete), `files.rs` (`FileDialogs`, `FakeDialogs`).
- `crates/pf-ui/tests/ui_csv_wizard.rs` (how tests drive a file-picking dialog).

## Deliverables

- `crates/pf-ui/src/dialogs/reconcile.rs` (split into `reconcile/{mod,load,review}.rs` if it passes 600
  lines): `pub struct ReconcileForm` and `pub fn show(cx, ui, form) -> Outcome`.
- `Dialog::Reconcile(Box<ReconcileForm>)` in `dialogs/mod.rs`, with its `show_active` arm.
- `ReconcileForm::new_load(account)` (start at the Load step) and
  `ReconcileForm::with_statement(db, account, statement) -> Result<ReconcileForm, String>` (start at Review;
  T31 uses this).
- "Reconcile…" button in the account action row, Reality only, next to "Import CSV…".
- `ConfirmAction::EditReconciled { id, field }` and `ConfirmAction::DeleteReconciled(Vec<TransactionId>)`
  (or one variant with an enum), used by `edit.rs`.

## Behaviour

**Load step.** "Choose File…" opens `files.pick_file("Choose Statement", &["json", "csv"])`. JSON is parsed
with `parse_statement_json`. For CSV, the step shows four fields first: Period start, Period end (date
pickers), Opening balance, Closing balance (amount fields), then calls `parse_statement_csv`. Any parse or
self-check error is shown in red under the button, word for word, and the step stays open.

**Review step.** Top: `{institution} {account_hint}` and `Statement {start} to {end}`, then the reading line:
`Read as: owed balances, statement signs as printed` (or `asset balances`, `signs flipped`) with a
**Flip signs** button that recomputes the report with the other reading (pass `preferred` = the other one and
recompute; the counts stay visible as `({n} of {m} lines match)`). Then the balance lines:

- `Opening balance: {statement opening}` and, when the opening difference is not zero,
  `PFRust's reconciled balance is {x} (off by {d}).` In red unless `first_time`.
- When `first_time`: a checkbox `Mark earlier transactions in this account as Reconciled` (default off) and
  the note `This is the first reconciliation for this account.`
- `Closing balance: {statement closing}` and **`Difference: {d}`** (green at zero, red otherwise), recomputed
  every frame with `difference(report, choice)`.

Then one collapsible group per non-empty section (R-REC-12), in this order and with these headings:
`Matched ({n})`, `Amount differs ({n})`, `On statement only ({n})`, `In PFRust only, in this period ({n})`,
`Already reconciled ({n})`, `In PFRust only, outside this period ({n})` (the last one collapsed by default).
Rows show date, statement description, amount, and for pairs the PFRust payee and amount. Each pair row has an
**Accept** checkbox (default on); an "Amount differs" row also has `Use statement amount` (default off). Each
"On statement only" row has **Add** (default on). Each "In PFRust only, in this period" row has
**Pair with…**, a menu of the unpaired statement lines with the same sign; choosing one makes a manual pair
(confidence shown as `Manual`). A manual pair can be removed with **Unpair**.

Bottom: **Cancel** and **Finish**. Finish is enabled when Difference is zero. When it is not zero, the button
reads **Finish with Adjustment…** and opens `Dialog::Confirm`-style text: `The statement and PFRust differ by
{d}. Add a "Reconciliation adjustment" transaction for {d}?` with **Add Adjustment** and **Go Back**.
Finish calls `apply_reconciliation`; an error is shown in red and the dialog stays open; success closes it
and shows the toast `Reconciled {n} transactions.`

**Reconciled-edit warning (R-REC-17).** In the account table, a commit that changes the amount or the date
of a Reconciled transaction, and Delete on a selection that contains a Reconciled transaction, open a
confirm: title `Change Reconciled Transaction?` (`Delete Reconciled Transaction?` for delete), message
`This transaction is Reconciled. Change it anyway?` (`{n} of these transactions are Reconciled. Delete them
anyway?` for a multi-delete), buttons **Change** / **Delete** and **Cancel**. Cancel leaves the row as it
was (the edit buffer is dropped). Status changes, payee, memo, category and contexts do not warn: changing
the status away from Reconciled is how a reconciliation is undone by hand.

## Rules and IDs

Add to `docs/business-rules.md` section 14:

| ID | Rule |
|---|---|
| R-REC-17 | Changing the amount or date of a Reconciled transaction, or deleting one, asks first (`This transaction is Reconciled. Change it anyway?`). Other fields and status changes do not ask. |

Add to the UI scenarios table in `docs/testing.md` (task T30):

| ID | Scenario |
|---|---|
| UI-REC-01 | Load a JSON statement through the fake file dialog; Difference shows $0.00; Finish marks the transactions Reconciled as one undo step `Reconcile Statement` |
| UI-REC-02 | Untick one Matched pair: Difference becomes non-zero and the button reads `Finish with Adjustment…`; tick it again: $0.00 |
| UI-REC-03 | Pair an "In PFRust only" transaction with an "On statement only" line by hand; both leave their sections |
| UI-REC-04 | Edit the amount of a Reconciled transaction: the warning appears; Cancel keeps the old amount; Change saves it |

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `ui_rec_01_load_review_finish` | UI-REC-01, R-REC-14 | as UI-REC-01 (fixture `fixtures/statements/checking.json`, copied to a temp path for the fake dialog) |
| `ui_rec_02_untick_pair_changes_difference` | UI-REC-02, R-REC-13 | as UI-REC-02 |
| `ui_rec_03_manual_pair` | UI-REC-03 | as UI-REC-03 |
| `ui_rec_04_reconciled_edit_warning` | UI-REC-04, R-REC-17 | as UI-REC-04, plus delete of a Reconciled row asks; a payee edit does not |
| `ui_rec_csv_needs_balances` | R-REC-2 | the CSV path shows the four fields and a bad row's error |
| `ui_rec_flip_signs` | R-REC-7 | Flip signs changes the reading line and the counts |
| `ui_rec_first_time_checkbox` | R-REC-13, R-REC-14 | the first-time note and checkbox; ticking it marks older rows on Finish |
| `ui_rec_button_reality_only` | R-REC-14 | "Reconcile…" hidden while a scenario is active |

## Pitfalls

- egui duplicate-ID overlay in debug builds: every table needs its own `id_salt`; rows inside loops need
  `push_id`.
- The report must be recomputed only when the reading or the engine revision changes (cache it on the form
  with the revision), not every frame; `difference` is cheap and can run every frame.
- `Choice` holds statement line indexes into `statement.lines`; keep them stable when flipping (only the
  reading changes).
- kittest: plain labels are values, buttons and combos are labels; duplicate texts need `.next()` / `.last()`.
- Keep the dialog within the window at 800 pt wide (see the drill-down width helpers).

## Done when

```bash
cargo test -p pf-ui
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

Manual check for the owner (add a row to `tasks/MANUAL_CHECKS.md`): reconcile one real checking statement
from a CSV export and confirm the Difference reaches $0.00.

## Out of scope

- The API, the in-memory proposal and the "Statement ready to review" banner (T31). The skill (T32).
