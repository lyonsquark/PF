# PF

PF is a personal-finance app for the Mac, written in Rust. It is fast with large data sets (the whole database is held in
memory, so the screens respond instantly). It uses egui for the window and SQLite for storage, and it has a local HTTP API
and a command line tool.

## Requirements

- macOS 13 or later (Apple Silicon or Intel).
- Rust stable. Install with `brew install rust`, or with [rustup](https://rustup.rs). The toolchain is pinned in
  `rust-toolchain.toml`.
- Xcode command-line tools, for `iconutil` and `codesign`: `xcode-select --install`.

## Build and run

Run these from the `PF` folder.

```bash
cargo run -p pf --release        # build and run the app
scripts/build_app.sh             # build PF.app in this folder
scripts/build_app.sh --install   # also copy it to /Applications
cargo test --workspace           # run the test suite
```

The app listens on port 8989 for its local API.

## Restoring a backup

Start PF, go to Settings, then Import from Markdown, and choose a backup file. Or from the command line, with PF closed and
using a database file of your choice:

```bash
cargo run -q -p pf-cli -- --db ~/pf-test.sqlite import-md ~/backup.md --yes
```

Importing replaces all the data in the database it is given, so use a new file for a trial run.

## Where your data lives

- The database: `~/Library/Application Support/PF/PF.sqlite`. Before the rename it was `PFRust.sqlite`; the first start moves it, once any app still running has quit. SQLite may also create `PF.sqlite-wal` and
  `PF.sqlite-shm` next to it. To copy the database by hand, quit PF first, then copy all three files.
- Backups: if your data changed since the newest backup, PF backs up when it exits and shows a small message to say so. You
  can turn this off in Settings. It asks for a folder the first time if none is set. To back up at any other time, press Cmd-S. Each backup is a Markdown file named
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

The API listens on `127.0.0.1:8989` while the app runs. It only answers on the same computer. Set `format` to `json`, `text` or `markdown` (the default is `text`, except where a route says
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

1. Open PF and download the statement PDF from your bank.
2. In Claude Code, from the PF folder, run `/reconcile path/to/statement.pdf Account Name`.
3. Claude reads the PDF, checks that it adds up, and sends it to PF. It explains anything that does not match.
4. In PF, open the account. A banner says the statement is ready. Click **Review**, look over the pairs, and press
   **Finish**. Only then are transactions marked Reconciled. One undo reverses the whole reconciliation.

**Without AI (a CSV).** On the account, click **Reconcile…**, choose the statement's CSV (columns `Date`, `Description`,
`Amount`; optional `Post Date`, `Section`, `Reference`), type the period and the opening and closing balances printed on
the statement, and review the same way. A JSON statement file works too (see `docs/statement-schema.json`).

The review shows what matched, what differs, what is only on the statement or only in PF, and a live **Difference**
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

## For developers and agents

- `PLAN.md`: the plan the app was built from.
- `CLAUDE.md`: rules for working in this repository.
- `tasks/STATUS.md`: the task list and known deviations.
- `docs/`: the business rules (`business-rules.md`), the data model, the API backup format, the test layers and the egui
  notes.
