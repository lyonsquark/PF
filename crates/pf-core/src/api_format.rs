//! Text, Markdown and JSON output shared by the CLI (T11) and the HTTP API (T12), so the two
//! cannot drift. Port of Swift `Services/APIHandlers.swift`. Pure: no IO, no clock.

mod json;
mod reconcile;
#[cfg(test)]
mod tests;

pub use json::{JsonValue, to_apple_json};
pub use reconcile::format_reconcile_report;

use jiff::civil::Date;
use pf_model::{AccountId, Db, ScenarioId, Transaction, TransactionId, TransactionStatus};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::balance::{account_balance, is_visible, running_balances};
use crate::format::format_currency;

/// Output format of a reply. The CLI defaults to `Text`; the API defaults to `Json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Pretty JSON with sorted keys.
    Json,
    /// Aligned plain text.
    Text,
    /// Markdown tables.
    Markdown,
}

/// R-API-3: case-insensitive `text`, `markdown` or `md`. Anything else (including `json` and "")
/// is JSON.
pub fn parse_format(s: &str) -> Format {
    match s.to_lowercase().as_str() {
        "text" => Format::Text,
        "markdown" | "md" => Format::Markdown,
        _ => Format::Json,
    }
}

/// R-API-4: `None`, "" or "reality" (any case) is Reality. A live scenario is found by name, any
/// case. Anything else is `Err("Scenario '{name}' not found.")`.
pub fn resolve_scenario(db: &Db, name: Option<&str>) -> Result<Option<ScenarioId>, String> {
    let Some(name) = name else {
        return Ok(None);
    };
    if name.is_empty() || name.to_lowercase() == "reality" {
        return Ok(None);
    }
    match db.scenario_by_name_ci(name) {
        Some(s) => Ok(Some(s.id)),
        None => Err(format!("Scenario '{name}' not found.")),
    }
}

/// An error reply. JSON is `{"error":"<msg>"}` (compact, escaped); text and Markdown are
/// `Error: <msg>`.
pub fn error_body(msg: &str, f: Format) -> String {
    match f {
        Format::Json => to_apple_json(&obj(vec![("error", jstr(msg))]), false, false),
        Format::Text | Format::Markdown => format!("Error: {msg}"),
    }
}

/// One account's balance row for the balances reply.
#[derive(Debug, Clone, PartialEq)]
pub struct BalanceEntry {
    /// Account name.
    pub name: String,
    /// Raw account type, e.g. `checking`.
    pub account_type: String,
    /// Institution name.
    pub institution: String,
    /// R-BAL-1 balance under the requested scenario.
    pub balance: Decimal,
    /// External accounts are shown with a marker and left out of the total.
    pub is_external: bool,
}

/// R-SCN-1, R-BAL-1: one entry per live account, sorted by name, under `scenario`.
pub fn balance_entries(db: &Db, scenario: Option<ScenarioId>) -> Vec<BalanceEntry> {
    db.accounts_by_name()
        .into_iter()
        .filter_map(|a| balance_entry(db, a.id, scenario))
        .collect()
}

/// The balance row of one live account, or `None` when there is no such account.
pub fn balance_entry(
    db: &Db,
    account: AccountId,
    scenario: Option<ScenarioId>,
) -> Option<BalanceEntry> {
    let a = db.account(account).filter(|a| !a.is_deleted)?;
    Some(BalanceEntry {
        name: a.name.clone(),
        account_type: a.account_type.as_str().to_string(),
        institution: a.institution.clone(),
        balance: account_balance(db, account, scenario),
        is_external: a.is_external,
    })
}

/// Sum of the non-external balances.
fn balances_total(entries: &[BalanceEntry]) -> Decimal {
    entries
        .iter()
        .filter(|e| !e.is_external)
        .map(|e| e.balance)
        .sum()
}

