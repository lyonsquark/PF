# PFRust

PFRust is a Rust rewrite of the PF personal-finance app (the Swift app in `../PFApp`). It does the same job, with the same
numbers, and it is much faster with large data sets (the whole database is held in memory, so the screens respond
instantly). It uses egui for the window and SQLite for storage. It also keeps the Swift app's local HTTP API and command
line tool working.

The rewrite is checked against the Swift app: `docs/parity-checklist.md` lists every screen and feature, with the result of
each check. Some checks can only be done by looking at the app, so they wait for you to run them.

## Requirements

- macOS 13 or later (Apple Silicon or Intel).
- Rust stable. Install with `brew install rust`, or with [rustup](https://rustup.rs). The toolchain is pinned in
  `rust-toolchain.toml`.
- Xcode command-line tools, for `iconutil` and `codesign`: `xcode-select --install`.

## Build and run

Run these from the `PFRust` folder.

```bash
cargo run -p pf --release        # build and run the app
scripts/build_app.sh             # build PFRust.app in this folder
scripts/build_app.sh --install   # also copy it to /Applications
cargo test --workspace           # run the test suite
```

The app listens on port 8989 for its local API, the same port as the Swift app. Only one of the two can run at a time.
Quit one before starting the other.

## Moving your data from the Swift app

Your Swift data is never changed by PFRust, so you can keep using the Swift app.

1. In the Swift app, export a backup: `curl -s localhost:8989/api/export/markdown > ~/pf-export.md`
   (the Swift app must be running).
2. Quit the Swift app, so that port 8989 is free.
3. Start PFRust. Go to Settings, then Import from Markdown, and choose `~/pf-export.md`.
   Or from the command line, using a database file of your choice:
   ```bash
   cargo run -q -p pf-cli -- --db ~/pf-rust-test.sqlite import-md ~/pf-export.md --yes
   ```
   Importing replaces all the data in the database it is given, so use a new file for a trial run.
4. Check the balances. Start PFRust and compare its Dashboard with the Swift app, or run:
   ```bash
   cargo run -q -p pf-cli -- --db ~/pf-rust-test.sqlite balances
   ```

## Where your data lives

- The database: `~/Library/Application Support/PF/PFRust.sqlite`. SQLite may also create `PFRust.sqlite-wal` and
  `PFRust.sqlite-shm` next to it. To copy the database by hand, quit PFRust first, then copy all three files.
- Backups: choose a folder with Settings, then press Cmd-S. Each backup is a Markdown file named
  `PFBackup_yyyy-MM-dd_HH-mm-ss.md`. Backups never overwrite an earlier file.
- The command line tool can use any database file: `--db PATH`, or the `PF_DB` environment variable.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| Cmd-Z | Undo the last change (the Edit menu names it, for example "Undo Change Payee") |
| Shift-Cmd-Z | Redo |
| Cmd-S | Back up the data to the backup folder |
| Cmd-N | New transaction, on an account's screen |
| Cmd-F | Search, on an account's screen |
| Cmd-\\ | Show or hide the sidebar (also in the View menu) |

The menus are inside the window, not at the top of the screen.

## HTTP API

The API listens on `127.0.0.1:8989` while the app runs. It only answers on the same computer. Its routes and parameters
match the Swift app's API. Set `format` to `json`, `text` or `markdown` (the default is `text`, except where a route says
otherwise). The `scenario` parameter takes a scenario name, or `REALITY` for no scenario.

| Route | Parameters | Example |
| --- | --- | --- |
| `GET /api/db-path` | | `curl localhost:8989/api/db-path` |
| `GET /api/scenarios` | `format` | `curl "localhost:8989/api/scenarios?format=text"` |
| `GET /api/accounts/balances` | `format`, `scenario` | `curl "localhost:8989/api/accounts/balances?format=text"` |
| `GET /api/accounts/{name}/balance` | `format`, `scenario` | `curl "localhost:8989/api/accounts/Main%20Checking/balance?format=text"` |
| `GET /api/accounts/{name}/transactions` | `n` (default 10), `format`, `scenario` | `curl "localhost:8989/api/accounts/Main%20Checking/transactions?n=5&format=text"` |
| `POST /api/accounts/{name}/import` | CSV in the body; uses the account's saved column mappings | `curl -X POST --data-binary @bank.csv localhost:8989/api/accounts/Checking/import` |
| `GET /api/accounts/{name}/transactions` | `from`, `to` (`YYYY-MM-DD`, either optional): every transaction in the range, each with its `id` | `curl "localhost:8989/api/accounts/Checking/transactions?from=2026-02-01&to=2026-02-28"` |
| `POST /api/accounts/{name}/reconcile/proposal` | Statement JSON in the body; replies with the reconciliation report. Nothing is changed until you finish in the app | `curl -X POST --data-binary @statement.json localhost:8989/api/accounts/Checking/reconcile/proposal` |
| `GET` / `DELETE /api/accounts/{name}/reconcile/proposal` | `format` | `curl "localhost:8989/api/accounts/Checking/reconcile/proposal?format=text"` |
| `GET /api/export/markdown` | | `curl -s localhost:8989/api/export/markdown > pf-export.md` |

