# T28: Reconciliation core (`pf-core::reconcile`)

**Phase:** 5 · **Deps:** – (all earlier tasks are done) · **Crate(s):** `pf-core` · **Size:** large (≈ 900 lines incl. tests)

## Goal

A pure module that takes a bank or card **statement file** and an account, and returns a **reconciliation
report**: does the statement add up, how to read its signs, which statement lines pair with which PFRust
transactions, what is missing on either side, and the balance differences. Nothing is written to the
database here; T29 applies the result, T30 draws it, T31 serves it over the API.

This is a **new feature with no Swift source**. This file and `docs/reconciliation-plan.md` are the truth.

## Read first

- `docs/reconciliation-plan.md`: whole file (the design and the owner's decisions).
- `CLAUDE.md`, `docs/testing.md` (rule IDs, test naming, `Covers:` comments).
- `crates/pf-core/src/csv_import/mod.rs` and `parse.rs`, `plan.rs`: module layout to copy, and the helpers
  `split_csv_line`, `parse_csv_amount`, `parse_csv_date` (reuse them, do not rewrite).
- `crates/pf-core/src/balance.rs`: `is_visible`, how balances are summed.
- `crates/pf-model/src/entities.rs`: `Transaction`, `ImportMapping`, `AccountType`; `crates/pf-model/src/db.rs`.
- Rules R-SCN-1, R-BAL-2, R-CSV-1, R-CSV-6, R-CSV-7 in `docs/business-rules.md`.

## Deliverables

Add `serde_json = { workspace = true }` to `crates/pf-core/Cargo.toml` (it is already a workspace
dependency; note this additive change in STATUS.md).

Create `crates/pf-core/src/reconcile/` with `mod.rs` re-exporting the public API, and these files (keep each
under about 600 lines): `statement.rs` (types, JSON and CSV parsing), `check.rs` (self-check, readings),
`payee.rs` (normalising and scoring), `matching.rs` (passes and sections), `report.rs` (balance checks and
the `Report` type), `tests/` (one file per source file, like `csv_import/tests`). Add `pub mod reconcile;`
to `crates/pf-core/src/lib.rs`.

```rust
// statement.rs
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub institution: String,          // "" when absent
    pub account_hint: String,         // "" when absent
    pub period_start: Date,
    pub period_end: Date,
    pub opening_balance: Decimal,     // as printed (sign convention unknown)
    pub closing_balance: Decimal,
    pub total_credits: Option<Decimal>,
    pub total_debits: Option<Decimal>,
    pub lines: Vec<StatementLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatementLine {
    pub date: Date,
    pub post_date: Option<Date>,
    pub description: String,
    pub amount: Decimal,              // holder's view: spent/owed more < 0, received/owed less > 0
    pub section: String,              // heading on the statement, "" when absent
    pub reference: String,            // "" when absent
}

pub fn parse_statement_json(text: &str) -> Result<Statement, StatementError>;
pub fn parse_statement_csv(
    text: &str,
    period_start: Date,
    period_end: Date,
    opening: Decimal,
    closing: Decimal,
    tz: &TimeZone,
) -> Result<Statement, StatementError>;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StatementError {
    #[error("{0}")]
    Invalid(String),                  // exact messages in R-REC-1 / R-REC-2
}

// check.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceStyle { Asset, Owed }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading { AsPrinted, Flipped }   // Flipped = every line amount negated

pub fn self_check(st: &Statement) -> Result<[(Reading, BalanceStyle); 2], String>;
pub fn line_amount(line: &StatementLine, reading: Reading) -> Decimal;
pub fn pf_balance(printed: Decimal, style: BalanceStyle) -> Decimal;

// payee.rs
pub fn normalize_description(s: &str) -> String;
pub fn payee_score(description: &str, payee: &str, aliases: &[(String, String)]) -> f64;

// report.rs (the entry point)
pub struct ReconcileInput<'a> {
    pub db: &'a Db,
    pub account: AccountId,
    pub statement: &'a Statement,
    pub preferred: Option<Reading>,  // remembered per account by T29; only breaks a tie
}
pub fn reconcile(input: &ReconcileInput<'_>) -> Result<Report, String>;
pub fn difference(report: &Report, choice: &Choice) -> Decimal;
pub fn reconciled_balance(db: &Db, account: AccountId) -> Decimal;
```

`Report`, `Pair`, `Confidence`, `Section` and `Choice` are defined in step 7. `f64` is allowed **only** for
the payee score (it is not money); every amount stays `Decimal`.

## Rules (add these rows to `docs/business-rules.md` as a new section 14)

Add a section `## 14. Statement reconciliation (`pf-core::reconcile`)` after section 13, with these rows,
word for word. They are new PFRust behaviour; the Swift column says `— (new)`.

| ID | Rule |
|---|---|
| R-REC-1 | **Statement JSON**: an object with `period_start`, `period_end` (`YYYY-MM-DD`), `opening_balance`, `closing_balance` (decimal strings; `$` and `,` stripped as R-CSV-7) and `lines` (array, may be empty); optional `institution`, `account_hint`, `total_credits`, `total_debits`. Each line has `date` and `amount` (required), optional `post_date`, `description`, `section`, `reference`. Unknown keys are ignored. Errors (exact): `Statement is not valid JSON: {serde message}`, `Missing field: {name}`, `Line {n}: missing field: {name}` (n is 1-based), `Invalid date in {field}: {value}`, `Invalid amount in {field}: {value}` (`{field}` is `line {n} amount` for lines), `Statement period ends before it starts.` |
| R-REC-2 | **Statement CSV**: parsed with R-CSV-1. Headers matched case-insensitively after trimming: `date` (required), `description` (or `payee`, `merchant`, `name`), `amount` (required), optional `post date`, `section`, `reference`. Dates per R-CSV-6, amounts per R-CSV-7. A row whose date or amount does not parse is an error `Line {n}: invalid date or amount.` (not skipped: a statement must be complete). Balances and the period come from the caller. Missing required column: `The CSV needs Date and Amount columns.` |
| R-REC-3 | **Self-check**: with S = Σ line amounts, the statement adds up *asset style* if opening + S = closing, and *owed style* if opening − S = closing. If neither holds, the error is `Statement does not add up: opening {o}, lines {S}, closing {c}. Off by {d}.` (amounts with `format_currency`; d = the smaller of |opening + S − closing| and |opening − S − closing|). If negating the lines of exactly one non-empty `section` would make either style add up, append ` Flipping the signs under '{section}' would make it add up.` (first such section in line order). |
| R-REC-4 | **Totals check** (only when both `total_credits` and `total_debits` are present): the pair {Σ positive line amounts, Σ |negative line amounts|} must equal the pair {total_credits, total_debits} in either order. Otherwise: `Statement totals do not match: credits {a} and debits {b} listed, lines give {x} and {y}.` |
| R-REC-5 | **Two readings**: when the statement adds up in style X with the lines as printed, it also adds up in the other style with every line negated. `self_check` returns both: (AsPrinted, X) first, then (Flipped, other). If S = 0 both styles hold for both readings; then return (AsPrinted, style by account type: Credit or Loan ⇒ Owed, else Asset) and (Flipped, the same style). |
| R-REC-6 | **PFRust sign of balances**: asset style balances are used as printed; owed style balances are negated (owed 1,764.30 ⇒ −1,764.30), because PFRust stores a card's debt as a negative balance. |
| R-REC-7 | **Choosing the reading**: for each of the two readings, count statement lines that have a candidate (R-REC-8) with exactly the same amount within 10 days. More matches wins. On a tie: `preferred` if given; else the reading whose style fits the account type (Credit or Loan ⇒ Owed, else Asset); else AsPrinted. The report states the chosen reading, its style, and both counts. |
| R-REC-8 | **Candidates**: live transactions of the account in Reality (`scenario_id` None), dated from period start − 7 days to period end + 7 days inclusive, whose status is not Reconciled. Reconciled ones in the same window are kept apart for R-REC-12. |
| R-REC-9 | **Pass 1 and 2 (exact amount)**: a line and a candidate may pair when their amounts are equal and their day distance is at most 3 (pass 1, confidence High) or at most 10 (pass 2, Medium, on what pass 1 left). Day distance = the smaller of |line date − tx date| and |post date − tx date|. Within a pass, list every allowed (line, candidate) pair, sort by day distance, then payee score descending, then line index, then transaction date, then transaction id; take each pair in that order if both sides are still free. |
| R-REC-10 | **Pass 3 (amount differs)**: on what is left, a line and a candidate may pair when both amounts have the same sign, the day distance is at most 3, the payee score is at least 0.5, and |line amount − tx amount| ≤ 25% of |line amount|. Same sort and greedy taking as R-REC-9, with |amount difference| ascending inserted after day distance. |
| R-REC-11 | **Payee score**: `normalize_description` upper-cases, replaces every character that is not a letter or digit with a space, splits on spaces, drops tokens that contain a digit, drops the stop words `POS DEBIT CREDIT ACH SQ TST PURCHASE PAYMENT CARD CHECKCARD VISA MC ONLINE RECURRING WWW COM INC LLC CO THE`, and joins the rest with single spaces. Score = shared tokens ÷ the smaller token count (0 when either is empty). An alias (normalized description, payee) whose description equals the line's normalized description and whose payee equals the transaction's payee case-insensitively gives 1.0. Aliases are the account's `statementPayee` mappings plus every `payee` mapping (source normalized, target as is). |
| R-REC-12 | **Sections**: Matched (pass 1 and 2 pairs), AmountDiffers (pass 3 pairs), AlreadyReconciled (unpaired lines with an exact-amount Reconciled transaction within 3 days; the nearest one, ties by id), StatementOnly (other unpaired lines), PfOnlyInPeriod (unpaired candidates dated inside the period), PfOnlyOutside (unpaired candidates outside it). Each section is sorted by date, then line index or transaction id. |
| R-REC-13 | **Balance checks**: reconciled balance = Σ amounts of the account's live Reality Reconciled transactions. Opening difference = statement opening (PFRust sign) − reconciled balance; `first_time` is true when the account has no Reconciled transaction. Difference for a choice = statement closing (PFRust sign) − (statement opening (PFRust sign) + Σ accepted pairs' amounts (the PFRust amount, or the statement amount where chosen) + Σ added lines' amounts + adjustment). The default choice accepts every Matched and AmountDiffers pair (PFRust amounts) and adds every StatementOnly line. |

## Steps

1. Add the rule rows above to `docs/business-rules.md`. Run `cargo xtask rules`; it now lists R-REC-1..13 as
   missing. That list must be empty when you finish.
2. `statement.rs`: write the R-REC-1 and R-REC-2 tests first (good file, each error message), then the
   parsers. Parse JSON with `serde_json::Value` (not derive), so the error messages are yours. `tz` is only
   needed by `parse_csv_date`.
3. `check.rs`: tests for R-REC-3..6 first: an asset-style checking statement, an owed-style card statement, a
   file that does not add up, the one-section hint, the totals pair in both orders, S = 0.
4. `payee.rs`: R-REC-11 tests with real-looking descriptions (`SQ *BLUE BOTTLE 8812`, `TST* JOE'S PIZZA #12`,
   `AMAZON.COM*2K4 AMZN.COM/BILL WA`, `CHECKCARD 0914 TRADER JOE'S #552`), then the code.
5. `matching.rs`: candidates (R-REC-8), passes (R-REC-9, R-REC-10), sections (R-REC-12).
6. `report.rs`: reading choice (R-REC-7), balance checks (R-REC-13), `reconcile`, `difference`.
7. Define the report types (public, `Debug, Clone, PartialEq`):

```rust
pub enum Confidence { High, Medium, AmountDiffers }
pub struct Pair { pub line: usize, pub transaction: TransactionId, pub confidence: Confidence,
                  pub day_distance: i32, pub payee_score: f64 }
pub struct Report {
    pub reading: Reading, pub style: BalanceStyle,
    pub matches_as_printed: usize, pub matches_flipped: usize,
    pub amounts: Vec<Decimal>,                 // each line's amount under the chosen reading
    pub opening: Decimal, pub closing: Decimal, // PFRust sign
    pub matched: Vec<Pair>, pub amount_differs: Vec<Pair>,
    pub already_reconciled: Vec<(usize, TransactionId)>,
    pub statement_only: Vec<usize>,
    pub pf_only_in_period: Vec<TransactionId>, pub pf_only_outside: Vec<TransactionId>,
    pub reconciled_balance: Decimal, pub opening_difference: Decimal, pub first_time: bool,
}
pub struct Choice {
    pub pairs: Vec<(usize, TransactionId, bool)>, // bool: use the statement amount
    pub add_lines: Vec<usize>,
    pub adjustment: Option<Decimal>,
}
impl Choice { pub fn default_for(report: &Report) -> Choice; }
```

8. Golden tests: three statement files under `fixtures/statements/` (see Tests), each with a small
   `TestDb` built in the test, checked with `insta` snapshots of the `Report` (`{:#?}`). Review each snapshot
   by hand against the rule text before accepting it.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_rec_1_parses_full_json` | R-REC-1 | every field, unknown keys ignored, `$1,234.50` parsed |
| `r_rec_1_error_messages` | R-REC-1 | each exact message (bad JSON, missing field, line field, date, amount, period) |
| `r_rec_2_parses_csv` | R-REC-2 | header aliases, post date, section; balances from arguments |
| `r_rec_2_bad_row_is_error` | R-REC-2 | an unparseable row is an error, not skipped; missing column message |
| `r_rec_3_asset_and_owed_styles` | R-REC-3 | checking adds up asset style; card adds up owed style |
| `r_rec_3_does_not_add_up_with_hint` | R-REC-3 | exact message including the one-section hint; no hint when none helps |
| `r_rec_4_totals_either_order` | R-REC-4 | totals match in both orders; mismatch message |
| `r_rec_5_two_readings` | R-REC-5 | both readings returned in order; the S = 0 case uses the account type |
| `r_rec_6_owed_balance_is_negative` | R-REC-6 | owed 1764.30 ⇒ −1764.30; asset unchanged |
| `r_rec_7_reading_by_match_count` | R-REC-7 | a fully flipped file is read Flipped; tie uses `preferred`, then account type |
| `r_rec_8_candidates_window` | R-REC-8 | start − 7 and end + 7 included, − 8 and + 8 excluded; scenario, deleted, Reconciled and other accounts excluded |
| `r_rec_9_exact_amount_passes` | R-REC-9 | High within 3 days, Medium within 10, none at 11; post date used when closer |
| `r_rec_9_same_day_order_does_not_matter` | R-REC-9 | proptest: shuffling lines and transaction ids gives the same pairs (by date, amount and payee) |
| `r_rec_9_payee_breaks_ties` | R-REC-9 | two equal amounts on the same day pair by payee score |
| `r_rec_10_amount_differs` | R-REC-10 | a $41.17 entry pairs with a $43.17 line (tip); not when over 25% or another sign or score < 0.5 |
| `r_rec_11_normalize_and_score` | R-REC-11 | the four sample descriptions; alias gives 1.0; empty gives 0 |
| `r_rec_12_sections` | R-REC-12 | each section filled by a purpose-built case, including AlreadyReconciled and both PfOnly kinds |
| `r_rec_13_balance_checks` | R-REC-13 | reconciled balance, opening difference, `first_time`, `difference` for the default choice and after removing a pair |
| `golden_checking_statement` | R-REC-3, R-REC-9, R-REC-12 | `fixtures/statements/checking.json` |
| `golden_card_separate_tables` | R-REC-5, R-REC-6, R-REC-7 | `fixtures/statements/card-separate.json` (charges and credits in separate sections) |
| `golden_card_single_table` | R-REC-7, R-REC-10 | `fixtures/statements/card-single.json` (one signed table) |

Every test doc comment lists `Covers: R-REC-…`.

## Pitfalls

- Money is `Decimal` everywhere; compare with `==`, never through `f64`. Only `payee_score` returns `f64`.
- No clock or time zone reads: dates come in; `tz` is a parameter.
- Greedy order is part of the rule (R-REC-9): implement the sort exactly, or the golden tests and the
  shuffle property will disagree.
- Day distance: subtracting two `jiff::civil::Date`s gives a `Span` in days, so `(a - b).get_days()` is the
  signed day count; take `.abs()`. Check it with a test across a month end and a year end.
- The two readings: flipping the lines changes which style adds up. Do not also flip the balances.
- The account type comes from `db.account(id)`; an unknown account is an error from `reconcile`
  (`Account not found.`), not a panic.
- No `unwrap`/`expect` outside tests.

## Done when

```bash
cargo test -p pf-core reconcile
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules        # R-REC-1..13 covered, nothing else missing
```

## Out of scope

- Writing anything (T29), the dialog (T30), the API and the in-memory proposal (T31), the skill and the JSON
  schema file (T32).
