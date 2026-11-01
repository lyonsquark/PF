//! R-MD-1: writes a `Db` in the exact Swift backup format (`docs/markdown-format.md` → "Layout").

use std::collections::BTreeMap;

use jiff::tz::TimeZone;
use pf_model::{Category, Db, ImportMapping, Transaction};

use super::escape::esc;
use super::values::{fmt_date, fmt_decimal, fmt_timestamp, parse_date};

const TITLE: &str = "# Personal Finance Data\n\n";
const CONTEXT_HEADER: &[&str] = &["Name", "Color", "Notes"];
/// The contexts table when any context is archived (Rust addition, see `docs/markdown-format.md`).
const CONTEXT_HEADER_ARCHIVED: &[&str] = &["Name", "Color", "Notes", "Archived"];
const CATEGORY_HEADER: &[&str] = &["Name", "Type", "Icon", "Notes", "Parent", "Match Patterns"];
const SCENARIO_HEADER: &[&str] = &["Name", "Color", "Notes"];
const ACCOUNT_HEADER: &[&str] = &["Name", "Type", "Institution", "Currency", "External"];
const OBLIGATION_HEADER: &[&str] = &[
    "Name",
    "Status",
    "Target",
    "Balance",
    "Due Date",
    "Created Date",
    "Updated Date",
    "Notes",
    "Account",
    "Context",
];
const BUDGET_HEADER: &[&str] = &[
    "Name",
    "Type",
    "Amount",
    "Start Date",
    "End Date",
    "Frequency",
    "Specific Months",
    "Notes",
    "Category",
    "Account",
];
const MAPPING_HEADER: &[&str] = &["Type", "Source Value", "Target Value", "Negate Amounts"];
const TRANSACTION_HEADER: &[&str] = &[
    "Date",
    "Payee",
    "Category",
    "Amount",
    "Memo",
    "Scenario",
    "Contexts",
    "Status",
    "Attachment",
];
const SETTINGS_HEADER: &[&str] = &["Key", "Value"];

/// Setting keys (`docs/data-model.md` → "Settings keys").
const KEY_PROJ_SCENARIO: &str = "lastProjectionScenario";
const KEY_PROJ_START: &str = "lastProjectionStartDate";
const KEY_PROJ_END: &str = "lastProjectionEndDate";
const KEY_BACKUP_FOLDER: &str = "backupFolder";
/// The key the Markdown file uses for the backup folder (Swift's bookmark name).
const MD_KEY_BACKUP_BOOKMARK: &str = "backupFolderBookmark";

/// Group name for transactions whose account is missing (`Uncategorized` in Swift).
const UNCATEGORIZED: &str = "Uncategorized";

/// R-MD-1: write `db` (live rows only) in the exact Swift backup format.
/// Calendar dates are written as local midnight in `tz`, converted to UTC.
pub fn export_markdown(db: &Db, tz: &TimeZone) -> String {
    let mut out = String::from(TITLE);
    write_contexts(db, &mut out);
    write_categories(db, &mut out);
    write_scenarios(db, &mut out);
    write_accounts(db, &mut out);
    write_obligations(db, tz, &mut out);
    write_budget_items(db, tz, &mut out);
    write_import_mappings(db, &mut out);
    write_transactions(db, tz, &mut out);
    write_settings(db, tz, &mut out);
    out
}

/// Appends `heading`, the header row, the separator row, the data rows and one blank line.
fn push_table(out: &mut String, heading: &str, header: &[&str], rows: &[Vec<String>]) {
    out.push_str(heading);
    out.push('\n');
    push_row(out, header);
    let separator = vec!["---"; header.len()];
    push_row(out, &separator);
    for row in rows {
        push_row(out, row);
    }
    out.push('\n');
}

/// One table row: `| a | b |`. Empty cells keep both spaces (`|  |`).
fn push_row<S: AsRef<str>>(out: &mut String, cells: &[S]) {
    out.push_str("| ");
    let joined: Vec<&str> = cells.iter().map(AsRef::as_ref).collect();
    out.push_str(&joined.join(" | "));
    out.push_str(" |\n");
}

/// `Some(text)` as an escaped cell, `None` as an empty cell.
fn opt_cell(value: Option<&str>) -> String {
    esc(value.unwrap_or(""))
}

