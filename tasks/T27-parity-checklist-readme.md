# T27: Final parity checklist walkthrough + user README

**Phase:** 4 · **Deps:** all other tasks · **Crate(s):** docs only (plus bug-fix tasks it may spawn) · **Size:** medium (docs + one manual session)

## Goal

Two documents. (1) `docs/parity-checklist.md` is a manual, screen-by-screen walkthrough of every Swift feature in PFRust.
Each item cites its rule/test IDs and has a ✅/❌ column; the agent and then the user run it once. (2) `PFRust/README.md` is
the user-facing guide: build, run, migrate from the Swift app, where data lives, the API and CLI. Any ❌ found becomes a bug
note in `tasks/STATUS.md` (a new row `T27-fix-n`) instead of being fixed silently inside this task.

> Do not add engine or UI behaviour in this task. Use the actual names from T13/pf-engine where the docs mention them.

## Read first

- `PLAN.md` (Migration procedure, Out of scope), `docs/business-rules.md`, `docs/testing.md` (all BEH-/UI- IDs), `docs/data-model.md` (paths, settings)
- Every Swift view under `../PFApp/Sources/PFApp/Views/` (skim them to make sure the checklist misses nothing)
- `tasks/STATUS.md` notes (known deviations recorded by earlier tasks)

## Deliverable 1: `docs/parity-checklist.md`

Format: one table per screen with the columns `# | Check | IDs | Result | Notes`. The draft below is abbreviated: give each
table a proper header row `| # | Check | IDs | Result | Notes |` followed by `| --- | --- | --- | --- | --- |`. The steps use `fixtures/SampleData2.md`
(import it in Settings first) unless a step says otherwise. Start from this draft, complete it against the Swift views, and
keep every row:

```markdown
# Parity checklist (PFRust vs Swift PFApp)
Date: … · PFRust commit: … · Tester: …
Setup: fresh data folder (`PF_DATA_DIR=/tmp/pfcheck cargo run -p pf` if T13 supports an override; otherwise back up and move
`~/Library/Application Support/PF/PFRust.sqlite`), then Settings → Import from Markdown → `fixtures/SampleData2.md`.

## 1. Launch & shell
| # | Check | IDs | Result | Notes |
| 1.1 | App launches in < 1 s; the window title is "PF" | perf budget | | |
| 1.2 | The sidebar has Dashboard, accounts, Budget, Obligations, Scenarios, Reports, Charts, Categories, Contexts, Settings | UI-NAV-01 | | |
| 1.3 | Sidebar grouping A-Z / Institution / Type works and is remembered after restart | R-ACC-4 | | |
| 1.4 | The scenario menu shows "Reality" + scenarios with a ✓; it is hidden on Budget/Categories/Contexts/Settings/Obligations/Charts | UI-SCN-01 | | |
| 1.5 | ⌘Z / ⇧⌘Z undo and redo the last action; the Edit menu shows "Undo {name}" | R-UND-5, UI-UND-01 | | |
| 1.6 | ⌘S backs up (prompts for a folder the first time); toast "Backup saved to: …" | R-MD-7 | | |
| 1.7 | The API answers `curl localhost:8989/api/scenarios` while the app runs | R-API-1 | | |

## 2. Dashboard
| 2.1 | Summary cards Net Worth / Assets / Credit Debt / Loan Debt equal the cleared sums, excluding external accounts | R-BAL-5 | | |
| 2.2 | Account groups in order: Cash & Checking, Savings, Credit Cards, Loans, Investments; empty groups hidden | R-BAL-9 | | |
| 2.3 | "Show Available Balance" subtracts reserved obligations | R-BAL-6, UI-DSH-01 | | |
| 2.4 | With a scenario active, reality balances are shown alongside | R-BAL-3 | | |
| 2.5 | Clicking an account opens its detail | UI-NAV-02 | | |
| 2.6 | Delete account (context menu) → confirm → gone together with its transactions; ⌘Z restores | R-ACC-1 | | |

## 3. Account detail & transactions
| 3.1 | Header shows balance and cleared balance | R-BAL-1, R-BAL-2 | | |
| 3.2 | Time period picker (Past Month … All, Archival) filters rows; scenario rows always visible | R-FLT-1, R-FLT-2 | | |
| 3.3 | The running balance does not change when switching period | R-BAL-7, BEH-TXN-08 | | |
| 3.4 | Inline edit of date/payee/amount/category/memo/contexts; each is one undo step | R-TXN-12, UI-TXN-02 | | |
| 3.5 | Editing a payee on an uncategorised row auto-categorises it | R-TXN-13 | | |
| 3.6 | In scenario mode, reality rows are disabled | R-TXN-1, UI-TXN-03 | | |
| 3.7 | Status menu: Future on reality refused with a message; pending on a scenario row promotes it | R-TXN-2…5 | | |
| 3.8 | Search (⌘F): payee / category / context scopes, date range, search running balance | R-FLT-3…6, R-BAL-8, UI-TXN-04 | | |
| 3.9 | Context menu: Duplicate, Delete, Add to Budget ("Added "…" to Bills" toast) | R-TXN-6, R-TXN-7, R-TXN-11, UI-TXN-05 | | |
| 3.10 | Attachments: attach, open (allowed types only), remove | R-TXN-14 | | |
| 3.11 | 100k-transaction fixture scrolls smoothly | perf | | |

## 4. Add Transaction / Accounts dialogs
| 4.1 | Deposit / Withdrawal force the sign; Transfer creates two rows | R-TXN-8, UI-TXN-01 | | |
| 4.2 | The date cannot be in the future for reality cleared/reconciled rows | R-TXN-9 | | |
| 4.3 | Save & Another keeps account, date, type | R-TXN-10 | | |
| 4.4 | Category auto-fills while typing a payee; the Rec button adds the payee as a pattern | R-CAT-5, R-CAT-6 | | |
| 4.5 | Add Account / Edit Account; starting balance and Calculate | R-ACC-2, R-ACC-3 | | |

## 5. Budget
| 5.1 | Tabs Bills / Expenses / Income | R-BUD-1 | | |
| 5.2 | Editor: frequency incl. Specific Months, end date ≥ start | R-BUD-2, UI-BUD-01 | | |
| 5.3 | Projection dialog: new or existing scenario, date range, item selection, count shown, settings remembered | R-PRJ-6…8, UI-BUD-02 | | |
| 5.4 | Jan-31 monthly bill projects to Feb 28/29, Mar 31 | R-PRJ-3 | | |
| 5.5 | Reproject a single item | R-PRJ-9 | | |

## 6. Obligations
| 6.1 | Grouped by status; rows show balance "of target", due and created dates | R-OBL-5 | | |
| 6.2 | New Obligation auto-generates the context name; Save needs a name and an account | R-OBL-4, UI-OBL-01 | | |
| 6.3 | Detail: status segments, progress bar, related transactions, due date set/remove | R-OBL-1 | | |

## 7. Scenarios
| 7.1 | Click toggles active (orange + ✓); create / edit with colour | R-SCN-2 | | |
| 7.2 | Deleting the active scenario deactivates it and hides its transactions; undo | R-SCN-3 | | |

## 8. Categories & Contexts
| 8.1 | Tree by type with indented children | R-CAT-8, UI-CAT-01 | | |
| 8.2 | Invalid regex patterns removed with an alert; apply rules reports a count | R-CAT-2, R-CAT-3 | | |
| 8.3 | Delete category cascades; undo | R-CAT-7 | | |
| 8.4 | Contexts Active/Archived, archive/unarchive, counts | R-CTX-3, UI-CTX-01 | | |
| 8.5 | Deleting a context used by an obligation shows "Cannot Delete" | R-CTX-1 | | |

## 9. Reports
| 9.1 | P&L rows, Total, colours, quick buttons, drill-downs | R-RPT-1…3, R-RPT-6, UI-RPT-01 | | |
| 9.2 | Balance sheet: accounts, pending, obligations, net worth, available | R-RPT-4 | | |
| 9.3 | Monthly matrix: modes, year, NET row, cell drill-down | R-RPT-5 | | |

## 10. Charts
| 10.1 | All 9 cards render; the account filter works | UI-CHT-01 | | |
| 10.2 | Each chart's numbers match the Swift app for the same month (spot-check 3 values per chart side by side) | R-CHT-1…9 | | |
| 10.3 | Waterfall bar click and Sankey node click open drill-downs | R-CHT-6, R-CHT-9 | | |

## 11. Settings & data
| 11.1 | Backup folder Configured / Not Set; Backup Now | R-MD-7 | | |
| 11.2 | Export → re-import gives identical data (`diff` of two exports is empty) | R-MD-3, UI-SET-01 | | |
| 11.3 | Monarch CSV import wizard | R-CSV-*, UI-CSV-01 | | |
| 11.4 | Bank CSV import from account detail, second import skips duplicates | R-CSV-11, BEH-CSV-01 | | |
| 11.5 | Orphaned transactions list + Delete All | R-TXN-15 | | |
| 11.6 | Delete All Data | — | | |
| 11.7 | Restart: soft-deleted rows purged, undo empty | R-PRG-1, BEH-PRG-01 | | |

## 12. Migration from the real Swift data (user, optional, read-only on the Swift side)
| 12.1 | Swift export → PFRust import: counts per entity match (`pf-cli stats` vs the Swift Markdown sections) | BEH-MD-01 | | |
| 12.2 | `pf-cli balances --format text` equals Swift `curl …/api/accounts/balances?format=text` | R-API-6 | | |
| 12.3 | Same check with each scenario | R-API-4 | | |
```

