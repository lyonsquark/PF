# Testing Strategy

PFRust replaces a working app holding real financial data, so the test suite is the
specification. **No task is done until its tests exist and pass.** Each task file
lists the exact tests it must add; this document defines the layers, tools,
conventions and the cross-cutting suites.

## Tools (dev-dependencies, pinned in the workspace `Cargo.toml`)

| Crate | Version | Use |
|---|---|---|
| `proptest` | 1.12 | property tests (projector, markdown round-trip, CSV splitter, undo) |
| `insta` | 1.49 (features `json`) | golden/snapshot files for Markdown export, API responses, report tables |
| `pretty_assertions` | 1.4 | readable diffs in `assert_eq!` |
| `tempfile` | 3.27 | temporary SQLite files and backup folders |
| `assert_cmd` + `predicates` | 2.2 / 3.1 | CLI behaviour tests |
| `egui_kittest` | 0.37 | UI behaviour tests (headless, query widgets by label, click, type) |

Install once: `cargo install cargo-insta` (to review snapshots: `cargo insta review`).
Optional: `cargo install cargo-llvm-cov` for coverage.

## The layers

```
            ┌────────────────────────────────────────────┐
  L6        │ Parity tests vs. the Swift app (golden)     │  fixtures/parity/*
            ├────────────────────────────────────────────┤
  L5        │ UI behaviour tests (egui_kittest)           │  crates/pf-ui/tests/
            ├────────────────────────────────────────────┤
  L4        │ API + CLI integration tests                 │  crates/pf-api/tests/, apps/pf-cli/tests/
            ├────────────────────────────────────────────┤
  L3        │ Engine behaviour tests (Given/When/Then)    │  crates/pf-engine/tests/behaviour_*.rs
            ├────────────────────────────────────────────┤
  L2        │ Store tests (SQLite round-trip, purge)      │  crates/pf-store/tests/
            ├────────────────────────────────────────────┤
  L1        │ Unit + property + golden tests              │  #[cfg(test)] mod tests in pf-model, pf-core
            └────────────────────────────────────────────┘
  Perf      │ #[ignore] benchmarks on 100k transactions   │  crates/pf-engine/tests/perf.rs
```

### L1: unit, property and golden tests (pf-model, pf-core)

* Live next to the code in `#[cfg(test)] mod tests`.
* **Naming:** `fn r_<area>_<n>_<what>()`, e.g. `fn r_prj_3_jan31_clamps_to_feb28()`.
  A test can cover several rules; list the extra IDs in a doc comment:
  `/// Covers: R-BAL-1, R-SCN-1`.
* Each rule in `docs/business-rules.md` needs **at least one positive and one negative/edge case**.
* Build test data with the `TestDb` builder (task T01). Do not hand-assemble structs in every test:
  ```rust
  let mut t = TestDb::new();
  let chk = t.account("Checking", AccountType::Checking);
  let s   = t.scenario("Plan A");
  t.tx(chk, "2026-01-05", "-12.50", "Coffee").status(Cleared).done();
  t.tx(chk, "2026-02-01", "-100", "Rent").scenario(s).status(Future).done();
  let db = t.build();
  ```
  Dates are given as `"YYYY-MM-DD"` strings and amounts as decimal strings. Never use floats.
* **"Today" is injected.** No `pf-core` function reads the clock; they take `today: Date`.
  Tests use a fixed today (`date(2026, 3, 15)`).
* **Time zone is injected** for anything converting timestamps (Markdown); tests use
  `America/New_York` *and* `Asia/Tokyo` to catch off-by-one-day bugs.
* **Property tests** (proptest, 256 cases by default):
  * Projector: output sorted strictly ascending; all dates in `[max(from,start), min(until,end)]`; for month-based frequencies every date's day == `min(start.day, last_day_of_month)`; for weekly, consecutive diffs == 7; SpecificMonths months ⊆ the set; the count never exceeds `months_in_range / inc + 1`.
  * Markdown: `import(export(db)) ≅ db` for generated `Db`s (R-MD-3), and escaping round-trips any string, including `|` and `｜`.
  * CSV: `split_csv_line` never panics; for strings without quotes, the result equals `line.split(',').map(trim)`.
  * Balance: the sum of per-account balances == the sum of visible amounts with an account.
* **Golden tests** (insta): the Markdown export of each fixture after import; each P&L,
  balance sheet and monthly matrix for `fixtures/SampleData2.md`; chart data series.
  Snapshot files are committed. A failing snapshot is a test failure, never something to auto-accept:
  review the diff, and only accept it if the change was intended by the task.

