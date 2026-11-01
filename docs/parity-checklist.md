# Parity checklist (Rust PF vs Swift PFApp)

Date: 2026-10-27 · Rust PF commit: see `git log -1` at the time of the run · Tester: Claude (automated rows), the app's owner (needs user rows)
Setup: `fixtures/SampleData2.md` imported in Settings (Import from Markdown) into a fresh data folder, unless a row says otherwise.

## How to read the results

- **✅ auto**: a passing automated test asserts this rule (`cargo test --workspace` passes, and `cargo xtask rules` covers every ID).
- **✅ run**: the check was run by hand in this session, with the output noted.
- **needs user**: needs someone to look at the app, or it is destructive or uses personal data. Not run.
- **❌**: a failure. Each one gets a `T27-fix-n` row in `tasks/STATUS.md`. None found so far.

The Swift app was not running when the automated rows were checked, so the parity captures are from the earlier Swift session
(see `fixtures/parity/meta.txt`).

## 1. Launch & shell
| #   | Check                                                                                                                          | IDs                | Result     | Notes                                                                                                                                                                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------ | ------------------ | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1.1 | App launches in < 1 s; the window title is "PF"                                                                                | perf budget        | needs user | Launch time is not measured by the tests. Title is set in `apps/pf/src/main.rs`.                                                                                                     |
| 1.2 | The sidebar has Dashboard, accounts, Budget, Obligations, Scenarios, Reports, Charts, Categories, Contexts, Settings           | UI-NAV-01          | needs user |                                                                                                                                                                                      |
| 1.3 | Sidebar grouping A-Z / Institution / Type works and is remembered after restart                                                | R-ACC-4            | needs user | Grouping is UI-only and not saved (see `sidebar.rs`), so "remembered" is expected to be false. Check what Swift does.                                                                |
| 1.4 | The scenario menu shows "Reality" + scenarios with a ✓; it is hidden on Budget/Categories/Contexts/Settings/Obligations/Charts | UI-SCN-01          | needs user |                                                                                                                                                                                      |
| 1.5 | ⌘Z / ⇧⌘Z undo and redo the last action; the Edit menu shows "Undo {name}"                                                      | R-UND-5, UI-UND-01 | needs user | Undo itself is covered by ✅ auto tests.                                                                                                                                              |
| 1.6 | ⌘S backs up (prompts for a folder the first time); toast "Backup saved to: …"                                                  | R-MD-7             | needs user |                                                                                                                                                                                      |
| 1.7 | The API answers `curl localhost:8989/api/scenarios` while the app runs                                                         | R-API-1            | ✅ run      | App started; `/api/db-path`, `/api/scenarios`, `/api/accounts/balances`, one account's balance and transactions, and an unknown route (404 `Not Found: GET /api/nope`) all answered. |

## 2. Dashboard
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 2.1 | Summary cards Net Worth / Assets / Credit Debt / Loan Debt equal the cleared sums, excluding external accounts | R-BAL-5 | ✅ auto | `r_bal_5_summary_cards_use_cleared_non_external` |
| 2.2 | Account groups in order: Cash & Checking, Savings, Credit Cards, Loans, Investments; empty groups hidden | R-BAL-9 | ✅ auto | `r_bal_9_groups_order_and_empty_hidden` |
| 2.3 | "Show Available Balance" subtracts reserved obligations | R-BAL-6, UI-DSH-01 | ✅ auto | `r_bal_6_available_subtracts_reserved`, `ui_dsh_01_available_toggle_changes_balances` |
| 2.4 | With a scenario active, reality balances are shown alongside | R-BAL-3 | ✅ auto | `r_bal_3_reality_columns_ignore_scenario` |
| 2.5 | Clicking an account opens its detail | UI-NAV-02 | ✅ auto | `ui_nav_02_click_account_opens_detail` |
| 2.6 | Delete account (context menu) → confirm → gone together with its transactions; ⌘Z restores | R-ACC-1 | ✅ auto | `ui_dashboard_delete_account_via_confirm`; the dashboard layout itself is needs user. |

