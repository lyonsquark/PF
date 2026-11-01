# Swift parity captures (T26)

These files record the read-only HTTP replies of the **Swift** PF app for the data in
`fixtures/SampleData2.md`. `crates/pf-api/tests/parity.rs` loads the same data into Rust PF and
compares its replies with these captures.

## Files

- `manifest.tsv`: one line per request: `name`, `method`, `path+query`, `status`, `content-type`.
- `<name>.body`: the raw reply body for that line.
- `meta.txt`: the capture date, the macOS time zone (the Markdown import converts timestamps with it),
  the Swift app's git commit, and the fixture name.

## How to re-capture

1. Quit Rust PF (both apps use port 8989).
2. Start the **Swift** app and export a backup first: importing wipes its data.
3. In the Swift app, Settings > Import from Markdown, with `fixtures/SampleData2.md`.
4. Run `scripts/capture_swift_parity.sh` and answer `y` at the prompt.
5. Run `cargo test -p pf-api --test parity`.
6. Commit the changed files.

Re-capture when the Swift app's API or export format changes. Do not capture from real user data.

## Comparison rules

- Status codes and content types must match (case and spaces ignored).
- JSON bodies are compared as parsed values, with numbers within 0.005 (Swift converts Decimal to
  Double). Key order is ignored. Whitespace is ignored: Swift's pretty printer writes `"key" : value`,
  serde_json writes `"key": value`.
- Text and Markdown bodies must match byte for byte after line endings are normalised.
- `/api/export/markdown` is compared after removing the whole `## Settings` section from both sides,
  because the bookmark values differ.
- Swift does not order transactions that fall on the same day, and a row's running balance depends on that
  order. For the transaction lists (JSON, text and Markdown), each run of same-day rows is compared as a set.
  The first row of a run keeps its running balance, which is the balance after the whole day and does not
  depend on the order; the other rows' balances are not compared.
- The Markdown export's trailing blank lines are ignored.

## Not captured

- `GET /api/db-path` (machine specific).
- `POST .../import` (it changes the Swift data; T12's own tests cover it).
