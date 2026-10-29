# T31: Reconciliation proposals over the local API (`pf-api`, `pf-engine`, `pf-ui`)

**Phase:** 5 · **Deps:** T30 · **Crate(s):** `pf-engine`, `pf-core`, `pf-api`, `pf-ui` · **Size:** medium (≈ 500 lines incl. tests)

## Goal

Let the `/reconcile` skill (T32) hand a statement to the running app: it posts a statement file, gets the
report back as JSON, and the account shows a banner that opens the T30 dialog. **No API route changes a
transaction**; only Finish in the app does (owner decision). Also add date ranges and ids to the existing
transactions route so the skill can look around a discrepancy.

## Read first

- `docs/reconciliation-plan.md`: "Option B" (routes table and the decision), "No new data".
- `docs/business-rules.md` section 13 (R-API-1..10) and `crates/pf-api/src/router.rs` (`handle`, how
  `changed` triggers a repaint), `crates/pf-api/tests/api.rs` (port 0, temp databases).
- `crates/pf-core/src/api_format.rs` (`transaction_lines`, `format_transactions`, the JSON helpers `obj`,
  `num`, `jstr`, `ymd`; keys are sorted).
- `crates/pf-api/tests/parity.rs`: the existing `/transactions` replies must stay byte-identical.
- T28 to T30 task files and code.

## Deliverables

- `crates/pf-engine/src/state.rs` (or `engine.rs`): `pub proposals: BTreeMap<AccountId, Statement>` on the
  engine, in memory only, not undoable, not stored, empty at launch.
- `crates/pf-core/src/api_format.rs`: `format_reconcile_report(account_name, statement, report, f) -> String`
  and the `id` field for ranged transaction lines.
- `crates/pf-api/src/router.rs`: the routes below.
- `crates/pf-ui/src/screens/account/mod.rs`: the banner.

## Rules (add to section 13 of `docs/business-rules.md`; extend R-API-2's route list)

| ID | Rule |
|---|---|
| R-API-11 | `GET /api/accounts/{name}/transactions?from=YYYY-MM-DD&to=YYYY-MM-DD`: when `from` or `to` is given, return every visible transaction of the account with from ≤ date ≤ to (either bound may be absent), sorted as R-API-7, and each JSON transaction gains an `id` key (number; keys stay sorted). `n` is ignored when a bound is given. A bad date is a 400 with `Invalid date: {value}`. Without `from` and `to` the reply is unchanged (R-API-7), so existing clients see the same bytes. |
| R-API-12 | `POST /api/accounts/{name}/reconcile/proposal` with a statement JSON body (R-REC-1): parse, self-check (R-REC-3, R-REC-4) and build the report (R-REC-7 with the account's remembered reading). Any error is a 400 with the message (JSON `{"error": …}` or text `Error: …` per R-API-3). Success stores the statement as the account's pending proposal (replacing an older one), triggers a repaint, and returns the report (R-API-13) with status 200. Nothing else changes. |
| R-API-13 | **Report body** (`format=json`, keys sorted, amounts as JSON numbers): `account`, `period` {`start`,`end`}, `reading` (`asPrinted`/`flipped`), `style` (`asset`/`owed`), `opening`, `closing` (PFRust sign), `reconciledBalance`, `openingDifference`, `firstTime`, `difference` (default choice), and `sections` with `matched`, `amountDiffers`, `statementOnly`, `pfOnlyInPeriod`, `pfOnlyOutside`, `alreadyReconciled`; each entry carries the statement line (`line` index, `date`, `description`, `amount` under the reading) and/or the transaction (`id`, `date`, `payee`, `amount`), plus `confidence` and `dayDistance` for pairs. `format=text` gives a readable summary: one line per check, then each section as an aligned list. |
| R-API-14 | `GET /api/accounts/{name}/reconcile/proposal` returns R-API-13 for the stored proposal, recomputed now; none stored ⇒ 404 `No reconciliation proposal for account '{name}'.` `DELETE` on the same path removes it (200 `Proposal removed.`; 404 as above when none) and triggers a repaint. |
| R-REC-18 | When the account on screen has a pending proposal, a banner under the action row reads `A statement for {start} to {end} is ready to review.` with **Review** (opens the T30 dialog at the Review step with that statement) and **Discard** (removes the proposal). Finish or Cancel in that dialog removes the proposal. The banner is hidden while a scenario is active. |

## Steps

1. Add the rows (and the routes in R-API-2) to `docs/business-rules.md`. Add `proposals` to the "Data access"
   notes in `docs/data-model.md` as in-memory state.
2. API tests first in `crates/pf-api/tests/api.rs` (port 0, temp DB): every row above, including the 400s,
   the 404s and the unchanged `/transactions` reply without bounds. Run `cargo test -p pf-api --test parity`
   to prove the parity replies did not move.
3. Engine field and the `api_format` functions, then the router arms. `POST` and `DELETE` set `changed`.
4. Banner in the account screen; a kittest for Review and Discard.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_api_11_range_with_ids` | R-API-11 | bounds inclusive, one bound only, ids present, bad date 400 |
| `r_api_11_no_bounds_unchanged` | R-API-11, R-API-7 | the reply without bounds is identical to before (snapshot) |
| `r_api_12_post_proposal` | R-API-12, R-API-13 | stores, returns the report JSON (snapshot), repaint flag set, no transaction changed |
| `r_api_12_post_errors` | R-API-12 | bad JSON, does not add up (message with the section hint), unknown account |
| `r_api_14_get_and_delete` | R-API-14 | GET recomputes after an edit; DELETE removes; 404 texts |
| `r_api_13_text_format` | R-API-13 | the text summary (snapshot) |
| `ui_rec_banner_review_and_discard` | R-REC-18 | banner shows for a stored proposal; Review opens the dialog at Review; Discard removes it; hidden in a scenario |

## Pitfalls

- The router holds the engine lock for the whole request (`lock(engine)`); build the report inside it, and
  do not call into the UI.
- Keep JSON keys sorted like the existing replies (`obj` sorts, check).
- R-API-11 must not change any byte of the existing `/transactions` reply: add `id` only when a bound is
  present.
- `proposals` is not part of `Db`; undo, redo and backup must not see it.

## Done when

```bash
cargo test -p pf-api
cargo test -p pf-ui
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- The skill and the JSON schema file (T32). Any route that applies a reconciliation (the owner decided
  against it).
