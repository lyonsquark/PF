# Statement reconciliation: plan

Status: Option B chosen (accept in the app). Nothing here is built yet.

## Goal

Check an account in PFRust against a bank or card statement:

1. The statement's opening and closing balances agree with PFRust.
2. Every statement line is paired with a PFRust transaction, or is listed as missing.
3. Every PFRust transaction in the statement period is paired, or is listed as not on the statement.
4. When the owner accepts the result, the paired transactions become **Reconciled**, in one undoable step.

Today, Reconciled is only a label (it counts as cleared everywhere, see R-BAL-2). This work gives it a
meaning: "checked against a statement".

## The difficulties, and how the design handles them

| Difficulty | Design response |
|---|---|
| Getting statements needs passwords and MFA | Out of scope on purpose. The owner downloads the PDF (or CSV) by hand. No option here logs in to a bank or stores bank credentials. |
| The statement is a PDF that must be understood | Turn the PDF into a small, fixed **statement file** (JSON, schema below). The options differ only in *who* writes that file: the owner, Claude Code, or the app calling the Claude API. Everything after that is ordinary, tested Rust code with no AI in it. |
| An AI may misread the PDF | The statement file must add up: opening + sum of lines = closing, and the deposit and withdrawal totals match the statement's summary box. If it does not add up, the file is rejected before any matching, with the difference shown. This catches dropped lines, doubled lines and sign mistakes. |
| Same-day transactions in a different order | Matching never uses position or order. Lines and transactions are matched as sets, by amount and date (see the algorithm). A property test shuffles the order and requires the same result. |
| Transactions at the start or end may fall outside the statement | PFRust candidates are taken from a wider window (statement start − 7 days to end + 7 days). Unpaired PFRust rows outside the statement period are listed as "carried to the next statement", not as errors. Unpaired rows inside the period are listed separately. |
| My payee is not the statement's description | The payee is never required to match. Amount and date make the match; payee similarity only breaks ties and ranks suggestions. Pairs the owner confirms are remembered as aliases, so the next statement matches better. |
| Confirm start and end balances, and explain discrepancies | Three balance checks (below), each with a stated difference and the rows that explain it. Finishing with a non-zero difference needs an explicit choice. |

## Shared core (needed by every option)

### The statement file

One JSON file per statement. A schema file is checked in (`docs/statement-schema.json`) and is used both to
validate input and as the structured-output schema in Option C.

```json
{
  "institution": "Example Bank",
  "account_hint": "Visa ending 1234",
  "period_start": "2026-09-01",
  "period_end": "2026-09-30",
  "opening_balance": "1520.11",
  "closing_balance": "1764.30",
  "total_credits": "250.00",
  "total_debits": "494.19",
  "lines": [
    {
      "date": "2026-09-03",
      "post_date": "2026-09-04",
      "description": "SQ *BLUE BOTTLE 8812",
      "amount": "-6.50",
      "section": "Purchases",
      "reference": "2410…"
    }
  ]
}
```

* Amounts are strings, parsed as `Decimal` (never `f64`).
* `amount` is signed from the account holder's point of view, as PFRust stores it: money spent or owed more
  is negative (purchases, fees, interest, withdrawals), money received or owed less is positive (deposits,
  payments to a card, refunds, credits). See "Signs and statement layouts" below for how this is made robust.
* `section` is the heading the line came from on the statement ("Purchases", "Payments and Credits", "Fees",
  "Interest Charged", "Deposits", …), copied as printed. It is shown in the review and used by the sign checks.
* `date` is the transaction date, with `post_date` optional. Matching uses `date`, then `post_date`.
* `total_credits`, `total_debits` and `reference` are optional. They only feed the self-check and the display.

### Signs and statement layouts

Card statements differ. Some list charges and credits in separate tables, each printed as positive numbers;
some use one table with a minus sign or a "CR" mark on credits; some print the balance owed as positive, others
as negative. Nothing here assumes one layout:

