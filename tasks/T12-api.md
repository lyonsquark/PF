# T12: `pf-api`: localhost HTTP API, byte-compatible with the Swift app

**Phase:** 2 · **Deps:** T10c (and the `pf-core::api_format` module from T11; if T11 is not done yet, do T11's `api_format.rs` part first) · **Crate(s):** `pf-api` · **Size:** medium (about 300 lines of code plus 350 lines of tests)

## Goal

External scripts that talk to the Swift app on `http://localhost:8989` must keep working
unchanged against PFRust. Port `APIServer.swift` + `APIHandlers.swift` onto `tiny_http`, running on
a background thread that shares the engine with the UI.

## Read first

- `docs/business-rules.md` §13 (R-API-1…10): the whole section
- `tasks/T11-cli.md`, section "Exact formats" (all bodies come from `pf_core::api_format`)
- `tasks/T09-engine-core.md` (`SharedEngine`, World), `tasks/T10c-…md` (`import_csv_with_saved_mappings`, `export_markdown`)
- Swift: `Services/APIServer.swift` (routing order, `extractAccountName`, `HTTPRequest.parse`, `sendResponse`), `Services/APIHandlers.swift`
- `docs/testing.md` L4 and L6

## Deliverables

```
crates/pf-api/src/lib.rs      pub mod server; pub mod router; pub use server::*; pub use router::*;
crates/pf-api/src/router.rs   pure request → response (no sockets): unit-testable
crates/pf-api/src/server.rs   tiny_http thread
crates/pf-api/tests/common/mod.rs   tiny std::net HTTP client
crates/pf-api/tests/api.rs
crates/pf-api/tests/parity.rs  (#[ignore] until T26)
```

### router.rs

```rust
pub struct ApiResponse { pub status: u16, pub content_type: &'static str, pub body: String }
/// `url` is the raw request target, e.g. "/api/accounts/My%20Checking/transactions?n=5&format=text".
/// `changed` is set to true when the request mutated data (so the server can request a UI repaint).
pub fn handle(engine: &SharedEngine, method: &str, url: &str, body: &str, changed: &mut bool) -> ApiResponse;
/// Query parsing like HTTPRequest.parse: split on '&', each pair split on the FIRST '=', pairs without '='
/// are ignored, '+' ⇒ ' ' then percent-decode (invalid encoding ⇒ keep raw), later duplicates win.
pub fn parse_query(q: &str) -> HashMap<String, String>;
/// "/api/accounts/{name}{suffix}" ⇒ percent-decoded name (invalid encoding ⇒ raw middle).
pub fn extract_account_name(path: &str, suffix: &str) -> Option<String>;
pub fn percent_decode(s: &str) -> Option<String>;   // hand-rolled; UTF-8 aware; no new dependency
```

Routing, **in this order** (first match wins; mirrors `route`):

| # | Method + path | Handler |
|---|---|---|
| 1 | `GET /api/db-path` | 200 `text/plain`, body = the SQLite path (`Engine` must expose `db_path() -> Option<&Path>`; add it if T09 lacks it; in-memory ⇒ `Unknown`) |
| 2 | `GET /api/scenarios` | `format_scenarios` |
| 3 | `GET /api/accounts/balances` | `resolve_scenario` → `balance_entries` → `format_balances` |
| 4 | `GET /api/accounts/{name}/balance` | account by name (ci, live) else 404 `text/plain` `Account not found: {name}`; then like #3 with one entry |
| 5 | `GET /api/accounts/{name}/transactions` | `n` (default 10; non-integer ⇒ 10), 404 as #4, `transaction_lines` → `format_transactions` |
| 6 | `POST /api/accounts/{name}/import` | see below |
| 7 | `GET /api/export/markdown` | 200 `text/markdown`, `export_markdown` |
| – | anything else | 404 `text/plain` `Not Found: {METHOD} {path}` (path without the query) |

Note the order: `/api/accounts/balances` is matched **before** the `{name}/balance` pattern, so
an account literally named "balances" cannot be queried. That is a Swift quirk; keep it.

Common rules:
- `format` from the query, default `"json"`. **Content-Type quirk (R-API-3):** Swift picks
  `application/json` only when the raw `format` string is **exactly** `"json"` (case-sensitive),
  else `text/plain`, while the body format is chosen case-insensitively. So `format=JSON` returns a
  JSON body labelled `text/plain`. Keep it, and test it.
- Scenario errors (R-API-4) return **200** with `error_body` (Swift quirk).
- Import (#6): find the account (ci); not found ⇒ body `error_body("Account not found: {name}")`.
  Else call `commands::import::import_csv_with_saved_mappings(engine, id, body)`. `Ok(r)` ⇒
  `format_import_result`; `Err(e)` ⇒ `error_body(e.to_string())`. **Status:** 400 if the body contains
  `"error"` or `"Error:"`, else 200 (Swift's string sniffing; an account named "error" gets a 400; keep it).
  Content-Type per the format rule above. `changed = imported > 0`.
- Lock the engine **once per request**: `let mut eng = engine.lock()`; on a poisoned mutex use
  `poisoned.into_inner()` (log a warning). Never hold the lock while writing the socket.

### server.rs

```rust
pub struct ApiHandle { pub port: u16, server: Arc<tiny_http::Server>, thread: Option<JoinHandle<()>> }
impl ApiHandle { pub fn stop(mut self); }          // server.unblock(); join
/// R-API-1: binds 127.0.0.1:{port} ONLY (port 0 ⇒ OS-assigned; read the real port from
/// server.server_addr().to_ip()). Bind failure ⇒ Err (the desktop app logs a warning and continues).
/// `on_change` is called after any request with changed == true (the app passes ctx.request_repaint).
pub fn start(engine: SharedEngine, port: u16, on_change: Arc<dyn Fn() + Send + Sync>) -> std::io::Result<ApiHandle>;
```

Per request (in the server thread, one request at a time like Swift's main-queue handler):
1. R-API-10: if `Content-Length` > 10,485,760 ⇒ 400 `text/plain` `Bad Request`. Read the body as UTF-8;
   invalid UTF-8 ⇒ 400 `Bad Request`.
2. `router::handle(...)`.
3. Respond with headers `Content-Type: {ct}; charset=utf-8`, `Connection: close`,
   `Access-Control-Allow-Origin: *` (tiny_http adds Content-Length). Status reason phrases come from tiny_http.
4. If changed ⇒ `on_change()`.

## Steps

1. Write `tests/common/mod.rs`: `fn get(port, path) -> (u16, String /*content-type*/, String /*body*/)` and
   `fn post(port, path, body)`, using `TcpStream` and writing a raw `HTTP/1.1` request with `Connection: close`.
   Parse the status line, headers and body (read to EOF).
2. Write `tests/api.rs` (table). Each test: `World::with_fixture("world")` → `engine.shared()` → `start(shared, 0, cb)`.
3. Implement router.rs, then server.rs. Add the `db_path` accessor to Engine if it is missing.
4. Write `tests/parity.rs` with `#[ignore = "needs fixtures/parity (T26)"]` tests that load
   `fixtures/SampleData2.md`, call each route/format and compare with `fixtures/parity/<route>__<format>.txt`
   (JSON: compare parsed values, numbers within 0.005; text/markdown: byte-equal).

## Tests (required) (`tests/api.rs`, each with `/// Covers: R-API-…`)

| Test name | Covers | What it checks |
|---|---|---|
| `r_api_1_binds_loopback_only` | R-API-1 | `handle.port != 0`; connecting to the machine's non-loopback address fails or is refused (skip if none); binding the same port twice ⇒ Err |
| `r_api_2_unknown_route_404` | R-API-2 | `GET /nope?x=1` ⇒ 404 body `Not Found: GET /nope` |
| `r_api_2_db_path` | R-API-2 | ends with the World's file name |
| `r_api_3_headers` | R-API-3 | content-type `application/json; charset=utf-8`, `Connection: close`, CORS header |
| `r_api_3_uppercase_json_quirk` | R-API-3 | `format=JSON` ⇒ JSON body with `text/plain; charset=utf-8` |
| `r_api_4_unknown_scenario_200_error_body` | R-API-4 | json `{"error":"Scenario 'Nope' not found."}`, status 200; text ⇒ `Error: …` |
| `r_api_4_reality_keyword` | R-API-4 | `scenario=reality` == no param |
| `r_api_5_percent_and_plus_decoding` | R-API-5 | `/api/accounts/External%20401k/balance` works; `?scenario=Plan+A` works; `CHECKING` works |
| `r_api_5_account_not_found_404` | R-API-5 | body `Account not found: Nope` |
| `r_api_6_balances_all_formats` | R-API-6 | insta snapshots of json, text, markdown for world |
| `r_api_6_balance_single_account` | R-API-6 | Checking with Plan A ⇒ 551.95 |
| `r_api_7_transactions_all_formats` | R-API-7 | insta snapshots, n=5 |
| `r_api_7_default_n_is_10` | R-API-7 | Checking has 8 reality rows ⇒ count 8; with Plan A ⇒ 9 |
| `r_api_8_import_without_mappings_400` | R-API-8 | exact message, 400 |
| `r_api_8_import_success_then_duplicates` | R-API-8, R-CSV-11 | after a `run_csv_import` (bank, Checking) saved column mappings in the test setup, POST the same CSV ⇒ 200, `imported 0`, `skippedDuplicates 2`; POST a new row ⇒ imported 1 |
| `r_api_9_mutation_goes_through_engine_and_repaints` | R-API-9 | after a successful import the callback counter is 1, `engine.can_undo()` is true and `undo_name()` is "Import CSV" |
| `r_api_10_oversized_body_400` | R-API-10 | Content-Length 10_485_761 (send the header only, then close) ⇒ 400 |
| `export_markdown_route` | R-API-2 | `text/markdown`; body == `export_markdown(engine)` |
| `concurrent_requests_do_not_deadlock` | – | 8 threads × 20 GETs complete within 5 s |

`router.rs` unit tests: `parse_query` (+, %20, missing `=`, duplicate keys), `extract_account_name`
(suffix mismatch ⇒ None, `%E2%80%94` ⇒ `—`, bad `%zz` ⇒ raw).

## Parity notes

- Swift `JSONSerialization` writes doubles from `NSDecimalNumber.doubleValue`; Rust uses `Decimal → f64`.
  Values like `0.1 + 0.2` may print differently, so parity compares numbers within 0.005.
- Swift's scenario JSON had random key order; PFRust sorts keys. Parity compares parsed JSON.
- `/api/db-path` necessarily differs (a different file); parity skips it.
- Both apps default to port 8989: the user must quit the Swift app first (the migration README says so).
- The Swift API ignores the saved "negate amounts" setting on import; PFRust keeps that quirk (see T10c).

## Pitfalls

- `tiny_http::Server::http("127.0.0.1:0")`. **Never** `0.0.0.0`.
- Tests run in parallel: always port 0, never 8989.
- Do not call `on_change` while holding the engine lock (the UI thread may be waiting on it).
- `server.incoming_requests()` blocks; `unblock()` from `stop()` ends the loop.

## Done when

```bash
cargo test -p pf-api
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules      # all R-API-* present
```

## Out of scope

- Starting the server from the desktop app (T13 does `pf_api::start(shared, port, repaint)` and logs on Err).
- Capturing the Swift parity files (T26).
