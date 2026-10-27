# Task status

Statuses: `todo` · `in-progress` · `done` · `blocked`. Keep notes to one to three lines. Newest note goes last.
Pick the lowest-numbered `todo` task whose deps are all `done` (see `PLAN.md`).

| Task | Title | Deps | Status | Notes |
|---|---|---|---|---|
| T00 | Workspace scaffold | – | done | Files created verbatim from T00 spec. build, test, clippy -D warnings and fmt --check all clean with zero warnings. `cargo run -p pf` launched without errors; the visual "Scaffold OK" window check is still for the user to confirm. Nothing committed. |
| T00b | xtask rules / ci | T00 | done | rules.rs and ci.rs added with the 11 spec tests plus one extra (12 pass). Clippy and fmt clean. `rules` prints 0/208 and exits 1, as expected until tests exist. `ci` runs fmt, clippy, tests, then rules. Regexes are LazyLock<Result>, so no expect in non-test code. Note: testing.md says test names match lowercased with - to _; the implementation follows the spec regex, which is more lenient. Nothing committed. |
| T01 | pf-model | T00 | done | ids, enums (serde rename = as_str), entities, record (Record/RowKey/Change), Db, TestDb behind test-support feature. 21 pf-model tests pass; workspace tests, clippy and fmt clean. Db::put bumps next_id past the id so alloc never collides. Db PartialEq includes next_id: BEH-UND-01 must compare contents or account for the counter (T10c). categories_by_name is byte-wise; pickers must sort case-insensitively (R-CAT-4). |
| T02 | Budget projector | T01 | done | pf-core projector: generate_dates, projected_amount, projected_payee, PROJECTION_MEMO. 24 tests incl. 4 property tests; workspace green. Matches Swift; no discrepancies. Projection dates are offset from the start month, so Jan 31 gives Mar 31 after Feb 28.
| T03 | Balances, visibility, filters, obligations | T01 | done | pf-core balance, filter and obligation modules. 65 pf-core tests pass (41 new), workspace green. Deviations (deliberate): toggleScenario compares by id, not name (rule marked fix); context search uses live contexts only; archival cutoff is today minus one year. Remaining uncovered IDs R-SCN-3, R-SCN-4, R-OBL-6 belong to T09/T10b/T19, and R-OBL-4 to T04. proptest regressions dir was removed. |
| T04 | Categorize, colours, formatting | T01 | done | pf-core categorize, color, format, names. 97 pf-core tests pass, workspace green. Deviations: picker lists parent categories too (Swift sortedCategories), so the spec example differs; empty patterns are skipped (Swift contains("") is false). Regex crate rejects look-around that Swift accepts; those patterns are flagged invalid (documented in code). format_amount_for_edit of a negative value rounding to zero gives "-0", unverified against NumberFormatter. R-CAT-7 remains for T10b. |
| T05 | Markdown import/export | T01 | done | pf-core markdown (escape, values, export, import). 126 pf-core tests, 30 markdown; workspace green; 6 insta snapshots reviewed. DECIDED by user: (a) keep the spec local-date rule for dates; fixture tests pin America/Chicago. Real backups store local midnight, so only synthetic fixtures shift. (b) keep reverse id assignment so same-date ties keep file order. Swift has no id tie-break; revisit ties only in a later task. R-MD-5, R-MD-7, BEH-MD-03 remain for T10c/T21. Export escapes every string cell (Swift does not), needed for round-trip. Missing trailing pipe with 2 columns is rejected, as in Swift. |
| T06 | CSV import core | T01, T04 | done | pf-core csv_import (parse, columns, lookup, unmapped, translate, plan). 157 pf-core tests pass, workspace green. Deviations from Swift, following the spec: line splitting uses str::lines not Swift newlines; a deleted bank target is skipped (Swift would insert orphans); duplicates check live rows only (Swift API also scans deleted); new tag contexts are deduped within a plan (Swift creates one per row); saved column names match exactly (API trims). BEH-CSV-01/02 remain for T10c. |
| T07 | pf-store SQLite | T01 | done | pf-store: schema, rows, codec, store (load, apply changes, purge). 4 unit + 13 integration tests pass, 1 ignored perf test (100k load, run in release under 500 ms; belongs to T12b). Workspace green. Decisions: purge hard-deletes soft-deleted rows and nulls dangling links per spec (fix); duplicate context ids in one transaction fail the step, so T09 must dedupe before apply. R-UND-1..5 belong to T09. |
| T08 | Reports | T02, T03, T05 | done | pf-core reports: P&L, drill-down, balance sheet, monthly matrix. 187 pf-core tests pass (30 reports), workspace green. Six golden snapshots reviewed by hand, checked against hand computations. Deviations (spec fixes): P&L and matrix ignore deleted categories consistently; matrix year end includes Dec 31 by date. Sorting uses str order, not locale order. Archival quick-select is unreachable in the UI. Snapshots are checked by plain cargo test, not cargo-insta. |
| T08b | Chart data | T02, T03, T05, T08 | todo | |
| T09 | Engine core + undo | T03, T07 | todo | |
| T10a | Commands: accounts, transactions | T09, T04 | todo | |
| T10b | Commands: scenarios, obligations, contexts, categories | T09, T04 | todo | |
| T10c | Commands: budget, projection, import, backup | T09, T02, T05, T06 | todo | |
| T11 | pf-cli | T10c | todo | |
| T12 | pf-api | T10c | todo | |
| T12b | Perf suite | T10c | todo | |
| T13 | UI shell | T10a, T10b, T10c | todo | |
| T14 | Dashboard | T13 | todo | |
| T15a | Account detail + read-only table | T13 | todo | |
| T15b | Table inline editing | T15a | todo | |
| T15c | Table search, context menu, attachments | T15b | todo | |
| T16 | Add Transaction / Account dialogs | T13 | todo | |
| T17 | Budget + projection UI | T13, T10c | todo | |
| T18 | Obligations UI | T13 | todo | |
| T19 | Scenarios UI | T13 | todo | |
| T20 | Categories + Contexts UI | T13 | todo | |
| T21 | Settings UI | T13, T10c | todo | |
| T22 | CSV import wizard UI | T15a, T10c | todo | |
| T23 | Reports UI | T13, T08 | todo | |
| T24a | Charts: dashboard + egui_plot charts | T13, T08b | todo | |
| T24b | Charts: donut, waterfall, Sankey | T24a | todo | |
| T25 | .app bundle script | T13 | todo | |
| T26 | Swift parity capture + tests | T12 | todo | |
| T27 | Parity checklist + user README | all | todo | |