/// R-API-6: the balances reply in `f`.
pub fn format_balances(entries: &[BalanceEntry], f: Format) -> String {
    let total = balances_total(entries);
    match f {
        Format::Text => {
            let mut lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    let ext = if e.is_external { " [external]" } else { "" };
                    format!(
                        "{} ({}){ext}: {}",
                        e.name,
                        e.institution,
                        format_currency(e.balance)
                    )
                })
                .collect();
            lines.push(String::new());
            lines.push(format!("Total: {}", format_currency(total)));
            lines.join("\n")
        }
        Format::Markdown => {
            let mut lines = vec![
                "| Account | Institution | Type | Balance |".to_string(),
                "| --- | --- | --- | ---: |".to_string(),
            ];
            for e in entries {
                let name = if e.is_external {
                    format!("{} *(ext)* ", e.name)
                } else {
                    e.name.clone()
                };
                lines.push(format!(
                    "| {name} | {} | {} | {} |",
                    e.institution,
                    e.account_type,
                    format_currency(e.balance)
                ));
            }
            lines.push(format!(
                "| **Total** | | | **{}** |",
                format_currency(total)
            ));
            lines.join("\n")
        }
        Format::Json => {
            let accounts = entries
                .iter()
                .map(|e| {
                    obj(vec![
                        ("name", jstr(&e.name)),
                        ("type", jstr(&e.account_type)),
                        ("institution", jstr(&e.institution)),
                        ("balance", num(e.balance)),
                        ("external", JsonValue::Bool(e.is_external)),
                    ])
                })
                .collect();
            let v = obj(vec![
                ("accounts", JsonValue::Arr(accounts)),
                ("total", num(total)),
            ]);
            to_apple_json(&v, true, true)
        }
    }
}

/// One transaction row of the transactions reply.
#[derive(Debug, Clone, PartialEq)]
pub struct TxLine {
    /// Transaction id.
    pub tx: TransactionId,
    /// Calendar date.
    pub date: Date,
    /// Payee text.
    pub payee: String,
    /// Signed amount.
    pub amount: Decimal,
    /// R-BAL-7 running balance over all visible rows of the account.
    pub running_balance: Decimal,
    /// Reconciliation status.
    pub status: TransactionStatus,
    /// Category name (the plain name, without the parent prefix).
    pub category: Option<String>,
    /// Memo, only when non-empty.
    pub memo: Option<String>,
    /// Scenario name, when the row belongs to a scenario.
    pub scenario: Option<String>,
    /// Context names, in the transaction's order.
    pub contexts: Vec<String>,
}

/// R-API-7: visible rows of `account` under `scenario`, sorted by (date, id) descending, first `n`.
/// The running balance is accumulated over every visible row, oldest first.
pub fn transaction_lines(
    db: &Db,
    account: AccountId,
    scenario: Option<ScenarioId>,
    n: usize,
) -> Vec<TxLine> {
    lines_where(db, account, scenario, |_| true, Some(n))
}

/// R-API-11: every visible row of `account` with `from` <= date <= `to` (either bound optional),
/// newest first, with the running balance over all visible rows.
pub fn transaction_lines_between(
    db: &Db,
    account: AccountId,
    scenario: Option<ScenarioId>,
    from: Option<Date>,
    to: Option<Date>,
) -> Vec<TxLine> {
    let keep = |t: &Transaction| from.is_none_or(|f| t.date >= f) && to.is_none_or(|e| t.date <= e);
    lines_where(db, account, scenario, keep, None)
}

/// Visible rows passing `keep`, newest first, at most `limit`.
fn lines_where(
    db: &Db,
    account: AccountId,
    scenario: Option<ScenarioId>,
    keep: impl Fn(&Transaction) -> bool,
    limit: Option<usize>,
) -> Vec<TxLine> {
    let visible: Vec<&Transaction> = db
        .transactions_for_account(account)
        .filter(|t| is_visible(t, scenario))
        .collect();
    let running = running_balances(&visible);
    let mut newest_first: Vec<&Transaction> = visible.into_iter().filter(|t| keep(t)).collect();
    newest_first.sort_by_key(|t| std::cmp::Reverse((t.date, t.id)));
    newest_first
        .into_iter()
        .take(limit.unwrap_or(usize::MAX))
        .map(|t| TxLine {
            tx: t.id,
            date: t.date,
            payee: t.payee.clone(),
            amount: t.amount,
            running_balance: running.get(&t.id).copied().unwrap_or(Decimal::ZERO),
            status: t.status,
            category: t
                .category_id
                .and_then(|c| db.category(c))
                .map(|c| c.name.clone()),
            memo: t.memo.clone().filter(|m| !m.is_empty()),
            scenario: t
                .scenario_id
                .and_then(|s| db.scenario(s))
                .map(|s| s.name.clone()),
            contexts: t
                .context_ids
                .iter()
                .filter_map(|c| db.context(*c))
                .map(|c| c.name.clone())
                .collect(),
        })
        .collect()
}

