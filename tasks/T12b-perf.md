# T12b: Performance suite (100k transactions), and fixes until the budgets pass

**Phase:** 2 · **Deps:** T10c · **Crate(s):** `pf-engine` (tests + `testing.rs`), possibly small fixes in `pf-model`, `pf-core` and `pf-store` · **Size:** medium (about 200 lines of tests, plus fixes)

## Goal

Speed is why the app is being rewritten. This task adds a reproducible 100k-transaction
dataset and release-mode timing tests for the hot paths, then fixes whatever misses its budget.
Budgets come from `docs/testing.md` → "Performance tests".

## Read first

- `docs/testing.md`, section "Performance tests"
- `PLAN.md`: "Data access" decision and the UI note about `ViewCaches`
- The current code of `pf-store` (load, apply_changes), `pf-core` (balance/filter/running balance from T03, markdown from T05), `pf-engine` (apply)

## Deliverables

### Generator, in `crates/pf-engine/src/testing.rs` (public: the UI and CLI may use it for demos)

```rust
/// Deterministic large dataset. No RNG crate: a 64-bit LCG
///   state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); use (state >> 33).
pub struct LargeDbSpec { pub seed: u64, pub accounts: usize, pub transactions: usize, pub years: i16,
                         pub categories: usize, pub contexts: usize, pub scenarios: usize }
impl Default for LargeDbSpec { /* seed 42, accounts 10, transactions 100_000, years 5, categories 40, contexts 20, scenarios 3 */ }
/// Builds the Db in memory with `Db::alloc_id` + `Db::put` (not TestDb, which is test-only; and NOT through engine commands: that would be slow
/// and is not what is being measured). Mix: 90% reality, 10% spread over the scenarios; statuses
/// 80% cleared, 10% pending, 5% reconciled, 5% future (future only on scenario rows); amounts −500.00…+3000.00
/// with 2 decimals; 20% of transactions get 1–2 contexts; every category gets 1–3 match patterns;
/// dates uniformly spread over `years` years ending at TEST_TODAY; payees from a list of 200 generated names.
pub fn generate_large_db(spec: &LargeDbSpec) -> Db;
/// Writes it to a fresh SQLite file via pf-store and returns the World (history empty).
pub fn large_world(spec: &LargeDbSpec) -> World;
```

### `crates/pf-engine/tests/perf.rs`

Every test is `#[ignore = "perf: run with --release -- --ignored"]`. Each builds the dataset once
(use `std::sync::OnceLock` for the generated `Db`), times the operation with `std::time::Instant`
(best of 5 runs, after 1 warm-up), prints `perf {name}: {ms:.2} ms (budget {b} ms)` to stderr, and
asserts the budget **only when `!cfg!(debug_assertions)`**.

| Test name | Operation | Budget |
|---|---|---|
| `perf_load_from_sqlite` | `Engine::open(path)` on the 100k file (includes purge check and load) | < 500 ms |
| `perf_account_view` | for the busiest account: filter by PastYear in Reality + running balance (R-BAL-7), i.e. exactly what the account screen calls (T03's function(s)) | < 15 ms |
| `perf_account_view_scenario_all` | same with a scenario active and TimePeriod::All | < 15 ms |
| `perf_dashboard_balances` | balance + cleared + reality balances for all accounts + obligation reserved totals (R-BAL-1…6) | < 10 ms |
| `perf_single_edit_apply` | `edit_transaction(Payee)` including the SQLite write | < 5 ms |
| `perf_undo_redo_single` | undo + redo of that edit | < 5 ms |
| `perf_bulk_projection` | `generate_projection` for 40 monthly items over 5 years (about 2,400 rows) | < 200 ms |
| `perf_search_payee` | payee search "ab" over all 100k (R-FLT-3) | < 30 ms |
| `perf_markdown_export` | `export_markdown` | < 1000 ms |
| `perf_markdown_import` | `import_markdown_replacing` of that export | < 3000 ms |
| `perf_api_transactions` | `api_format::transaction_lines(n = 10)` for the busiest account | < 15 ms |

Also one **non-ignored** test, `large_db_generator_is_deterministic`: two runs with the same seed
produce equal Dbs with exactly `transactions` rows (use a small spec, e.g. 2,000 rows, so it is fast in debug).

## Steps

1. Implement the generator and the determinism test.
2. Write `perf.rs`. Run `cargo test --release -p pf-engine --test perf -- --ignored --nocapture`.
3. For each miss, profile (`cargo build --release` + `samply record` if installed, or add temporary
   `Instant` logging) and fix. **Allowed fixes** (keep them small, tested, behaviour-preserving):
   - pf-store: wrap load in one read transaction; `prepare_cached`; load `transaction_contexts` in one
     query grouped by id; bulk inserts with one prepared statement in `apply_changes`.
   - pf-model: add a secondary index `tx_by_account: HashMap<AccountId, Vec<TransactionId>>` (sorted by
     `(date, id)`), maintained in `Db::put`/`Db::remove`. Give it unit tests and expose `transactions_for_account` through it.
   - pf-core: compute the running balance in one pass over the per-account index; avoid cloning
     `Transaction`s (return ids or references); precompile category regexes once per call, not per row.
   - pf-engine: avoid cloning the whole `Db`; the Writer's touched map must stay O(1).
   - Markdown: `String::with_capacity`, `write!` into one buffer, no per-row `format!` + concat.
4. Re-run the whole normal suite (`cargo test --workspace`) after every fix: behaviour must not change.
5. Record the final timings in `tasks/STATUS.md` notes (one line, e.g. `load 180ms, view 4ms, …`).

## Tests (required)

- The 11 perf tests above (ignored by default) plus `large_db_generator_is_deterministic`.
- Any index or cache added to pf-model/pf-core gets unit tests proving it stays consistent after
  insert, update (date or account change), soft delete and undo.

## Pitfalls

- Measure **release** builds only; debug numbers are meaningless here.
- Do not "fix" a budget by caching in the test itself. The production code path must be fast.
- The UI will additionally cache views per `engine.revision()` (T15a), but the budgets above are for
  the **uncached** computation, so a cache miss after an edit is still instant.
- Generating 100k rows through `Engine::apply` one by one is slow by design; the generator builds the Db directly.

## Done when

```bash
cargo test --workspace                                                     # normal suite still green
cargo test --release -p pf-engine --test perf -- --ignored --nocapture     # all budgets pass
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Out of scope

- UI frame-time profiling (covered by T15a's cache design and T27's manual checklist).
