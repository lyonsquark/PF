# T00b: `xtask` (`rules` traceability check and `ci`)

**Phase:** 0 · **Deps:** T00 · **Crate(s):** `xtask` · **Size:** small (about 200 lines including tests)

## Goal

Add two commands every later task uses to check its work:

* `cargo xtask rules` fails when any rule or behaviour ID from the specs has no test.
* `cargo xtask ci` runs the full gate: fmt check, clippy, tests, then rules.

## Read first

- `docs/testing.md` → "Traceability check" and "Commands"
- `docs/business-rules.md` (skim: see what IDs look like)
- `xtask/Cargo.toml`, `xtask/src/main.rs`, `.cargo/config.toml` (created in T00)

## Deliverables

```rust
// xtask/src/main.rs
fn main() -> anyhow::Result<()>;          // dispatch: "rules" | "ci" | anything else → usage, exit 2

// xtask/src/rules.rs
/// IDs declared in the spec docs, e.g. "R-PRJ-3", "R-RPT-4a", "BEH-ACC-01", "UI-NAV-01".
pub fn declared_ids(doc_text: &str) -> std::collections::BTreeSet<String>;
/// IDs referenced from test code in one Rust source file (see "Test-code scope").
pub fn referenced_ids(path: &std::path::Path, source: &str) -> std::collections::BTreeSet<String>;
/// Runs the whole check from the workspace root; returns the missing IDs.
pub fn run_rules(root: &std::path::Path) -> anyhow::Result<Vec<String>>;

// xtask/src/ci.rs
pub fn run_ci(root: &std::path::Path) -> anyhow::Result<()>;
```

## Specification

### Declared IDs
* Scan exactly two files: `docs/business-rules.md` and `docs/testing.md`.
* Regex: `\b(R-[A-Z]+-\d+[a-z]?|BEH-[A-Z]+-\d+|UI-[A-Z]+-\d+)\b`.
* Ignore IDs that only appear inside the pattern examples. That happens automatically, because the
  literal text `R-[A-Z]+-\d+[a-z]?` does not match the regex. Also ignore the template
  placeholders `R-XXX-n` / `BEH-XXX-01` (they never match because `n`/`XXX` are not digits/valid).
* Docs name some sub-rules with a trailing "…" (e.g. `R-RPT-4a…`). The regex picks up
  `R-RPT-4a`, which is intended: T08 must create it.

### Referenced IDs
* Walk `crates/` and `apps/` recursively for `*.rs`; skip any `target` directory.
* **Test-code scope:** if the path contains a `tests` directory component, scan the whole file.
  Otherwise scan only the text from the first `#[cfg(test)]` to the end of the file. This means
  a `// R-PRJ-3` comment in implementation code does **not** count as a test.
* Within that text, accept two spellings:
  1. Exact IDs (same regex as above), e.g. in `/// Covers: R-BAL-1, R-SCN-1`.
  2. Test-name spelling: regex (case-insensitive)
     `(?:^|[^A-Za-z0-9])(r|beh|ui)_([a-z]+)_(\d+)([a-z])?(?:_|[^A-Za-z0-9]|$)`.
     Normalise to `PREFIX-FAMILY-NUM[suffix]` uppercase prefix/family, e.g.
     `r_prj_3_jan31` → `R-PRJ-3`, `r_rpt_4a_x` → `R-RPT-4a`, `beh_acc_01_create` → `BEH-ACC-01`.
     Keep the number exactly as written (`01` stays `01`).

### Output of `rules`
```
rules: 212/230 IDs covered
missing (18):
  R-API-1
  ...
```
Exit code 0 if none missing, else 1. Print the missing IDs in sorted order.

### `ci`
Run in order from the workspace root, stopping at the first failure (show the command, inherit stdio):
1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. the rules check (in-process, via `run_rules`)

Workspace root = `Path::new(env!("CARGO_MANIFEST_DIR")).parent()`.

## Steps

1. Create `xtask/src/rules.rs` and `xtask/src/ci.rs`; declare `mod rules; mod ci;` in `main.rs`.
2. Write the unit tests (below) in `rules.rs` first.
3. Implement `declared_ids`, `referenced_ids`, a recursive `fn rust_files(dir, &mut Vec<PathBuf>)` (plain `std::fs::read_dir`, no new crates), then `run_rules`.
4. Implement `run_ci` with `std::process::Command::new("cargo")`.
5. `main`: match on `std::env::args().nth(1).as_deref()`: `Some("rules")`, then print and `std::process::exit(1)` if missing; `Some("ci")`; otherwise print `usage: cargo xtask <rules|ci>` and exit 2.