### L2: store tests (pf-store)

`crates/pf-store/tests/store.rs`, using `tempfile::tempdir()`:
* `save_then_load_round_trips_every_entity_type`: every field, including `None`s, Decimals with scale, dates and the context order.
* `schema_creation_is_idempotent`: open twice, no error, version stays 1.
* `apply_changes_is_atomic`: simulate a failing write mid-batch (e.g. inject an invalid row) ⇒ nothing persisted.
* `purge_removes_soft_deleted_and_dangling_links` (R-PRG-1).
* `next_id_survives_restart`.
* `wal_mode_enabled`.

### L3: engine behaviour tests (pf-engine): the heart of the suite

One file per feature: `crates/pf-engine/tests/behaviour_<feature>.rs`. Each test is
a Given/When/Then scenario driven only through the **public engine API**, exactly
as the UI would call it, and checks state via public queries. The helper
`World` (task T10) wraps an `Engine` on a temp SQLite file and offers
`world.reload()` so persistence is checked too.

```rust
#[test]
/// BEH-SCN-02 — Covers: R-SCN-1, R-BAL-1, R-BAL-3
fn scenario_transactions_do_not_change_reality() {
    // Given
    let mut w = World::with_fixture("world");           // fixtures/world.md (defined in T09)
    let chk = w.account("Checking");
    let before = w.balance(chk);
    // When
    w.activate_scenario("Plan A");
    w.add_withdrawal(chk, "2026-04-01", "500", "Vacation");
    // Then
    assert_eq!(w.balance(chk), before - dec!(500));
    assert_eq!(w.reality_balance(chk), before);
    w.activate_reality();
    assert_eq!(w.balance(chk), before);
    w.reload();                                          // persisted?
    assert_eq!(w.balance(chk), before);
}
```

**Required behaviour scenarios.** Each task that owns a feature implements its BEH IDs (MD-01/02 golden behaviour lives in T05):

| ID | Scenario | Task |
|---|---|---|
| BEH-ACC-01 | Create account → appears in sidebar list, sorted; reload keeps it | T10a |
| BEH-ACC-02 | Delete account with transactions and obligations → all hidden; undo restores all; redo hides again | T10a |
| BEH-ACC-03 | Set a starting balance on an account with transactions → dated the day before the earliest; edit again updates the same row | T10a |
| BEH-ACC-04 | "Calculate" starting balance from a known current balance → resulting balance equals the entered value | T10a |
| BEH-TXN-01 | Add deposit/withdrawal → sign forced correctly regardless of the sign typed | T10a |
| BEH-TXN-02 | Transfer → two linked rows, payees "Transfer to/from", balances move both ways; one undo removes both | T10a |
| BEH-TXN-03 | Inline-edit payee on an uncategorized row matching a pattern → category auto-set; undo once reverts the category only; undo again reverts the payee | T10a |
| BEH-TXN-04 | In scenario mode, editing a reality row is refused (Locked) and nothing changes | T10a |
| BEH-TXN-05 | Status rules: future on reality refused; clearing a future-dated scenario row refused; pending on a scenario row promotes it to reality; undo restores scenario and status | T10a |
| BEH-TXN-06 | Duplicate 3 rows → 3 new rows dated today; one undo removes all 3 | T10a |
| BEH-TXN-07 | Delete multiple → one undo step named "Delete Transactions" | T10a |
| BEH-TXN-08 | Running balance stays the same when switching the time window | T10a |
| BEH-SCN-01 | Toggle the same scenario twice → back to Reality | T10b |
| BEH-SCN-02 | Scenario transactions do not change reality (example above) | T10b |
| BEH-SCN-03 | Delete the active scenario → deactivated, its transactions hidden; undo restores | T10b |
| BEH-OBL-01 | Obligation with context + tagged transactions → computed balance; available balance on the dashboard decreases by the reserved total | T10b |
| BEH-OBL-02 | Completed/Deferred obligations are not reserved | T10b |
| BEH-OBL-03 | New obligation with "create new context" → context name generated and de-duplicated | T10b |
| BEH-CTX-01 | Deleting a context used by an obligation is refused with the exact message | T10b |
| BEH-CTX-02 | Delete a context → removed from transactions; undo puts it back on the same transactions | T10b |
| BEH-CAT-01 | Save a category with an invalid regex pattern → pattern dropped, warning returned, valid patterns kept | T10b |
| BEH-CAT-02 | Apply rules to uncategorized → count returned; only uncategorized rows change; one undo | T10b |
| BEH-CAT-03 | Delete a parent category → children hidden too; undo restores the tree | T10b |
| BEH-BUD-01 | Generate projection for 12 months (monthly bill on the 31st, quarterly, specific months) → exact dates and amounts in the new scenario; Reality untouched | T10c |
| BEH-BUD-02 | Projection settings remembered and prefilled | T10c |
| BEH-BUD-03 | Reproject one item → its generated rows for the range are added to the scenario (existing rows untouched); toast count correct; one undo removes them | T10c |
| BEH-BUD-04 | Add-to-budget from a transaction → bill created with the transaction's fields | T10c |
| BEH-UND-01 | 20 random edits, then undo all → the Db equals the initial Db; redo all → equals the final Db (property test) | T10c |
| BEH-UND-02 | New edit after undo clears redo | T10c |
| BEH-PRG-01 | Soft-delete, restart (reload) → row purged and undo stack empty | T10c |
| BEH-CSV-01 | Bank CSV import with a saved column mapping → counts; duplicates skipped on second import; one undo removes the batch | T10c (core logic in T06) |
| BEH-CSV-02 | Monarch CSV → accounts resolved via mappings, tags become contexts (auto-created), unmapped payees skipped unless allowed | T10c (core logic in T06) |
| BEH-MD-01 | Import each fixture → counts per entity match the expected numbers in `fixtures/expected_counts.json` | T05 |
| BEH-MD-02 | Export after import == golden file | T05 |
| BEH-MD-03 | Import a backup in Settings → wipes old data, undo stack cleared | T10c |
| BEH-BAK-01 | Cmd-S backup with folder set → file `PFBackup_*.md` written with the export contents | T10c |
| BEH-REC-01 | Reconcile a September statement, then October's: the second report has opening difference 0 and `first_time` false | T29 |

