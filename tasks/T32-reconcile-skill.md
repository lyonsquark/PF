# T32: The `/reconcile` Claude Code skill and the statement schema

**Phase:** 5 · **Deps:** T31 · **Files:** `.claude/skills/reconcile/SKILL.md`, `docs/statement-schema.json` · **Size:** small (no Rust)

## Goal

A project skill that turns a statement PDF into a statement file, checks it, posts it to the running app as
a proposal (T31), and explains the report in plain words. It never changes the books; the owner finishes in
the app. It never logs in anywhere.

## Read first

- `docs/reconciliation-plan.md`: "Option B", "Signs and statement layouts", "Decisions".
- `docs/business-rules.md` R-REC-1..13 and R-API-11..14.
- An existing skill in `~/.claude/skills/` or the Claude Code docs for the `SKILL.md` frontmatter format
  (`name`, `description`, optional `allowed-tools`).

## Deliverables

1. `docs/statement-schema.json`: JSON Schema (draft 2020-12) for R-REC-1. Every object has
   `"additionalProperties": false` and a `required` list; amounts and dates are strings. No numeric or length
   constraints (so the same file works as a Claude structured-output schema for a later Option C).
2. `.claude/skills/reconcile/SKILL.md`, invoked as `/reconcile <pdf path> [account name]`. Its instructions,
   in order:
   1. Check the app is running: `curl -s localhost:8989/api/db-path`. If it fails, tell the owner to open
      PFRust and stop.
   2. If no account name is given, list accounts (`GET /api/accounts/balances?format=text`) and ask which one.
   3. Read the PDF. If it holds several accounts, make one statement file per account and ask which PFRust
      account each belongs to (owner decision: one proposal per account).
   4. Write the statement file in the scratchpad, following `docs/statement-schema.json`. Sign **each line by
      what it is** (spent or owed more ⇒ negative; received, paid to the card, refunded ⇒ positive), using its
      section heading and marks like `CR`, `-` or brackets, not the column it sat in. Copy `section` exactly
      as printed. Use the transaction date as `date` and the posting date as `post_date` when both are shown.
      Copy balances and totals exactly as printed.
   5. Check the arithmetic locally before posting (opening ± Σ lines = closing, R-REC-3; totals, R-REC-4). If
      it does not add up, re-read the PDF to find the missing, doubled or mis-signed line, and say what was
      fixed. After two failed attempts, stop and show the owner the difference and the lines in doubt.
   6. `POST /api/accounts/{name}/reconcile/proposal` with the file (`curl --data-binary @file`). On a 400,
      show the message; the section hint (R-REC-3) says which section to re-check.
   7. Explain the report (`format=text` and the JSON for detail): the balance checks first, then each
      non-empty section other than Matched, in plain words. For "Amount differs" and "On statement only",
      look up nearby transactions with `GET …/transactions?from=…&to=…` and suggest the likely cause (tip,
      fee, foreign exchange, not entered yet, entered on another account).
   8. End with: "Open {account} in PFRust and click Review on the banner to check and finish." Never claim
      the statement is reconciled.
   Also state plainly in the skill: never ask for or use bank credentials; never call a route that changes
   transactions; never edit the PFRust database file directly; keep the statement file only in the
   scratchpad.
3. `README.md`: a short "Reconciling a statement" section (Option B steps for the owner, and the CSV path of
   Option A).

## Tests

No automated tests (no Rust). Owner checks, added to `tasks/MANUAL_CHECKS.md`:

| Check | Expected |
|---|---|
| `/reconcile` on a card statement with separate charge and credit tables | proposal posted, Difference $0.00 or explained |
| `/reconcile` on a card statement with one signed table | same |
| `/reconcile` on a checking statement | same |
| `/reconcile` with PFRust closed | stops with "open PFRust" |
| A deliberately wrong PFRust amount | listed under "Amount differs" with a sensible explanation |

## Done when

- The schema validates the three files in `fixtures/statements/` (check with any JSON Schema validator, or
  by hand against the schema).
- The owner has run the three statement checks above.
- `cargo xtask rules` still passes (the skill adds no IDs).

## Out of scope

- Option C (the app calling the Claude API): not planned; it would reuse `docs/statement-schema.json`.
