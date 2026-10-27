# Task status

Statuses: `todo` · `in-progress` · `done` · `blocked`. Keep notes to one to three lines. Newest note goes last.
Pick the lowest-numbered `todo` task whose deps are all `done` (see `PLAN.md`).

| Task | Title | Deps | Status | Notes |
|---|---|---|---|---|
| T00 | Workspace scaffold | – | done | Files created verbatim from T00 spec. build, test, clippy -D warnings and fmt --check all clean with zero warnings. `cargo run -p pf` launched without errors; the visual "Scaffold OK" window check is still for the user to confirm. Nothing committed. |
| T00b | xtask rules / ci | T00 | done | rules.rs and ci.rs added with the 11 spec tests plus one extra (12 pass). Clippy and fmt clean. `rules` prints 0/208 and exits 1, as expected until tests exist. `ci` runs fmt, clippy, tests, then rules. Regexes are LazyLock<Result>, so no expect in non-test code. Note: testing.md says test names match lowercased with - to _; the implementation follows the spec regex, which is more lenient. Nothing committed. |
| T01 | pf-model | T00 | done | ids, enums (serde rename = as_str), entities, record (Record/RowKey/Change), Db, TestDb behind test-support feature. 21 pf-model tests pass; workspace tests, clippy and fmt clean. Db::put bumps next_id past the id so alloc never collides. Db PartialEq includes next_id: BEH-UND-01 must compare contents or account for the counter (T10c). categories_by_name is byte-wise; pickers must sort case-insensitively (R-CAT-4). |
| T02 | Budget projector | T01 | todo | |
| T03 | Balances, visibility, filters, obligations | T01 | todo | |
| T04 | Categorize, colours, formatting | T01 | todo | |
| T05 | Markdown import/export | T01 | todo | |
| T06 | CSV import core | T01, T04 | todo | |
| T07 | pf-store SQLite | T01 | todo | |
| T08 | Reports | T02, T03, T05 | todo | |
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
