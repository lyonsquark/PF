# CLAUDE.md: rules for agents working in PFRust

You are porting the Swift app in `../PFApp` to Rust. The plan is `PLAN.md`; the work
queue is `tasks/STATUS.md`. You are expected to be a small model working on **one task at a time**.

## Workflow

1. Pick a task per `PLAN.md` → "How to use this plan". Mark it `in-progress` in `tasks/STATUS.md`.
2. Read, in this order: the task file, then every file under its "Read first".
   **Always read the Swift source a task names.** The docs summarise it; the Swift code is the
   behavioural truth. If the docs and Swift disagree, follow Swift (unless the rule is marked
   "(fix)"), and note the discrepancy in STATUS.md.
3. Write tests first (see `docs/testing.md`), then the code.
4. Run the task's "Done when" commands. All must pass with **zero warnings**.
5. Update `tasks/STATUS.md`: `done`, plus a short note. Commit only if the user asked you to.

## Commands (run from `PFRust/`)

```bash
cargo build --workspace
cargo test --workspace
cargo test -p <crate> <filter>
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo xtask rules            # after T00b
cargo run -p pf              # the desktop app
cargo run -p pf-cli -- --help
```

## Hard rules

* **Stay inside your task.** Do not refactor other crates or "improve" unrelated code. If you
  need a change in another crate's public API, make the smallest additive change and note it.
* **Never touch `../PFApp`** or the Swift database (`~/Library/Application Support/PF/PF.store`).
  Read Swift files; never edit them. Copy fixtures into `PFRust/fixtures/`, never move them.
* **Never use `f64` for money.** `rust_decimal::Decimal` everywhere; `dec!()` in tests.
  Convert to `f64` only at the last moment for chart rendering.
* **Dates are `jiff`.** `jiff::civil::Date` for days, `jiff::Timestamp` for instants. Do not add `chrono`.
* **No clock or time zone reads in `pf-core`.** Pass `today: Date` / `tz: &TimeZone`.
* **All mutations go through engine commands** (`pf-engine::commands`), one `Engine::apply` per user action.
* **No `unsafe`, no `unwrap()`/`expect()` in non-test code** except on invariants with a comment
  explaining why it cannot fail. Use `thiserror` errors; `anyhow` only in binaries and xtask.
* **No new dependencies** beyond `[workspace.dependencies]` in the root `Cargo.toml`. If you
  truly need one, mark the task `blocked` and explain why.
* **egui is version 0.37**, which differs from older egui you may remember. Use
  `docs/egui-cheatsheet.md`; when unsure, read the source in
  `~/.cargo/registry/src/index.crates.io-*/egui-0.37.0/`. Do not guess API names.
* Never weaken, delete or `#[ignore]` a test to get green (see `docs/testing.md`).
* Keep files under about 600 lines; split into modules when bigger.

## Style

* `rustfmt` defaults; `clippy` clean with `-D warnings`.
* Public items get a one-line `///` doc comment. Business-rule code cites the rule ID:
  `// R-PRJ-3: clamp day to month length`.
* Names: `snake_case` functions, `CamelCase` types, ids as newtypes (`AccountId`).
* Prefer plain functions and structs over traits and generics; this code must be easy to read.
* User-visible strings must match the Swift app's text exactly when a rule quotes them.

## Where things are

| Need | Look at |
|---|---|
| What to build next | `tasks/STATUS.md`, `PLAN.md` |
| Entities, SQL schema, settings keys | `docs/data-model.md` |
| Behaviour + rule IDs | `docs/business-rules.md` |
| Backup file format | `docs/markdown-format.md` |
| Test layers, BEH/UI IDs, tools | `docs/testing.md` |
| egui 0.37 API | `docs/egui-cheatsheet.md` |
| Swift source map | `docs/swift-map.md` |