fn write_contexts(db: &Db, out: &mut String) {
    let contexts = db.contexts_by_name();
    // Without an archived context the table stays exactly the Swift format.
    let any_archived = contexts.iter().any(|c| c.is_archived);
    let rows: Vec<Vec<String>> = contexts
        .into_iter()
        .map(|c| {
            let mut row = vec![
                esc(&c.name),
                opt_cell(c.color_hex.as_deref()),
                opt_cell(c.notes.as_deref()),
            ];
            if any_archived {
                row.push(c.is_archived.to_string());
            }
            row
        })
        .collect();
    let header = if any_archived {
        CONTEXT_HEADER_ARCHIVED
    } else {
        CONTEXT_HEADER
    };
    push_table(out, "## Contexts", header, &rows);
}

/// Top-level categories first, then children; each group by name (R-MD-1 sort order).
fn write_categories(db: &Db, out: &mut String) {
    let mut cats: Vec<&Category> = db.live_categories().collect();
    cats.sort_by(|a, b| {
        a.parent_id
            .is_some()
            .cmp(&b.parent_id.is_some())
            .then_with(|| a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
    let rows: Vec<Vec<String>> = cats
        .into_iter()
        .map(|c| {
            // The parent is looked up including soft-deleted rows, as Swift's relationship does.
            let parent = c.parent_id.and_then(|p| db.category(p));
            vec![
                esc(&c.name),
                c.category_type.as_str().to_string(),
                opt_cell(c.icon.as_deref()),
                opt_cell(c.notes.as_deref()),
                opt_cell(parent.map(|p| p.name.as_str())),
                esc(&c.match_patterns.join(";")),
            ]
        })
        .collect();
    push_table(out, "## Categories", CATEGORY_HEADER, &rows);
}

fn write_scenarios(db: &Db, out: &mut String) {
    let rows: Vec<Vec<String>> = db
        .scenarios_by_name()
        .into_iter()
        .map(|s| {
            vec![
                esc(&s.name),
                opt_cell(s.color_hex.as_deref()),
                opt_cell(s.notes.as_deref()),
            ]
        })
        .collect();
    push_table(out, "## Scenarios", SCENARIO_HEADER, &rows);
}

fn write_accounts(db: &Db, out: &mut String) {
    let rows: Vec<Vec<String>> = db
        .accounts_by_name()
        .into_iter()
        .map(|a| {
            vec![
                esc(&a.name),
                a.account_type.as_str().to_string(),
                esc(&a.institution),
                esc(&a.currency),
                a.is_external.to_string(),
            ]
        })
        .collect();
    push_table(out, "## Accounts", ACCOUNT_HEADER, &rows);
}

fn write_obligations(db: &Db, tz: &TimeZone, out: &mut String) {
    let rows: Vec<Vec<String>> = db
        .obligations_by_name()
        .into_iter()
        .map(|o| {
            let account = o.account_id.and_then(|a| db.account(a));
            let context = o.context_id.and_then(|c| db.context(c));
            vec![
                esc(&o.name),
                o.status.as_str().to_string(),
                o.target_amount.map(fmt_decimal).unwrap_or_default(),
                fmt_decimal(o.current_balance),
                o.due_date.map(|d| fmt_date(d, tz)).unwrap_or_default(),
                o.created_date.map(fmt_timestamp).unwrap_or_default(),
                o.last_updated_date.map(fmt_timestamp).unwrap_or_default(),
                opt_cell(o.notes.as_deref()),
                opt_cell(account.map(|a| a.name.as_str())),
                opt_cell(context.map(|c| c.name.as_str())),
            ]
        })
        .collect();
    push_table(out, "## Obligations", OBLIGATION_HEADER, &rows);
}

fn write_budget_items(db: &Db, tz: &TimeZone, out: &mut String) {
    let rows: Vec<Vec<String>> = db
        .budget_items_by_name()
        .into_iter()
        .map(|b| {
            let months: Vec<String> = b.specific_months.iter().map(u32::to_string).collect();
            let category = b.category_id.and_then(|c| db.category(c));
            let account = b.account_id.and_then(|a| db.account(a));
            vec![
                esc(&b.name),
                b.item_type.as_str().to_string(),
                fmt_decimal(b.amount),
                fmt_date(b.start_date, tz),
                b.end_date.map(|d| fmt_date(d, tz)).unwrap_or_default(),
                b.frequency.as_str().to_string(),
                months.join(","),
                opt_cell(b.notes.as_deref()),
                opt_cell(category.map(|c| c.name.as_str())),
                opt_cell(account.map(|a| a.name.as_str())),
            ]
        })
        .collect();
    push_table(out, "## BudgetItems", BUDGET_HEADER, &rows);
}

/// One mapping row: `| type | source | target | negate |`.
fn mapping_row(m: &ImportMapping) -> Vec<String> {
    vec![
        esc(&m.mapping_type),
        esc(&m.source_value),
        esc(&m.target_value),
        m.negate_amounts.to_string(),
    ]
}

/// Global mappings first, then one group per account name, sorted (R-MD-1 sort order).
fn write_import_mappings(db: &Db, out: &mut String) {
    out.push_str("## ImportMappings\n");
    let global: Vec<Vec<String>> = db
        .import_mappings
        .values()
        .filter(|m| m.account_id.is_none())
        .map(mapping_row)
        .collect();
    if !global.is_empty() {
        push_table(out, "### Monarch (Global)", MAPPING_HEADER, &global);
    }
    // Account names are looked up including soft-deleted accounts. Mappings whose account row
    // is missing entirely cannot be placed under a heading, so they are not written.
    let mut groups: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    for m in db.import_mappings.values() {
        let Some(account) = m.account_id.and_then(|a| db.account(a)) else {
            continue;
        };
        groups
            .entry(account.name.clone())
            .or_default()
            .push(mapping_row(m));
    }
    for (name, rows) in &groups {
        push_table(out, &format!("### {name}"), MAPPING_HEADER, rows);
    }
}

/// Transactions grouped by account name (`Uncategorized` for orphans), sorted by name;
/// within a group by date descending, then id descending (R-MD-1 sort order).
fn write_transactions(db: &Db, tz: &TimeZone, out: &mut String) {
    out.push_str("## Transactions\n");
    let mut groups: BTreeMap<String, Vec<&Transaction>> = BTreeMap::new();
    for t in db.live_transactions() {
        let name = t
            .account_id
            .and_then(|a| db.account(a))
            .map_or(UNCATEGORIZED, |a| a.name.as_str());
        groups.entry(name.to_string()).or_default().push(t);
    }
    for (name, mut txs) in groups {
        txs.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let rows: Vec<Vec<String>> = txs
            .into_iter()
            .map(|t| transaction_row(db, t, tz))
            .collect();
        push_table(out, &format!("### {name}"), TRANSACTION_HEADER, &rows);
    }
}

fn transaction_row(db: &Db, t: &Transaction, tz: &TimeZone) -> Vec<String> {
    let category = t.category_id.and_then(|c| db.category(c));
    let scenario = t.scenario_id.and_then(|s| db.scenario(s));
    let contexts: Vec<&str> = t
        .context_ids
        .iter()
        .filter_map(|c| db.context(*c))
        .map(|c| c.name.as_str())
        .collect();
    vec![
        fmt_date(t.date, tz),
        esc(&t.payee),
        opt_cell(category.map(|c| c.name.as_str())),
        fmt_decimal(t.amount),
        opt_cell(t.memo.as_deref()),
        opt_cell(scenario.map(|s| s.name.as_str())),
        esc(&contexts.join(",")),
        t.status.as_str().to_string(),
        opt_cell(t.attachment_path.as_deref()),
    ]
}

/// Written only when the last-projection scenario or the backup folder is set (R-MD-1).
fn write_settings(db: &Db, tz: &TimeZone, out: &mut String) {
    let scenario = db.setting(KEY_PROJ_SCENARIO).unwrap_or("");
    let backup = db.setting(KEY_BACKUP_FOLDER).unwrap_or("");
    if scenario.is_empty() && backup.is_empty() {
        return;
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    if !scenario.is_empty() {
        rows.push(vec![KEY_PROJ_SCENARIO.to_string(), esc(scenario)]);
    }
    for key in [KEY_PROJ_START, KEY_PROJ_END] {
        // Stored as YYYY-MM-DD; written as a calendar date (R-MD-1).
        let stored = db.setting(key).unwrap_or("");
        if let Some(date) = parse_date(stored, tz) {
            rows.push(vec![key.to_string(), fmt_date(date, tz)]);
        }
    }
    if !backup.is_empty() {
        rows.push(vec![MD_KEY_BACKUP_BOOKMARK.to_string(), esc(backup)]);
    }
    push_table(out, "## Settings", SETTINGS_HEADER, &rows);
}
