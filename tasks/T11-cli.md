# T11: `pf-cli` command-line tool (+ shared API/CLI output formatters)

**Phase:** 2 · **Deps:** T10c · **Crate(s):** `apps/pf-cli`, `pf-core` (new module `api_format`) · **Size:** medium (about 450 lines of code plus 250 lines of tests)

## Goal

A headless tool over the same engine, for scripting, migration (`import-md`) and for testing
everything without a GUI. It prints balances and transactions in the **same text, markdown and JSON
formats as the Swift HTTP API**. Those formatters are written here, in `pf-core::api_format`, and T12
reuses them unchanged, so the CLI and API can never drift.

## Read first

- `tasks/T09-engine-core.md` (Engine::open, default_db_path, World), `tasks/T10c-…md` (backup, import commands)
- `docs/business-rules.md` §13 (R-API-3, -4, -6, -7: the formats), R-SCN-1, R-BAL-7
- `docs/testing.md` L4 (CLI tests)
- Swift: `Services/APIHandlers.swift`, the whole file: every `format*` function, `resolveScenario`, `calculateBalance`, `jsonError`
- pf-core T04: the currency formatter (`$1,234.56`, `-$1,234.56`)

## Deliverables

### `crates/pf-core/src/api_format.rs` (pure, no IO)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format { Json, Text, Markdown }
/// Case-insensitive: "text" ⇒ Text; "markdown" | "md" ⇒ Markdown; anything else (incl. "json", "") ⇒ Json.
pub fn parse_format(s: &str) -> Format;

/// R-API-4: None, "" or "reality" (ci) ⇒ Ok(None); live scenario by name (ci) ⇒ Ok(Some(id));
/// else Err("Scenario '{name}' not found.").
pub fn resolve_scenario(db: &Db, name: Option<&str>) -> Result<Option<ScenarioId>, String>;
/// Error body: Json ⇒ `{"error":"<msg>"}` (compact, JSON-escaped); Text/Markdown ⇒ `Error: <msg>`.
pub fn error_body(msg: &str, f: Format) -> String;

pub struct BalanceEntry { pub name: String, pub account_type: String, pub institution: String, pub balance: Decimal, pub is_external: bool }
/// Live accounts sorted by name; balance per R-SCN-1/R-BAL-1 (all statuses).
pub fn balance_entries(db: &Db, scenario: Option<ScenarioId>) -> Vec<BalanceEntry>;
pub fn format_balances(entries: &[BalanceEntry], f: Format) -> String;

pub struct TxLine { pub tx: TransactionId, pub date: Date, pub payee: String, pub amount: Decimal,
    pub running_balance: Decimal, pub status: TransactionStatus, pub category: Option<String> /* plain name */,
    pub memo: Option<String>, pub scenario: Option<String>, pub contexts: Vec<String> }
/// R-API-7: visible tx of the account sorted by (date desc, id desc), first n; running balance accumulated
/// over ALL visible tx oldest→newest ((date, id) ascending).
pub fn transaction_lines(db: &Db, account: AccountId, scenario: Option<ScenarioId>, n: usize) -> Vec<TxLine>;
pub fn format_transactions(account_name: &str, lines: &[TxLine], f: Format) -> String;
pub fn format_scenarios(db: &Db, f: Format) -> String;
pub fn format_import_result(account_name: &str, imported: usize, dups: usize, unmapped: usize, f: Format) -> String;
/// Apple-JSONSerialization-style writer (see below). Keep it tiny and local to this module.
pub enum JsonValue { Null, Bool(bool), Num(f64), Str(String), Arr(Vec<JsonValue>), Obj(Vec<(String, JsonValue)>) }
pub fn to_apple_json(v: &JsonValue, pretty: bool, sort_keys: bool) -> String;
```

#### Exact formats (port of APIHandlers.swift; `cur(x)` = T04 currency format, `d(x)` = `YYYY-MM-DD`)

**Balances, Text**: one line per entry, then an empty line, then the total. Total = sum of non-external balances.
```
{name} ({institution}){ " [external]" if external }: {cur(balance)}
…
                                   ← empty line