Rules for the walkthrough:
1. The agent runs everything it can (automated equivalents: `cargo test`, `pf-cli`, `curl`) and fills Result with ✅/❌ plus a note.
   Items that need eyes on the UI are marked "needs user", and the agent asks the user to run them.
2. Every ❌ ⇒ a new `T27-fix-n` row in `tasks/STATUS.md` naming the owning task/crate and the failing ID. Do **not** fix it here.
3. Section 12 is only run by the user on their real data, after they have exported a backup. Never touch `PF.store`.

## Deliverable 2: `PFRust/README.md`

Audience: the app's owner (a Swift developer, new to Rust). Sections, in this order:

1. **What this is**: one paragraph. The Rust/egui rewrite of PFApp, why (speed), and its status (point to `docs/parity-checklist.md`).
2. **Requirements**: macOS 13+, Rust stable (`brew install rust`, or rustup), Xcode command-line tools (for `iconutil`/`codesign`).
3. **Build & run**:
   ```bash
   cd PFRust
   cargo run -p pf --release          # the app
   scripts/build_app.sh --install     # PFRust.app in /Applications (T25)
   cargo test --workspace             # test suite
   ```
4. **Moving your data from the Swift app** (from PLAN.md "Migration procedure", expanded). Export from Swift (Settings → Export
   to Markdown, or `curl -s localhost:8989/api/export/markdown > ~/pf-export.md`), **quit the Swift app** (port 8989), import in
   PFRust (Settings → Import from Markdown, or `pf-cli import-md ~/pf-export.md`), then verify by comparing the balances. The Swift
   app's data is never modified; you can keep using it.
5. **Where your data lives**: `~/Library/Application Support/PF/PFRust.sqlite` (+ `-wal`/`-shm`). Backups: the Cmd-S folder,
   files named `PFBackup_yyyy-MM-dd_HH-mm-ss.md`. How to back up the database file itself (quit the app first).
6. **Keyboard shortcuts**: ⌘Z, ⇧⌘Z, ⌘S, ⌘N, ⌘F (exactly those that T13 implemented; check).
7. **HTTP API** (localhost only, port 8989 or setting `apiPort`): a table of the routes and the `format`/`scenario`/`n`
   parameters with one `curl` example each (from R-API-2…8). Mention that responses match the Swift app.
8. **CLI**: `pf-cli --help` output (paste the real output after T11).
9. **Differences from the Swift app**: every "(fix)" rule from business-rules.md in plain words; the in-window menu bar instead of
   the macOS menu bar; SF Symbol icons not shown; anything recorded in STATUS.md notes.
10. **For developers / agents**: pointers to `PLAN.md`, `CLAUDE.md`, `tasks/STATUS.md`, `docs/`.

Keep it under ~250 lines. All commands must be copy-paste runnable, so test each one.

## Steps

1. Write `README.md`, running every command in it to confirm it works.
2. Write `docs/parity-checklist.md` from the draft, cross-checking each Swift view for missing features (add rows as needed).
3. Run the automated parts of the checklist and fill the results; ask the user for the "needs user" rows.
4. Record any ❌ as `T27-fix-n` rows in STATUS.md.
5. Mark T27 `done` only when every row has a result (✅, ❌ with a fix row, or "needs user" acknowledged by the user).

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `cargo xtask rules` | all IDs | No rule/BEH/UI ID is without a test. This is the final gate |
| `cargo test --workspace` + `cargo test --release -p pf-engine -- --ignored` | perf + all | Everything passes, including perf budgets and parity |
| README commands | — | Each command block runs without error on a clean checkout |

## Pitfalls

- Do not paraphrase CLI or API output from memory. Run the binary and paste.
- The checklist is evidence. Never mark ✅ without having run the step.

## Done when

```bash
cargo xtask ci          # fmt, clippy, tests, rules all green
```
And `docs/parity-checklist.md` has a result in every row, README.md exists and is accurate, and STATUS.md lists any fix rows.

## Out of scope

- Fixing the ❌ items (they become their own tasks). iOS, notarisation.