1. **Per line, from its section:** the skill (or the CSV import) signs each line by what it is, using its
   section heading and marks such as "CR", "-" or brackets, not by the column it sat in. A line in
   "Payments and Credits" is positive even when printed without a sign.
2. **Balance convention, checked by arithmetic:** the Rust side tries both ways of reading the balances,
   *asset style* (opening + Σ lines = closing) and *owed style* (balances are amounts owed, so opening − Σ
   lines = closing). The one that adds up is used, and owed-style balances are turned into PFRust's sign
   (owed ⇒ negative) for the balance checks. If neither adds up, the file is rejected, as before.
3. **Whole-file sign mistakes, checked against PFRust:** flipping every line and using the other balance
   convention also adds up, so arithmetic alone cannot tell those apart. The Rust side breaks the tie by
   counting exact-amount matches against PFRust both ways, then by the account type (Credit, Loan ⇒ owed
   style). The chosen reading is shown at the top of the review ("Read as: owed balances, charges negative")
   with a **Flip signs** button, and the choice is remembered per account as a starting guess for next time.
4. **Per-section mistakes:** if one section's lines all fail to match while their sign-flipped amounts do
   match, the review flags that section ("Lines under 'Payments and Credits' may have the wrong sign") with a
   one-click fix for that section only.

A CSV form is also accepted (one line per row, the balances entered in the dialog), so a bank's own CSV export
works without any AI.

### Matching algorithm (`pf-core::reconcile`, pure, no clock)

Inputs: the statement, the account's live Reality transactions, the account's existing payee aliases, and
`today`.

1. **Candidates:** live Reality transactions of the account, not already Reconciled, dated from period start −
   7 days to period end + 7 days. Scenario rows are never candidates.
2. **Pass 1, exact amount, close date:** group statement lines and candidates by amount. Within each group,
   pair them to minimise total day difference (at most 3 days apart). Payee similarity is the tie-breaker. The
   groups are small, so an exact assignment is cheap. Confidence: high.
3. **Pass 2, exact amount, wider date:** the same, up to 10 days apart. Confidence: medium.
4. **Pass 3, amount differs:** what is left, paired when the date is within 3 days and the payee is clearly
   similar (for example a restaurant charge plus tip). Shown as "amount differs: $X on statement, $Y in PFRust".
5. **Payee similarity:** both texts are upper-cased and stripped of digits, punctuation and common prefixes
   (`POS`, `DEBIT`, `ACH`, `SQ *`, `TST*`, …), then compared by shared tokens. A saved alias (statement text →
   PFRust payee) counts as an exact match. Existing Monarch payee mappings are used as aliases too.
6. **Deterministic:** ties are broken by date, then id, so the same input always gives the same pairing.

### Result sections (what the review screen shows)

| Section | Meaning | Usual action |
|---|---|---|
| Matched | Paired, high or medium confidence | Accept (default on) |
| Amount differs | Paired, but amounts differ | Keep PFRust's amount, or change it to the statement's |
| On statement only | No PFRust transaction | Add it (payee from alias or description; category by auto-categorize, R-CAT-1) |
| In PFRust only, inside the period | Not on the statement | Usually not posted yet, a duplicate, or the wrong account. Leave it uncleared, or delete it |
| In PFRust only, outside the period | Edge timing | Ignored; carried to the next statement |
| Already reconciled | A statement line matches a transaction already reconciled earlier | Warning: possible duplicate on the statement or a double import |

The owner can unpair any pair, and pair any statement line with any candidate by hand.

### Balance checks

1. **Statement adds up:** opening + Σ lines = closing (and the credit and debit totals, when present). If it
   fails, nothing else runs.
2. **Opening balance:** PFRust's reconciled balance (sum of Reconciled transactions of the account) must equal
   the statement's opening balance. On the first reconciliation nothing is Reconciled yet, so the dialog
   offers two choices: mark every transaction dated before the statement period as Reconciled (so the
   baseline exists from then on), or skip the opening check this once. Later, a difference here means a
   Reconciled transaction was edited or deleted since the last statement. The app shows the difference; it
   cannot say which row changed, since no history is kept (the edit warning makes this rare).