Reference implementation of the core (adapt as needed):

```rust
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

const ID_RE: &str = r"\b(R-[A-Z]+-\d+[a-z]?|BEH-[A-Z]+-\d+|UI-[A-Z]+-\d+)\b";
const NAME_RE: &str = r"(?i)(?:^|[^A-Za-z0-9])(r|beh|ui)_([a-z]+)_(\d+)([a-z])?(?:_|[^A-Za-z0-9]|$)";

pub fn declared_ids(doc: &str) -> BTreeSet<String> {
    let re = Regex::new(ID_RE).expect("static regex");
    re.captures_iter(doc).map(|c| c[1].to_string()).collect()
}

pub fn referenced_ids(path: &Path, source: &str) -> BTreeSet<String> {
    let in_tests_dir = path.components().any(|c| c.as_os_str() == "tests");
    let scope = if in_tests_dir {
        source
    } else {
        match source.find("#[cfg(test)]") { Some(i) => &source[i..], None => return BTreeSet::new() }
    };
    let mut out = declared_ids(scope);
    let re = Regex::new(NAME_RE).expect("static regex");
    for c in re.captures_iter(scope) {
        let prefix = c[1].to_uppercase();
        let family = c[2].to_uppercase();
        let suffix = c.get(4).map(|m| m.as_str().to_lowercase()).unwrap_or_default();
        out.insert(format!("{prefix}-{family}-{}{suffix}", &c[3]));
    }
    out
}
```

Note that `NAME_RE` consumes the trailing separator, so two adjacent IDs like `r_a_1_r_b_2` only match the first one.
That is acceptable: one ID per test name, and further IDs go in a `/// Covers:` comment.

## Tests (required)

All in `xtask/src/rules.rs` `#[cfg(test)] mod tests`:

| Test name | What it checks |
|---|---|
| `declared_finds_all_three_kinds` | `"R-PRJ-3 and BEH-ACC-01, UI-NAV-01"` gives 3 IDs |
| `declared_keeps_letter_suffix` | `"R-RPT-4a…"` gives `R-RPT-4a` |
| `declared_ignores_regex_literal` | the text `R-[A-Z]+-\d+[a-z]?` gives an empty set |
| `referenced_test_name_spelling` | `fn r_prj_3_jan31_clamps()` inside `#[cfg(test)]` gives `R-PRJ-3` |
| `referenced_beh_keeps_leading_zero` | `fn beh_acc_01_create()` gives `BEH-ACC-01` |
| `referenced_suffix_letter` | `fn r_rpt_4a_balance_sheet()` gives `R-RPT-4a` |
| `referenced_covers_comment` | `/// Covers: R-BAL-1, R-SCN-1` gives both |
| `impl_comment_outside_cfg_test_ignored` | `// R-PRJ-3` before `#[cfg(test)]` is not counted |
| `file_without_cfg_test_counts_nothing` | a plain source file gives an empty set |
| `tests_dir_scans_whole_file` | path `crates/x/tests/a.rs` with `fn beh_scn_02()` at top level gives `BEH-SCN-02` |
| `no_false_positive_on_words` | `fn error_handling()`, `let ui_state = 1;` give an empty set |

## Pitfalls

- `cargo xtask` works because of the alias in `.cargo/config.toml` (from T00). Do not change it.
- Do not add `walkdir` or `glob`; use `std::fs::read_dir` recursion.
- `regex` does not support look-around; that is why `NAME_RE` consumes a separator.
- Right after this task almost every ID will be "missing". That is expected: `rules` exits 1
  until the code exists. `ci` will therefore fail at step 4 for a while; tasks only require that
  **their own** IDs are not in the missing list.

## Done when

```bash
cargo test -p xtask
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules ; echo "exit=$?"      # runs, prints coverage, exit=1 (expected for now)
cargo xtask nonsense ; echo "exit=$?"   # usage message, exit=2
```

## Out of scope

- Coverage measurement (llvm-cov), bundling (T25 adds `cargo xtask bundle` if it wants).