Names in the path are case-insensitive. Replace spaces with `%20`. An unknown route returns `Not Found: GET /api/...`
with status 404.

## Reconciling a statement

Reconcile an account against a bank or card statement. You download the statement yourself; no program logs in to your bank.

**With Claude Code (the `/reconcile` skill).**

1. Open PFRust and download the statement PDF from your bank.
2. In Claude Code, from the PFRust folder, run `/reconcile path/to/statement.pdf Account Name`.
3. Claude reads the PDF, checks that it adds up, and sends it to PFRust. It explains anything that does not match.
4. In PFRust, open the account. A banner says the statement is ready. Click **Review**, look over the pairs, and press
   **Finish**. Only then are transactions marked Reconciled. One undo reverses the whole reconciliation.

**Without AI (a CSV).** On the account, click **Reconcile…**, choose the statement's CSV (columns `Date`, `Description`,
`Amount`; optional `Post Date`, `Section`, `Reference`), type the period and the opening and closing balances printed on
the statement, and review the same way. A JSON statement file works too (see `docs/statement-schema.json`).

The review shows what matched, what differs, what is only on the statement or only in PFRust, and a live **Difference**
that must reach $0.00 (or you can add a reconciliation adjustment). The first time you reconcile an account you can also
mark all earlier transactions as Reconciled.

## Command line tool

`pf-cli` works on any database file, and it refuses to run while the app holds port 8989. Run it with
`cargo run -q -p pf-cli -- ...`:

```
Usage: pf-cli [--db PATH] <command> [options]

Commands:
  db-path                                   print the database path
  balances [--scenario NAME] [--format F]   all account balances
  balance ACCOUNT [--scenario NAME] [--format F]
  transactions ACCOUNT [-n N] [--scenario NAME] [--format F]   (N default 10)
  scenarios [--format F]
  export-md [--out FILE]                    Markdown backup to stdout or FILE
  import-md FILE --yes                      REPLACE all data with a Markdown backup
  import-csv ACCOUNT FILE [--format F]      import using the account's saved column mappings
  backup FOLDER                             write PFBackup_<timestamp>.md into FOLDER
  purge                                     open the DB (which purges soft-deleted rows) and exit
  help | --help | -h

F = json | text | markdown (default text). --db defaults to $PF_DB, then the app database.
```

## Differences from the Swift app

Most behaviour is the same. These are the places where PFRust does something different on purpose. The rules marked
"(fix)" in `docs/business-rules.md` are the ones that were deliberately changed; the others are presentation differences:

- **Scenario toggle** compares scenarios by identity, not by name, so two scenarios with the same name no longer clash.
- **New scenario** trims the name before checking it is not empty. Empty notes are stored as nothing, not as an empty text.
- **Delete All** on Orphaned Transactions is undoable. The Swift app deleted them for good.
- **CSV duplicates** are checked against live transactions only. The Swift app also counted deleted ones.
- **CSV import** is one undo step, named "Import CSV". The Swift app could not undo it.
- **Attachments** are checked by file type before they open; the list of allowed types is in `R-TXN-14`.
- **Menus** are drawn inside the app window. The Swift app used the macOS menu bar.
- **Contexts screen** has the scenario menu, and its counts follow the active scenario. The Swift app hid the menu there. Clicking a context's count opens its transactions in the same drill-down window as the reports; the context editor no longer lists them.
- **Find bar** has Status and Memo scopes. Status finds Pending, Cleared, Reconciled or Future transactions; Memo searches the memo text. The Swift app had neither.
- **Drill-down windows:** clicking a transaction shows it in its account, selected and scrolled to. Typing in an open category list moves to the first match, and Enter picks it. The Swift app did neither.
- **Icons:** the Swift app's SF Symbol icons are not shown; a few are replaced with text.
- **Dashboard summary cards** add up all visible transactions when a scenario is chosen (pending and future rows included), and cleared transactions in Reality. A line under the cards says which.
- **Charts** use their own drawing, not an embedded web view. Some chart details differ slightly from the Swift app.

Anything else found in the parity checklist is listed in `tasks/STATUS.md`.

## For developers and agents

- `PLAN.md`: the porting plan and the migration procedure.
- `CLAUDE.md`: rules for working in this repository.
- `tasks/STATUS.md`: the task list and known deviations.
- `docs/`: the business rules (`business-rules.md`), the data model, the API backup format, the test layers and the egui
  notes.