### L4: API and CLI integration tests

* `crates/pf-api/tests/api.rs`: start the server on port `0` (OS-assigned) on a `World`;
  use a tiny `std::net::TcpStream` HTTP client in `tests/common/mod.rs` (no extra deps).
  One test per route × format (`json`, `text`, `markdown`), plus: unknown route 404,
  account not found 404, unknown scenario error, percent-encoded names, `+` in the query,
  import without column mappings, import success then duplicate import. JSON bodies are
  insta snapshots; text bodies too.
* **Every** test covers its R-API IDs in its doc comment.
* `apps/pf-cli/tests/cli.rs` uses `assert_cmd`: `pf-cli --db <tmp> import-md fixtures/SampleData2.md`,
  `balances --format text`, `transactions Checking -n 5`, `export-md > out.md` (diff vs golden), `import-csv`, `purge`; bad args ⇒ non-zero exit and a usage message.

### L5: UI behaviour tests (egui_kittest)

`crates/pf-ui/tests/ui_*.rs`. The app struct must be constructible headless:
`PfApp::new_for_test(engine: Engine)` (no window, no API server, no file dialogs: the
dialog service is a trait with a fake implementation for tests).

```rust
use egui_kittest::{Harness, kittest::Queryable};
#[test]
/// UI-NAV-01
fn sidebar_navigates_to_budget() {
    let mut app = PfApp::new_for_test(World::with_fixture("world").into_engine());
    let mut h = Harness::new_ui(|ui| app.ui_root(ui));
    h.run();
    h.get_by_label("Budget").click();
    h.run();
    h.get_by_label("Bills");          // tab visible ⇒ Budget screen shown
}
```

Widgets that tests must find need stable accessible labels: use visible text, or
`.on_hover_text()` / `Response::widget_info` for icon buttons.

**Required UI scenarios:**