/// R-API-7: the transactions reply in `f`. `count` is the number of lines given.
pub fn format_transactions(account_name: &str, lines: &[TxLine], f: Format) -> String {
    format_transactions_impl(account_name, lines, f, false)
}

/// R-API-11: like [`format_transactions`], with an `id` key on every JSON transaction.
pub fn format_transactions_with_ids(account_name: &str, lines: &[TxLine], f: Format) -> String {
    format_transactions_impl(account_name, lines, f, true)
}

fn format_transactions_impl(account_name: &str, lines: &[TxLine], f: Format, ids: bool) -> String {
    match f {
        Format::Text => {
            let header = format!("{account_name} — Last {} transactions", lines.len());
            let columns = format!(
                "{}  {}  {}  {}  Payee",
                pad("Date", 10),
                pad("Amount", 12),
                pad("Balance", 12),
                pad("Status", 10)
            );
            let rule = "-".repeat(columns.chars().count() + 20);
            let mut out = vec![header, String::new(), columns, rule];
            out.extend(lines.iter().map(text_line));
            out.join("\n")
        }
        Format::Markdown => {
            let mut out = vec![
                format!("## {account_name} — Last {} Transactions", lines.len()),
                String::new(),
                "| Date | Payee | Amount | Balance | Status | Category | Contexts | Memo |"
                    .to_string(),
                "| --- | --- | ---: | ---: | --- | --- | --- | --- |".to_string(),
            ];
            out.extend(lines.iter().map(markdown_line));
            out.join("\n")
        }
        Format::Json => {
            let txs = lines.iter().map(|l| json_tx(l, ids)).collect();
            let v = obj(vec![
                ("account", jstr(account_name)),
                ("count", JsonValue::Num(lines.len() as f64)),
                ("transactions", JsonValue::Arr(txs)),
            ]);
            to_apple_json(&v, true, true)
        }
    }
}

/// One text row: the padded columns, then the optional category, contexts, memo and scenario.
fn text_line(l: &TxLine) -> String {
    let mut s = format!(
        "{}  {}  {}  {}  {}",
        ymd(l.date),
        pad(&format_currency(l.amount), 12),
        pad(&format_currency(l.running_balance), 12),
        pad(l.status.label(), 10),
        l.payee
    );
    if let Some(c) = &l.category {
        s.push_str(&format!("  [{c}]"));
    }
    if !l.contexts.is_empty() {
        s.push_str(&format!("  {{#{}}}", l.contexts.join(", #")));
    }
    if let Some(m) = l.memo.as_deref().filter(|m| !m.is_empty()) {
        s.push_str(&format!("  // {m}"));
    }
    if let Some(sc) = &l.scenario {
        s.push_str(&format!("  ({sc})"));
    }
    s
}

