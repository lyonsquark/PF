---
name: reconcile
description: Turn a bank or credit-card statement PDF into a statement file, check that it adds up, and post it to the running PF app as a reconciliation proposal that the owner reviews and finishes in the app. Use when the user runs /reconcile <pdf path> [account name] or asks to reconcile an account against a statement. Requires PF to be running (local API on port 8989).
allowed-tools: Bash(curl:*), Read, Write
---

# Reconcile a statement

`/reconcile <pdf path> [account name]` reads a statement PDF, writes a statement file, checks its arithmetic, posts it to
PF as a **proposal**, and explains the result. The owner then opens the account in PF, clicks **Review** on the
banner, and presses Finish there. Nothing is reconciled until they do.

## Rules (read first)

- Never ask for or use bank credentials, passwords or codes. The owner downloads the PDF; you only read it.
- Never call a route that changes transactions. The only routes you use are the ones below; `POST`, `GET` and `DELETE`
  on `reconcile/proposal` change no transaction.
- Never edit the PF database file directly.
- Keep the statement file only in the scratchpad directory. Never claim the statement is reconciled.

## Steps

1. **Check the app is running**: `curl -s localhost:8989/api/db-path`. If this fails, tell the owner to open PF and
   stop.
2. **Pick the account.** If no account name was given, list accounts with
   `curl -s "localhost:8989/api/accounts/balances?format=text"` and ask which one. Replace spaces with `%20` in URLs.
3. **Read the PDF.** If it holds several accounts, make one statement file per account and ask which PF account each
   belongs to (one proposal per account).
4. **Write the statement file** in the scratchpad, following `docs/statement-schema.json`:
   - Sign **each line by what it is**: money spent or owed more is negative; money received, paid to the card or
     refunded is positive. Decide from the section heading and marks such as `CR`, `-` or brackets, **not** from the
     column the amount sat in. Some statements list charges and credits in separate tables, others in one signed table;
     handle both.
   - Copy `section` exactly as printed. Use the transaction date as `date`, and the posting date as `post_date` when
     both are shown.
   - Copy the opening and closing balances, and any printed credit and debit totals, exactly as printed. Do not change
     their signs.
5. **Check the arithmetic before posting.** Opening ± the sum of the lines must equal the closing balance (a checking
   statement adds the lines, a card statement subtracts them from the amount owed), and any printed totals must match
   the lines. If it does not add up, re-read the PDF for the missing, doubled or mis-signed line, and say what you fixed.
   After two failed attempts, stop and show the owner the difference and the lines in doubt.
6. **Post it**: `curl -s -X POST --data-binary @FILE "localhost:8989/api/accounts/{name}/reconcile/proposal"`.
   On a 400, show the message. The "Statement does not add up" message may say which section's signs to flip; re-check
   that section.
7. **Explain the report.** Fetch the readable form with
   `curl -s "localhost:8989/api/accounts/{name}/reconcile/proposal?format=text"` (and the JSON for detail). Start with the
   balance checks (opening, closing, Difference), then each non-empty section other than Matched in plain words:
   - *Amount differs*: the amounts disagree slightly (a tip, a fee, foreign exchange).
   - *On statement only*: the bank has it and PF does not (not entered yet, or entered on another account).
   - *In PF only, in this period*: PF has it and the statement does not.
   - *Already reconciled*, *In PF only, outside this period*: usually fine.
   For Amount differs and On statement only, look nearby with
   `curl -s "localhost:8989/api/accounts/{name}/transactions?from=YYYY-MM-DD&to=YYYY-MM-DD&format=text"` and suggest the
   likely cause.
8. **End with**: "Open {account} in PF and click Review on the banner to check and finish."

To drop a proposal: `curl -s -X DELETE "localhost:8989/api/accounts/{name}/reconcile/proposal"`.