3. **Closing balance:** opening + every accepted pair and added line must equal the statement's closing
   balance. The dialog shows **Difference: $0.00** live as the owner works. Finish is enabled at zero. With a
   non-zero difference, Finish asks whether to add a "Reconciliation adjustment" transaction for the
   difference (named and categorised so it is easy to find later), or to stop.

### Applying (`pf-engine::commands::reconcile`)

One `Engine::apply` named "Reconcile Statement":

* Accepted pairs become Reconciled. A pair whose amount differs is changed only if the owner chose that.
* Added lines are inserted as Reconciled transactions in Reality.
* An adjustment transaction is added if the owner chose one.
* Confirmed pairs whose payee and description differ are saved as aliases (an `ImportMapping` of a new type,
  `statementPayee`, per account).

Edit → Undo Reconcile Statement (⌘Z) reverses all of it in one step, as long as it is still in the undo
history. The history is in memory only (R-UND-4), so after a restart the statuses are set back by hand.

### No new data (decision)

Reconciliation is recorded **only as the Reconciled status** on transactions. There are no reconciliation
tables, no schema migration, and no change to the Markdown backup format.

* The pending proposal from the skill lives in memory in the running app. Quitting before Finish drops it;
  the skill is run again.
* A statement fed in twice is caught by the "Already reconciled" section, not by a stored file hash.
* Payee aliases are stored as `ImportMapping` rows of a new type, `statementPayee`, in the existing table.
  **Check before release:** the Swift app's backup import must accept (or ignore) an unknown mapping type,
  since these rows appear in the Markdown backup's ImportMappings section.
* Given up on purpose: undoing an old reconciliation as one unit after a restart, a history of past
  statements, and naming the changed row behind an opening-balance difference.
* `PF.store` is never touched.

### Testing

* Unit and property tests in `pf-core` for the self-check, each matching pass, and each result section.
* Property tests: shuffling same-day order changes nothing; moving a candidate just outside the period moves
  it to "outside the period"; random payee noise does not break exact-amount pairs.
* Golden tests with two or three anonymised statement files under `fixtures/statements/`.
* kittest for the review dialog: Difference reaches $0.00, Finish is one undo step, and manual pairing works.
* No AI call in any test. Option B and C extraction is checked by hand against real statements.

## Workflow options

The options differ only in how the PDF becomes a statement file, and who looks at the result first.

### Option A: the owner supplies the statement file (no AI)

1. Download the statement as PDF, plus CSV or OFX if the bank offers it.
2. In PFRust, open the account, then **Reconcile…**, then choose the CSV (or a statement JSON).
3. Type the opening and closing balances from the PDF (two fields).
4. Review, fix and finish in the dialog.

* **Pros:** smallest build. No network, no AI, nothing leaves the Mac. Fully deterministic. Works today with
  most banks' CSV exports.
* **Cons:** the owner reads the balances off the PDF. A bank with PDF-only statements needs some other way to
  get a CSV.
* **New dependencies:** none. CSV parsing already exists (R-CSV-1). OFX would need a small hand-written
  parser.

### Option B: Claude Code reads the PDF and hands a proposal to the app (chosen)

Decision (owner): **the result is accepted only in the app**, so the owner has to look at it there. The
skill and the local API prepare and explain; they never change a transaction's status. The API has no route
that marks anything Reconciled.

1. Download the PDF.
2. In a terminal, run a project skill: `/reconcile ~/Downloads/visa-2026-09.pdf "Visa"`.
3. Claude Code reads the PDF (it reads PDFs directly) and writes the statement file against the schema. It
   runs the self-check, and if the file does not add up it re-reads the PDF to find the missing or doubled
   line.
4. It sends the file to the running app: `POST /api/accounts/{name}/reconcile/proposal`. The app runs the
   matching in Rust, stores the proposal, and returns the match report and the balance checks. No
   transaction changes.