| ID | Scenario | Task |
|---|---|---|
| UI-NAV-01 | Every sidebar entry opens its screen | T13 |
| UI-NAV-02 | Clicking an account on the dashboard opens its detail | T14 |
| UI-SCN-01 | The scenario menu switches scenario; the dashboard numbers change; the menu is hidden on Budget/Categories/Contexts/Settings/Obligations/Charts | T13 |
| UI-UND-01 | Cmd-Z after deleting a transaction restores the row in the table; the Edit menu label shows "Undo Delete Transaction" | T15c |
| UI-DSH-01 | The available-balance toggle changes the shown balances | T14 |
| UI-TXN-01 | Add Transaction dialog: type amount, payee, Save → row appears; Save & Another keeps the dialog open with the account kept | T16 |
| UI-TXN-02 | Inline payee edit commits on Enter, creating exactly one undo entry | T15b |
| UI-TXN-03 | Locked rows (scenario mode) render disabled widgets | T15b |
| UI-TXN-04 | Search payee filters rows and shows a search running-balance column | T15c |
| UI-TXN-05 | The context menu has Duplicate/Delete/Add to Budget items and they work; status is changed via the row's status picker | T15c |
| UI-BUD-01 | Create a budget item with Specific Months; it appears in its tab | T17 |
| UI-BUD-02 | The projection dialog generates and reports the count | T17 |
| UI-OBL-01 | Create an obligation, change its status, and it moves to the right group | T18 |
| UI-CAT-01 | Create a child category; it is shown indented under its parent | T20 |
| UI-CTX-01 | Archive a context; it moves to the Archived section | T20 |
| UI-SET-01 | Export then import via a fake file dialog; data survives | T21 |
| UI-CSV-01 | Walk the CSV wizard (column mapping → translation → done) with a fake file; the result counts are shown | T22 |
| UI-REC-01 | Load a JSON statement through the fake file dialog; Difference shows $0.00; Finish marks the transactions Reconciled as one undo step `Reconcile Statement` | T30 |
| UI-REC-02 | Untick one Matched pair: Difference becomes non-zero and the button reads `Finish with Adjustment…`; tick it again: $0.00 | T30 |
| UI-REC-03 | Pair an "In PFRust only" transaction with an "On statement only" line by hand; both leave their sections | T30 |
| UI-REC-04 | Edit the amount of a Reconciled transaction: the warning appears; Cancel keeps the old amount; Change saves it | T30 |
| UI-RPT-01 | Each report renders for the fixture; P&L total row present | T23 |
| UI-CHT-01 | The charts dashboard renders all 9 charts without panic for the fixture and for an empty Db | T24b |

UI tests assert on **state through the engine** after interacting (e.g. the
transaction count), not on pixels. No screenshot tests.

### L6: parity with the Swift app

`fixtures/parity/` holds outputs captured **from the running Swift app**, loaded with
`fixtures/SampleData2.md`. Capture with `scripts/capture_swift_parity.sh` (task T26): it
calls every API route/format on `localhost:8989` and saves the bodies. A human runs it once
with the Swift app open. `crates/pf-api/tests/parity.rs` loads the same fixture into PFRust and
compares responses byte-for-byte (JSON compared as parsed values, because Swift's float
formatting of `balance` may differ: compare numbers to within 0.005). The Swift `/api/export/markdown`
output is compared to PFRust's export after normalising the `## Settings` section.
Parity tests are `#[ignore]` until the capture exists; T26 removes the `#[ignore]`.

### Performance tests

`crates/pf-engine/tests/perf.rs`, all `#[ignore]` (run `cargo test --release -p pf-engine -- --ignored`):
generate 100,000 transactions over 10 accounts and 5 years (seeded RNG, no extra crate:
a simple LCG in the test), then assert:
* load from SQLite < 500 ms
* `account_view(account, period, scenario)` (filter + running balance) < 15 ms
* dashboard balances for all accounts < 10 ms
* single edit `apply` including persist < 5 ms
* Markdown export < 1 s; import < 3 s
These budgets are the reason for this rewrite, so they are not optional, but
they are only checked in release builds.

## Commands

```bash
cd PFRust
cargo test --workspace                       # L1–L5 (fast, run constantly)
cargo test -p pf-core r_prj                  # run tests for one rule family
cargo test --release -p pf-engine -- --ignored   # perf
cargo insta review                           # review changed snapshots
cargo xtask rules                            # every R-* and BEH-*/UI-* ID has a test
cargo xtask ci                               # fmt --check, clippy -D warnings, test, rules
```

## Traceability check (`cargo xtask rules`, task T00b)

Scans `docs/business-rules.md` and this file for IDs matching
`R-[A-Z]+-\d+[a-z]?`, `BEH-[A-Z]+-\d+`, `UI-[A-Z]+-\d+`, then scans all `*.rs` under
`crates/` and `apps/` for those IDs in test names (lowercased, `-` → `_`) or comments.
Prints the missing IDs and exits 1 if any are missing. Until T00b is done, agents check by hand with `grep`.

## Rules for writing tests (for agents)

1. Write the test **first** from the rule text, watch it fail, then implement.
2. One behaviour per test; the name says what it checks.
3. Never weaken or delete an existing test to make a change pass. If a test is wrong,
   say so in your task report and fix it in a separate, explained step.
4. Never `#[ignore]` a failing test except the perf and parity tests named above.
5. Never use `f64` comparisons for money; use `dec!()` from `rust_decimal_macros`.
6. Never depend on the real clock, time zone, home directory or network in a test.
7. Tests must run in parallel: no shared temp paths, and the API uses port 0.