## 3. Account detail & transactions
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 3.1 | Header shows balance and cleared balance | R-BAL-1, R-BAL-2 | ✅ auto | `ui_account_header_shows_balance_and_cleared`, `ui_account_cleared_amount_aligns_under_balance` |
| 3.2 | Time period picker (Past Month … All, Archival) filters rows; scenario rows always visible | R-FLT-1, R-FLT-2 | ✅ auto | Rule tests pass; the picker itself needs user. |
| 3.3 | The running balance does not change when switching period | R-BAL-7, BEH-TXN-08 | ✅ auto | BEH-TXN-08 |
| 3.4 | Inline edit of date/payee/amount/category/memo/contexts; each is one undo step | R-TXN-12, UI-TXN-02 | ✅ auto | Edit tests in `ui_account_edit.rs` |
| 3.5 | Editing a payee on an uncategorised row auto-categorises it | R-TXN-13 | ✅ auto | |
| 3.6 | In scenario mode, reality rows are disabled | R-TXN-1, UI-TXN-03 | ✅ auto | |
| 3.7 | Status menu: Future on reality refused with a message; pending on a scenario row promotes it | R-TXN-2…5 | ✅ auto | `ui_status_future_on_reality_shows_toast_no_change`, `ui_status_pending_promotes_scenario_row` |
| 3.8 | Search (⌘F): payee / category / context scopes, date range, search running balance | R-FLT-3…6, R-BAL-8, UI-TXN-04 | ✅ auto | `ui_account_search.rs` |
| 3.9 | Context menu: Duplicate, Delete, Add to Budget ("Added "…" to Bills" toast) | R-TXN-6, R-TXN-7, R-TXN-11, UI-TXN-05 | ✅ auto | `ui_account_menu.rs`; the toast text needs user. |
| 3.10 | Attachments: attach, open (allowed types only), remove | R-TXN-14 | needs user | The file-type rule is tested; opening files and the file picker need user. |
| 3.11 | 100k-transaction fixture scrolls smoothly | perf | ✅ run (engine) / needs user (scroll) | Engine perf budgets: 11 of 11 pass in release. Smooth scrolling is only seen in the app. |

## 4. Add Transaction / Accounts dialogs
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 4.1 | Deposit / Withdrawal force the sign; Transfer creates two rows | R-TXN-8, UI-TXN-01 | ✅ auto | `ui_dialogs.rs` |
| 4.2 | The date cannot be in the future for reality cleared/reconciled rows | R-TXN-9 | ✅ auto | |
| 4.3 | Save & Another keeps account, date, type | R-TXN-10 | ✅ auto | |
| 4.4 | Category auto-fills while typing a payee; the Rec button adds the payee as a pattern | R-CAT-5, R-CAT-6 | needs user | Rules tested; the typing and button behaviour need user. |
| 4.5 | Add Account / Edit Account; starting balance and Calculate | R-ACC-2, R-ACC-3 | ✅ auto | |

## 5. Budget
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 5.1 | Tabs Bills / Expenses / Income | R-BUD-1 | ✅ auto | `ui_budget.rs` |
| 5.2 | Editor: frequency incl. Specific Months, end date ≥ start | R-BUD-2, UI-BUD-01 | ✅ auto | |
| 5.3 | Projection dialog: new or existing scenario, date range, item selection, count shown, settings remembered | R-PRJ-6…8, UI-BUD-02 | needs user | Rules tested; the dialog's remembered settings need user. |
| 5.4 | Jan-31 monthly bill projects to Feb 28/29, Mar 31 | R-PRJ-3 | ✅ auto | |
| 5.5 | Reproject a single item | R-PRJ-9 | ✅ auto | |

## 6. Obligations
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 6.1 | Grouped by status; rows show balance "of target", due and created dates | R-OBL-5 | ✅ auto | `ui_obligations.rs`; the layout needs user. |
| 6.2 | New Obligation auto-generates the context name; Save needs a name and an account | R-OBL-4, UI-OBL-01 | ✅ auto | |
| 6.3 | Detail: status segments, progress bar, related transactions, due date set/remove | R-OBL-1 | needs user | Rule tested; the progress bar and segments need user. |

## 7. Scenarios
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 7.1 | Click toggles active (orange + ✓); create / edit with colour | R-SCN-2 | ✅ auto | `ui_scenarios.rs`; the colours need user. |
| 7.2 | Deleting the active scenario deactivates it and hides its transactions; undo | R-SCN-3 | ✅ auto | |