5. The skill explains the report in plain words, for example "the $43.17 line on 9/14 is probably your
   'Trader Joe's' entry of $41.17; the statement adds a $2.00 fee". It can look up nearby transactions with
   `GET /api/accounts/{name}/transactions?from=…&to=…` to explain a gap. It ends by telling the owner to open
   the account in PFRust.
6. The app shows a "Statement ready to review" banner on the account. The owner opens the review dialog
   (the same one as Option A), adjusts pairs if needed, and presses Finish. **Only Finish changes the books**,
   in one "Reconcile Statement" undo step.

Local API changes (all read-only, except storing the proposal):

| Route | Purpose |
|---|---|
| `GET /api/accounts/{name}/transactions?from=YYYY-MM-DD&to=YYYY-MM-DD` | The account's visible transactions in a date range, each with its `id`. `n` keeps its current meaning when no range is given, so existing callers are unaffected (R-API-7 parity). |
| `POST /api/accounts/{name}/reconcile/proposal` | Body: the statement file. Rejects a file that does not add up (with the difference). Otherwise stores it as the account's pending proposal (replacing an older one) and returns the report: sections, pairs, balance checks, Difference. |
| `GET /api/accounts/{name}/reconcile/proposal` | The stored proposal's current report, recomputed against today's transactions (the owner may have edited some since). |
| `DELETE /api/accounts/{name}/reconcile/proposal` | Drops the pending proposal. |

The pending proposal is kept in memory in the running app (one per account, a newer one replaces it). A
restart drops it, and the skill is simply run again. Finishing or cancelling in the app removes it.

* **Pros:** reading the PDF and explaining differences is AI work, and the decision stays in the app.
  No API key or network code inside PFRust. Uses the owner's existing Claude Code setup. The AI never sees bank
  credentials. It sees only the PDF the owner hands it. The API cannot change the books, so a mistaken or
  repeated skill run is harmless. Little new Rust code beyond the shared core: four routes and a stored
  proposal.
* **Cons:** two tools (terminal and app). The statement PDF is sent to Anthropic as part of the Claude Code
  session. The app must be running for step 4 (the API already runs inside it).
* **New dependencies:** none in Rust. One skill file (`.claude/skills/reconcile/SKILL.md`) with the schema,
  the self-check loop, and the steps above.

### Option C: the app calls the Claude API itself

1. Download the PDF.
2. In PFRust, open **Reconcile…** and drop the PDF on the dialog.
3. The app sends the PDF to the Claude Messages API and gets the statement file back as structured JSON. It
   runs the self-check (and retries once if the file does not add up), then shows the same review.

Technical notes (checked against the current Claude API documentation):

* There is no official Rust SDK, so this is a raw HTTPS `POST /v1/messages` call.
* The PDF goes in as a base64 `document` content block (up to 32 MB and 600 pages per request).
* The reply is constrained to the statement schema with structured outputs (`output_config.format` with a JSON
  schema, `additionalProperties: false` on every object). The schema cannot use numeric or length
  constraints, so the Rust side validates those.
* Model `claude-opus-5-5` at `medium` effort. Refusals are handled with the server-side `fallbacks: "default"`
  option, and every `stop_reason` is checked (`max_tokens` and `refusal` mean the JSON may be incomplete).
* Rough cost: a 5 to 10 page statement is a few tens of thousands of input tokens, plus a few thousand output
  tokens. At $4 / $20 per million tokens that is roughly $0.10 to $0.30 per statement. Measure it on real
  statements before relying on this number.
* The call runs off the UI thread, with a progress line and Cancel. The app has no async runtime, so this is a
  plain thread and a channel.
* The API key is stored in the macOS Keychain (through the `security` tool), never in the database or the
  backup.