/// One Markdown table row.
fn markdown_line(l: &TxLine) -> String {
    let scenario = l
        .scenario
        .as_ref()
        .map(|s| format!(" *({s})*"))
        .unwrap_or_default();
    let contexts = l
        .contexts
        .iter()
        .map(|c| format!("`#{c}`"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "| {} | {}{} | {} | {} | {} | {} | {} | {} |",
        ymd(l.date),
        l.payee,
        scenario,
        format_currency(l.amount),
        format_currency(l.running_balance),
        l.status.label(),
        l.category.as_deref().unwrap_or(""),
        contexts,
        l.memo.as_deref().unwrap_or("")
    )
}

/// One transaction object of the JSON reply. Optional keys are left out when absent or empty.
fn json_tx(l: &TxLine, with_id: bool) -> JsonValue {
    let mut pairs = vec![
        ("amount", num(l.amount)),
        ("date", jstr(&ymd(l.date))),
        ("payee", jstr(&l.payee)),
        ("runningBalance", num(l.running_balance)),
        ("status", jstr(l.status.as_str())),
    ];
    if with_id {
        pairs.push(("id", JsonValue::Num(l.tx.raw() as f64)));
    }
    if let Some(c) = &l.category {
        pairs.push(("category", jstr(c)));
    }
    if !l.contexts.is_empty() {
        let names = l.contexts.iter().map(|c| jstr(c)).collect();
        pairs.push(("contexts", JsonValue::Arr(names)));
    }
    if let Some(m) = l.memo.as_deref().filter(|m| !m.is_empty()) {
        pairs.push(("memo", jstr(m)));
    }
    if let Some(sc) = &l.scenario {
        pairs.push(("scenario", jstr(sc)));
    }
    obj(pairs)
}

/// The scenarios reply: live scenarios sorted by name, or "(None)" when there are none.
pub fn format_scenarios(db: &Db, f: Format) -> String {
    let list = db.scenarios_by_name();
    match f {
        Format::Text => {
            let mut out = vec!["Available Scenarios:".to_string()];
            if list.is_empty() {
                out.push("(None)".to_string());
            }
            out.extend(list.iter().map(|s| format!("- {}", s.name)));
            out.join("\n")
        }
        Format::Markdown => {
            let mut out = vec!["## Available Scenarios".to_string(), String::new()];
            if list.is_empty() {
                out.push("*(None)*".to_string());
            }
            out.extend(list.iter().map(|s| format!("- **{}**", s.name)));
            out.join("\n")
        }
        Format::Json => {
            let items = list
                .iter()
                .map(|s| {
                    obj(vec![
                        ("colorHex", opt_str(s.color_hex.as_deref())),
                        ("name", jstr(&s.name)),
                        ("notes", opt_str(s.notes.as_deref())),
                    ])
                })
                .collect();
            to_apple_json(&obj(vec![("scenarios", JsonValue::Arr(items))]), true, true)
        }
    }
}

/// R-API-8: the CSV import summary. Text and Markdown share the same block.
pub fn format_import_result(
    account_name: &str,
    imported: usize,
    dups: usize,
    unmapped: usize,
    f: Format,
) -> String {
    match f {
        Format::Json => {
            let v = obj(vec![
                ("account", jstr(account_name)),
                ("imported", JsonValue::Num(imported as f64)),
                ("skippedDuplicates", JsonValue::Num(dups as f64)),
                ("skippedUnmapped", JsonValue::Num(unmapped as f64)),
            ]);
            to_apple_json(&v, true, true)
        }
        Format::Text | Format::Markdown => format!(
            "Import Complete for {account_name}:\nAdded:   {imported}\nSkipped: {dups} (duplicates)\nSkipped: {unmapped} (unmapped payees)"
        ),
    }
}

/// Pads with spaces to `n` characters, or truncates to `n` (Swift `padding(toLength:)`).
fn pad(s: &str, n: usize) -> String {
    let cut: String = s.chars().take(n).collect();
    let len = cut.chars().count();
    format!("{cut}{}", " ".repeat(n - len))
}

/// `YYYY-MM-DD`.
fn ymd(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

/// A JSON string.
fn jstr(s: &str) -> JsonValue {
    JsonValue::Str(s.to_string())
}

/// An optional JSON string: `null` when absent.
fn opt_str(s: Option<&str>) -> JsonValue {
    s.map_or(JsonValue::Null, jstr)
}

/// A decimal as a JSON number.
fn num(d: Decimal) -> JsonValue {
    JsonValue::Num(d.to_f64().unwrap_or(0.0))
}

/// A JSON object from `(key, value)` pairs.
fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    JsonValue::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