## 8. Categories & Contexts
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 8.1 | Tree by type with indented children | R-CAT-8, UI-CAT-01 | ✅ auto | `ui_categories.rs`; the indent needs user. |
| 8.2 | Invalid regex patterns removed with an alert; apply rules reports a count | R-CAT-2, R-CAT-3 | ✅ auto | `ui_cat_invalid_regex_alert`, `ui_cat_apply_rules_alert` |
| 8.3 | Delete category cascades; undo | R-CAT-7 | ✅ auto | `ui_cat_delete_cascades_undo` |
| 8.4 | Contexts Active/Archived, archive/unarchive, counts | R-CTX-3, UI-CTX-01 | ✅ auto | `ui_contexts.rs` |
| 8.5 | Deleting a context used by an obligation shows "Cannot Delete" | R-CTX-1 | ✅ auto | |

## 9. Reports
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 9.1 | P&L rows, Total, colours, quick buttons, drill-downs | R-RPT-1…3, R-RPT-6, UI-RPT-01 | ✅ auto | `ui_reports.rs`; the quick buttons' look needs user. |
| 9.2 | Balance sheet: accounts, pending, obligations, net worth, available | R-RPT-4 | ✅ auto | Net Worth includes pending on purpose; a note on the screen says so. |
| 9.3 | Monthly matrix: modes, year, NET row, cell drill-down | R-RPT-5 | ✅ auto | The matrix is a fixed-size grid, not a resizable table (noted in the session). |

## 10. Charts
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 10.1 | All 9 cards render; the account filter works | UI-CHT-01 | ✅ auto | `ui_charts.rs`, `ui_charts_custom.rs`. Covers the Budget vs Actual card (the Swift "Budget Pacing vs Actuals" view, card 3). |
| 10.2 | Each chart's numbers match the Swift app for the same month (spot-check 3 values per chart side by side) | R-CHT-1…9 | needs user | The chart data is tested; a side-by-side check against the Swift charts needs the Swift app. |
| 10.3 | Waterfall bar click and Sankey node click open drill-downs; donut wedge and expense bar clicks too | R-CHT-5, R-CHT-6, R-CHT-7, R-CHT-9 | needs user | Drill contents and hit tests are tested; the clicks themselves need user. |

## 11. Settings & data
| # | Check | IDs | Result | Notes |
| --- | --- | --- | --- | --- |
| 11.1 | Backup folder Configured / Not Set; Backup Now | R-MD-7 | needs user | |
| 11.2 | Export → re-import gives identical data (`diff` of two exports is empty) | R-MD-3, UI-SET-01 | ✅ run | Export of `SampleData2` → import into a new database → export again: `diff` is empty. |
| 11.3 | Monarch CSV import wizard | R-CSV-*, UI-CSV-01 | needs user | Known open finding: Monarch rows whose payee is not renamed are dropped without a message. Decision pending (see `tasks/MANUAL_CHECKS.md`). |
| 11.4 | Bank CSV import from account detail, second import skips duplicates | R-CSV-11, BEH-CSV-01 | ✅ auto | BEH-CSV-01 |
| 11.5 | Orphaned transactions list + Delete All | R-TXN-15 | ✅ auto | `ui_settings.rs` tests; Delete All itself is destructive, so the button is needs user. |
| 11.6 | Delete All Data | — | needs user | Destructive: not run. |
| 11.7 | Restart: soft-deleted rows purged, undo empty | R-PRG-1, BEH-PRG-01 | ✅ auto | BEH-PRG-01 |

## 12. Migration from the real Swift data (user, optional, read-only on the Swift side)
| #    | Check                                                                                                            | IDs       | Result     | Notes                                                                                                                    |
| ---- | ---------------------------------------------------------------------------------------------------------------- | --------- | ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| 12.1 | Swift export → Rust PF import: counts per entity match (the import's summary line vs the Swift Markdown sections) | BEH-MD-01 | needs user | `pf-cli stats` does not exist. Use the summary line that `import-md` prints, and count the sections of the Swift export. |
| 12.2 | `pf-cli balances --format text` equals Swift `curl …/api/accounts/balances?format=text`                          | R-API-6   | needs user | The same check on `SampleData2` passes (parity tests, ✅ auto). Run it on your data after exporting a backup.             |
| 12.3 | Same check with each scenario                                                                                    | R-API-4   | needs user | As 12.2.                                                                                                                 |

## Summary

- Automated rows: every ✅ row above, plus the engine performance budgets (11 of 11) and the export round trip.
- Failures found: none.
- Needs user: rows 1.1–1.6, the visual checks in each section, 3.10, 4.4, 5.3, 6.3, 10.2, 10.3, 11.1, 11.3, 11.6 and all of section 12.