Total: {cur(total)}
```
**Balances, Markdown**:
```
| Account | Institution | Type | Balance |
| --- | --- | --- | ---: |
| {name, or "{name} *(ext)* " with that trailing space if external} | {institution} | {type} | {cur(balance)} |
| **Total** | | | **{cur(total)}** |
```
**Balances, JSON** (pretty, sorted keys): `{"accounts":[{"balance":num,"external":bool,"institution":str,"name":str,"type":str}…],"total":num}`.
`type` is the raw account type (`checking`). Numbers are `Decimal → f64`.

**Transactions, Text**: `pad(s, n)` = Swift `padding(toLength:)`: pads with spaces **or truncates** to exactly n chars.
```
{account} — Last {count} transactions          ← count = number of lines shown; "—" is U+2014
                                               ← empty line
{pad("Date",10)}  {pad("Amount",12)}  {pad("Balance",12)}  {pad("Status",10)}  Payee
{"-" repeated (header char count + 20)}       ← header is 57 chars ⇒ 77 dashes
{d(date)}  {pad(cur(amount),12)}  {pad(cur(running),12)}  {pad(Status,10)}  {payee}[  [{category}]][  {#ctx1, #ctx2}][  // {memo}][  ({scenario})]
```
`Status` = raw value capitalised (`Cleared`). The contexts part is literally `{#` + names joined by `, #` + `}`.
Memo only when non-empty. Lines are joined with `\n`, no trailing newline.

**Transactions, Markdown**:
```
## {account} — Last {count} Transactions
                                               ← empty line
| Date | Payee | Amount | Balance | Status | Category | Contexts | Memo |
| --- | --- | ---: | ---: | --- | --- | --- | --- |
| {d(date)} | {payee}{ " *({scenario})*" if scenario} | {cur(amount)} | {cur(running)} | {Status} | {category or ""} | {"`#a` `#b`" or ""} | {memo or ""} |
```
**Transactions, JSON** (pretty, sorted keys): `{"account":str,"count":n,"transactions":[{"amount":num,"category"?:str,"contexts"?:[str],"date":"YYYY-MM-DD","memo"?:str,"payee":str,"runningBalance":num,"scenario"?:str,"status":"cleared"}…]}`.
`category`, `contexts` (only if non-empty), `memo` (only if non-empty) and `scenario` are omitted when absent.

**Scenarios**: Text: `Available Scenarios:` then `- {name}` per live scenario (sorted by name) or `(None)`.
Markdown: `## Available Scenarios`, empty line, `- **{name}**` or `*(None)*`. JSON (pretty, keys sorted, fix: Swift's order was random):
`{"scenarios":[{"colorHex":str|null,"name":str,"notes":str|null}]}`.

**Import result**: Text/Markdown:
```
Import Complete for {account}:
Added:   {imported}
Skipped: {dups} (duplicates)
Skipped: {unmapped} (unmapped payees)
```
JSON (pretty, sorted keys): `{"account":str,"imported":n,"skippedDuplicates":n,"skippedUnmapped":n}`.

**Apple JSON style** (`to_apple_json`): pretty = 2-space indent, `"key" : value` (space before and
after the colon), arrays/objects one element per line, empty array `[]`, empty object `{}`; compact =
no spaces at all. Strings escape `"` `\\` control chars **and `/` as `\/`** (JSONSerialization does that).
Numbers: integral values print without a decimal point (`4000`), others with Rust's shortest `f64` Display
(`2051.95`). Parity tests compare parsed JSON, so whitespace mismatches against Swift are tolerated, but match as closely as stated.

### `apps/pf-cli/src/main.rs` (+ `args.rs`, `commands.rs`)

No `clap` (not in workspace deps). Hand-rolled parser:

```
pf-cli [--db PATH] <command> [options]

Commands:
  db-path                                   print the database path
  balances [--scenario NAME] [--format F]   all account balances
  balance ACCOUNT [--scenario NAME] [--format F]
  transactions ACCOUNT [-n N] [--scenario NAME] [--format F]     (N default 10)
  scenarios [--format F]
  export-md [--out FILE]                    Markdown backup to stdout or FILE
  import-md FILE --yes                      REPLACE all data with a Markdown backup
  import-csv ACCOUNT FILE [--format F]      import using the account's saved column mappings
  backup FOLDER                             write PFBackup_<timestamp>.md into FOLDER
  purge                                     open the DB (which purges soft-deleted rows) and exit
  help | --help | -h
F = json | text | markdown   (default text for the CLI; note the API defaults to json)
```

```rust
pub struct Cli { pub db: Option<PathBuf>, pub command: Command }
pub enum Command { DbPath, Balances { scenario: Option<String>, format: Format }, Balance { account: String, … },
    Transactions { account: String, n: usize, scenario: Option<String>, format: Format }, Scenarios { format: Format },
    ExportMd { out: Option<PathBuf> }, ImportMd { file: PathBuf, yes: bool }, ImportCsv { account: String, file: PathBuf, format: Format },
    Backup { folder: PathBuf }, Purge, Help }
pub fn parse_args(args: &[String]) -> Result<Cli, String>;   // Err(msg) ⇒ print msg + usage to stderr, exit 2
```

Behaviour:
- `--db` default: `Engine::default_db_path()`; the env var `PF_DB` overrides the default (but not `--db`).
- Account names matched case-insensitively; not found ⇒ stderr `Account not found: {name}`, exit 1.
- Unknown scenario ⇒ print `error_body` (like the API) to stdout and exit 1.
- `import-md` without `--yes` ⇒ stderr `import-md replaces ALL data in {path}. Re-run with --yes to confirm.`, exit 2.
- **Safety:** mutating commands (`import-md`, `import-csv`, `backup` is read-only so it is allowed) first try `TcpStream::connect_timeout(127.0.0.1:{apiPort setting or 8989}, 200 ms)`.
  If it connects ⇒ stderr `The PF app appears to be running (API on port {p}). Quit it first, or use the HTTP API.`, exit 1.
  (Two processes allocating ids from the same file would collide.)
- Runtime errors ⇒ stderr `error: {e}`, exit 1. Success ⇒ exit 0.
- The clock is the real clock; the time zone is `TimeZone::system()` (tests set `TZ=America/New_York`).

## Steps

1. Write `api_format.rs` unit tests first (table). Build data with `TestDb` (T01; pf-core's dev-dependency on pf-model needs `features = ["test-support"]`) or the world fixture
   through `pf_engine::testing::World` (pf-core cannot depend on pf-engine, so use `TestDb` in pf-core tests).
2. Implement `api_format.rs`; `pub mod api_format;` in pf-core's lib.rs.
3. Write `apps/pf-cli/tests/cli.rs` (assert_cmd), then the parser and commands.

## Tests (required)

`pf-core` unit tests (`api_format.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `r_api_3_parse_format` | R-API-3 | "TEXT" ⇒ Text, "md" ⇒ Markdown, "xml" ⇒ Json |
| `r_api_4_resolve_scenario` | R-API-4 | None/""/"Reality" ⇒ None; "plan a" ⇒ Some; "Nope" ⇒ `Scenario 'Nope' not found.` |
| `r_api_6_balances_text_exact` | R-API-6 | full expected string for a 3-account TestDb incl. one external and a negative balance |
| `r_api_6_balances_markdown_exact` | R-API-6 | includes the `*(ext)* ` trailing-space quirk |
| `r_api_6_balances_json_shape` | R-API-6 | parsed keys sorted; total excludes external |
| `r_api_7_running_balance_over_all_visible` | R-API-7, R-BAL-7 | n = 2 still shows the running totals computed from all rows |
| `r_api_7_transactions_text_exact` | R-API-7 | header line, 77 dashes, padding/truncation of a 13-char amount, the `{#a, #b}`, `// memo`, `(scenario)` suffixes |
| `r_api_7_transactions_markdown_exact` | R-API-7 | scenario `*(Plan A)*` in the payee cell, contexts `` `#Taxes` `` |
| `r_api_7_transactions_json_optional_keys` | R-API-7 | no `memo` key for an empty memo; no `contexts` key when none |
| `scenarios_all_formats` | – | `(None)` / `*(None)*` / empty array |
| `import_result_formats` | R-API-8 | exact text block |
| `apple_json_escapes_slash_and_pretty_style` | – | `{"a/b" : "c\/d"}`-style output, `"key" : value` spacing |

`apps/pf-cli/tests/cli.rs` (assert_cmd; each test uses its own `tempfile` DB and sets `TZ=America/New_York`):

| Test name | What it checks |
|---|---|
| `cli_help_exit_0` | `--help` prints the usage |
| `cli_bad_command_exit_2` | `frobnicate` ⇒ exit 2, stderr contains `Usage` |
| `cli_import_md_requires_yes` | exit 2 with the exact message |
| `cli_import_md_then_balances_text` | import `fixtures/world.md` --yes; `balances --format text` contains `Checking (Chase): $2,051.95`, `External 401k (Fidelity) [external]: $1,000.00` and `Total: $11,883.55` (non-external, all statuses, Reality: 2051.95 + 10500 − 668.40 + 0) |
| `cli_transactions_n_5` | `transactions checking -n 5 --format text` ⇒ header `Checking — Last 5 transactions`, 5 data lines, first data line starts with `2026-03-12` |
| `cli_scenario_flag` | `balance Checking --scenario "Plan A" --format json` ⇒ balance 551.95 |
| `cli_unknown_scenario_exit_1` | stdout `Error: Scenario 'Nope' not found.` |
| `cli_export_md_round_trip` | export-md --out a.md; import-md a.md --yes into a second DB; export again ⇒ identical files |
| `cli_import_csv_requires_saved_mappings` | exact R-API-8 message, exit 1 |
| `cli_backup_writes_file` | one `PFBackup_*.md` in the folder |
| `cli_refuses_mutation_when_app_port_busy` | bind a `TcpListener` on 127.0.0.1:0, write that port into the DB setting `apiPort` (open an Engine in the test and `set_setting_direct`), run import-md ⇒ exit 1 with the "appears to be running" message |
| `cli_sample_data2_smoke` | import `fixtures/SampleData2.md` (copied by T05) then `balances` exits 0 |

## Pitfalls

- `padding(toLength:)` **truncates**. `-$12,345,678.90` is 15 chars and becomes `-$12,345,678.`; test it.
- Count characters, not bytes, for padding (`chars().count()`); `—` is multibyte.
- The Swift text format uses `category.name`, not the full name.
- Do not print anything except the result to stdout (logs go to stderr via env_logger).
- `assert_cmd`: use `Command::cargo_bin("pf-cli")`.

## Done when

```bash
cargo test -p pf-core api_format
cargo test -p pf-cli
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- The HTTP server (T12). Interactive prompts. Shell completion.