* **Pros:** one place, one click, no terminal.
* **Cons:** PFRust gains network code, API-key handling and AI error handling. Each statement costs money. The
  PDF is sent to Anthropic from inside the finance app. Per `CLAUDE.md`, the new dependencies (an HTTPS client
  such as `ureq` with `rustls`; base64 can be written by hand) need the owner's approval before the task
  starts.
* **New dependencies:** an HTTPS client, and possibly a base64 crate.

### Comparison

| | A: owner's file | B: Claude Code proposal | C: built-in API |
|---|---|---|---|
| Reads PDFs | No (CSV/OFX + 2 typed balances) | Yes | Yes |
| AI inside PFRust | No | No | Yes |
| New Rust dependencies | None | None | HTTPS client |
| API key in PFRust | No | No | Yes (Keychain) |
| Cost per statement | 0 | Claude Code usage | ≈ $0.10–0.30 (to measure) |
| Explains discrepancies in words | No | Yes | Only the numbers |
| Extra work beyond the core | Small | Small–medium | Medium–large |

## Recommendation and decision

The owner chose **Option B, with acceptance in the app**. Build the shared core and the review dialog first:
Option B feeds that dialog, and the core is where correctness lives. Loading a statement file or CSV
directly (Option A) comes almost free with the dialog and is useful for testing, so it stays in. Option C can
follow later if the terminal step becomes a nuisance. It reuses the schema and the review dialog unchanged.

## Proposed tasks

| Task | Title | Deps | Notes |
|---|---|---|---|
| T29 | `pf-core::reconcile` | – | Statement model and parsing (JSON and CSV), self-check with both balance conventions, sign tie-break against PFRust and per-section sign flags, matching passes, sections, balance checks. New rule IDs `R-REC-*`. Property and golden tests, including the three statement layouts. |
| T30 | Reconcile engine commands | T29 | Apply (one step: statuses, added lines, adjustment, aliases), mark-before-period as Reconciled for a first reconciliation, payee aliases (`statementPayee` mapping; check the Swift backup import with an unknown mapping type). No schema change. |
| T31 | Reconcile dialog | T30 | Account action row "Reconcile…", load file, "Read as" line with Flip signs and per-section fixes, review sections, manual pair and unpair, live Difference, Finish with optional adjustment. Warn-then-allow on editing or deleting a Reconciled transaction (account table, dialogs). kittest. |
| T32 | Option B: proposal API | T31 | `GET …/transactions?from&to` with ids; `POST`/`GET`/`DELETE …/reconcile/proposal`; proposal held in memory per account; "Statement ready to review" banner that opens the dialog with the proposal. No API route changes a status. API tests on port 0 with temp databases. |
| T32b | Option B: `/reconcile` skill | T32 | `.claude/skills/reconcile/SKILL.md`: read the PDF, split a multi-account PDF into one statement file per account, sign each line by its section, self-check and re-read loop, post the proposal, explain the report, point the owner to the app. Tried by hand on two real statements. |
| T33 | Option C: Claude API extraction (optional) | T31 | Blocked until the owner approves the HTTPS dependency. Keychain key, background call, retry on a failed self-check. |

## Decisions (owner, 2026-10-29)

1. **Accounts:** credit cards and checking from the start. Test statements: at least one card statement with
   separate charge and credit tables, one card statement with a single signed table, and one checking
   statement. Sign handling must not assume a layout (see "Signs and statement layouts").
2. **Editing a reconciled transaction:** warn, then allow. Changing the amount or date of, or deleting, a
   Reconciled transaction asks "This transaction is Reconciled. Change it anyway?". This keeps parity with
   Swift, which never locked. The next opening-balance check shows any difference this causes.
5. **Storage:** status only, no reconciliation tables and no database migration. Undo right after Finish
   reverses the whole reconciliation; after a restart, statuses are changed back by hand.
3. **Combined statements:** one proposal per account. The skill splits a multi-account PDF into one statement
   file per account and posts each to its account; each is reviewed and finished on its own.
4. **Transfers between own accounts:** nothing special. Each account is reconciled from its own statement.
