//! R-MD-2: parses a Swift-format backup into a fresh `Db`. Follows the "Import parsing algorithm"
//! in `docs/markdown-format.md` literally, quirks included. Name lookups are exact and only see
//! rows created by this import (R-MD-6).

use std::collections::HashMap;
use std::str::FromStr;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use pf_model::{
    Account, AccountId, AccountType, BudgetFrequency, BudgetItem, BudgetItemId, BudgetItemType,
    Category, CategoryId, CategoryType, Context, ContextId, Db, Id, ImportMapping, ImportMappingId,
    Obligation, ObligationId, ObligationStatus, Record, Scenario, ScenarioId, Transaction,
    TransactionId, TransactionStatus,
};
use rust_decimal::Decimal;

use super::MarkdownImport;
use super::escape::unesc;
use super::values::{parse_date, parse_decimal, parse_timestamp, swift_bool};

/// Setting keys (`docs/data-model.md` → "Settings keys").
const KEY_PROJ_SCENARIO: &str = "lastProjectionScenario";
const KEY_PROJ_START: &str = "lastProjectionStartDate";
const KEY_PROJ_END: &str = "lastProjectionEndDate";
const KEY_BACKUP_FOLDER: &str = "backupFolder";
/// The key the Markdown file uses for the backup folder.
const MD_KEY_BACKUP_BOOKMARK: &str = "backupFolderBookmark";

/// Rows containing one of these are header rows and are skipped (R-MD-2).
const HEADER_MARKERS: [&str; 4] = ["| Name |", "| Date |", "| Type |", "| Key |"];
/// The progress pre-count uses the same markers except `| Key |` (Swift quirk).
const COUNT_HEADER_MARKERS: [&str; 3] = ["| Name |", "| Date |", "| Type |"];

/// The `###` subsection of the current `##` section.
#[derive(Debug, Clone, PartialEq)]
enum Sub {
    /// `### Monarch (Global)`.
    Global,
    /// `### {account name}`.
    Account(String),
}

/// R-MD-2: parse a backup into a fresh Db. `now` fills missing obligation timestamps.
/// `progress` receives "Importing {section}...", "Importing Transactions... {n}/{total}"
/// (every 50 transaction rows) and finally "Finalizing...".
pub fn import_markdown(
    text: &str,
    tz: &TimeZone,
    now: Timestamp,
    progress: &mut dyn FnMut(&str),
) -> MarkdownImport {
    let lines: Vec<&str> = text.lines().collect();
    let totals = count_section_rows(&lines);
    let mut imp = Importer::new(tz, now);
    let mut section: Option<String> = None;
    let mut sub: Option<Sub> = None;
    let mut row_index = 0usize;

    for line in &lines {
        if let Some(rest) = line.strip_prefix("## ") {
            imp.flush_transactions();
            let name = rest.trim().to_string();
            progress(&format!("Importing {name}..."));
            section = Some(name);
            sub = None;
            row_index = 0;
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            if matches!(
                section.as_deref(),
                Some("Transactions" | "ImportMappings" | "Import Mappings")
            ) {
                let name = rest.trim();
                sub = Some(if name == "Monarch (Global)" {
                    Sub::Global
                } else {
                    Sub::Account(name.to_string())
                });
            }
            continue;
        }
        if !line.starts_with('|')
            || line.contains("---")
            || HEADER_MARKERS.iter().any(|m| line.contains(m))
        {
            continue;
        }
        let cols = split_row(line);
        if cols.is_empty() {
            continue;
        }
        row_index += 1;
        let Some(sec) = section.as_deref() else {
            continue;
        };
        if sec == "Transactions" && row_index.is_multiple_of(50) {
            let total = totals.get(sec).copied().unwrap_or(0);
            progress(&format!("Importing Transactions... {row_index}/{total}"));
        }
        let accepted = match sec {
            "Contexts" => imp.import_context(&cols),
            "Categories" => imp.import_category(&cols),
            "Scenarios" => imp.import_scenario(&cols),
            "Accounts" => imp.import_account(&cols),
            "Obligations" => imp.import_obligation(&cols),
            "BudgetItems" | "Budget Items" => imp.import_budget_item(&cols),
            "Transactions" => imp.import_transaction(&cols, sub.as_ref()),
            "ImportMappings" | "Import Mappings" => imp.import_mapping(&cols, sub.as_ref()),
            "Settings" => imp.import_setting(&cols),
            _ => continue,
        };
        if !accepted {
            imp.skipped += 1;
        }
    }
    imp.flush_transactions();
    progress("Finalizing...");
    imp.finish()
}

/// Data rows per `## ` section, counted the way Swift does for progress totals.
fn count_section_rows(lines: &[&str]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let mut section: Option<&str> = None;
    for line in lines {
        if let Some(rest) = line.strip_prefix("## ") {
            section = Some(rest.trim());
        } else if let Some(sec) = section {
            let header = COUNT_HEADER_MARKERS.iter().any(|m| line.contains(m));
            if line.starts_with('|') && !line.contains("---") && !header {
                *counts.entry(sec.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Splits a table row on `|`, drops the first and last piece, trims and unescapes each cell.
/// Empty pieces are kept, so `| a |  | b |` yields three cells.
fn split_row(line: &str) -> Vec<String> {
    let pieces: Vec<&str> = line.split('|').collect();
    if pieces.len() < 2 {
        return Vec::new();
    }
    pieces[1..pieces.len() - 1]
        .iter()
        .map(|p| unesc(p.trim()))
        .collect()
}

/// `None` for an empty cell, otherwise the text.
fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// Exact, case-sensitive lookup in a name cache. An empty name never matches (Swift `cached*`).
fn lookup<T: Copy>(cache: &HashMap<String, T>, name: &str) -> Option<T> {
    if name.is_empty() {
        None
    } else {
        cache.get(name).copied()
    }
}

/// `Specific Months`: ints in 1..=12, junk pieces dropped (R-MD-2).
fn parse_months(cell: &str) -> Vec<u32> {
    cell.split(',')
        .filter_map(|p| p.parse::<u32>().ok())
        .filter(|m| (1..=12).contains(m))
        .collect()
}

/// Builds the result. Every `import_*` method returns `false` for a rejected row; the caller
/// counts it. Each id is allocated only after the row is accepted, so ids have no gaps.
struct Importer<'a> {
    db: Db,
    tz: &'a TimeZone,
    now: Timestamp,
    skipped: usize,
    /// Transactions of the current `## Transactions` section, in file order (see `flush_transactions`).
    pending_txs: Vec<Transaction>,
    contexts: HashMap<String, ContextId>,
    categories: HashMap<String, CategoryId>,
    scenarios: HashMap<String, ScenarioId>,
    accounts: HashMap<String, AccountId>,
}

impl<'a> Importer<'a> {
    fn new(tz: &'a TimeZone, now: Timestamp) -> Self {
        Self {
            db: Db::new(),
            tz,
            now,
            skipped: 0,
            pending_txs: Vec::new(),
            contexts: HashMap::new(),
            categories: HashMap::new(),
            scenarios: HashMap::new(),
            accounts: HashMap::new(),
        }
    }

    /// Gives the pending transactions their ids and stores them. Ids decrease in file order, so
    /// the export rule "same date: id descending" (R-MD-1) reproduces the file order. Assigning
    /// ids in file order would reverse same-date rows on every round trip.
    fn flush_transactions(&mut self) {
        let pending = std::mem::take(&mut self.pending_txs);
        let ids: Vec<Id> = pending.iter().map(|_| self.db.alloc_id()).collect();
        for (mut tx, id) in pending.into_iter().zip(ids.into_iter().rev()) {
            tx.id = TransactionId::from(id);
            self.db.put(Record::Transaction(tx));
        }
    }

    fn finish(self) -> MarkdownImport {
        MarkdownImport {
            db: self.db,
            skipped_rows: self.skipped,
        }
    }

    /// Contexts: Name | Color | Notes | Archived (Archived optional). Skipped if the name already exists (R-MD-6).
    fn import_context(&mut self, c: &[String]) -> bool {
        if c.len() < 3 || lookup(&self.contexts, &c[0]).is_some() {
            return false;
        }
        let name = &c[0];
        let mut ctx = Context::new(ContextId::from(self.db.alloc_id()), name);
        ctx.color_hex = non_empty(&c[1]);
        ctx.notes = non_empty(&c[2]);
        ctx.is_archived = c.get(3).is_some_and(|v| v == "true");
        self.contexts.insert(name.clone(), ctx.id);
        self.db.put(Record::Context(ctx));
        true
    }

    /// Categories: Name | Type | Icon | Notes | Parent | Match Patterns (patterns optional).
    fn import_category(&mut self, c: &[String]) -> bool {
        if c.len() < 5 {
            return false;
        }
        let Ok(kind) = CategoryType::from_str(&c[1]) else {
            return false;
        };
        if lookup(&self.categories, &c[0]).is_some() {
            return false;
        }
        let name = &c[0];
        let mut cat = Category::new(CategoryId::from(self.db.alloc_id()), name, kind);
        cat.icon = non_empty(&c[2]);
        cat.notes = non_empty(&c[3]);
        cat.parent_id = lookup(&self.categories, &c[4]);
        if c.len() >= 6 {
            cat.match_patterns = c[5]
                .split(';')
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect();
        }
        self.categories.insert(name.clone(), cat.id);
        self.db.put(Record::Category(cat));
        true
    }

    /// Scenarios: Name | Color | Notes.
    fn import_scenario(&mut self, c: &[String]) -> bool {
        if c.len() < 3 || lookup(&self.scenarios, &c[0]).is_some() {
            return false;
        }
        let name = &c[0];
        let mut sc = Scenario::new(ScenarioId::from(self.db.alloc_id()), name);
        sc.color_hex = non_empty(&c[1]);
        sc.notes = non_empty(&c[2]);
        self.scenarios.insert(name.clone(), sc.id);
        self.db.put(Record::Scenario(sc));
        true
    }

    /// Accounts: Name | Type | Institution | Currency | External.
    fn import_account(&mut self, c: &[String]) -> bool {
        if c.len() < 5 {
            return false;
        }
        let Ok(kind) = AccountType::from_str(&c[1]) else {
            return false;
        };
        if lookup(&self.accounts, &c[0]).is_some() {
            return false;
        }
        let name = &c[0];
        let mut acc = Account::new(AccountId::from(self.db.alloc_id()), name, kind, &c[2]);
        acc.currency = c[3].clone();
        acc.is_external = swift_bool(&c[4]);
        self.accounts.insert(name.clone(), acc.id);
        self.db.put(Record::Account(acc));
        true
    }

    /// Obligations: 10 columns. Missing created/updated timestamps default to `now`.
    fn import_obligation(&mut self, c: &[String]) -> bool {
        if c.len() < 10 {
            return false;
        }
        let Ok(status) = ObligationStatus::from_str(&c[1]) else {
            return false;
        };
        let mut ob = Obligation::new(ObligationId::from(self.db.alloc_id()), &c[0]);
        ob.status = status;
        ob.target_amount = parse_decimal(&c[2]);
        // An unparsable balance falls back to zero, as Swift's `?? 0` does.
        ob.current_balance = parse_decimal(&c[3]).unwrap_or(Decimal::ZERO);
        ob.due_date = parse_date(&c[4], self.tz);
        ob.created_date = Some(parse_timestamp(&c[5], self.tz).unwrap_or(self.now));
        ob.last_updated_date = Some(parse_timestamp(&c[6], self.tz).unwrap_or(self.now));
        ob.notes = non_empty(&c[7]);
        ob.account_id = lookup(&self.accounts, &c[8]);
        ob.context_id = lookup(&self.contexts, &c[9]);
        self.db.put(Record::Obligation(ob));
        true
    }

    /// Budget items: 9 columns (old layout) or 10 (new layout, End Date at index 4).
    fn import_budget_item(&mut self, c: &[String]) -> bool {
        if c.len() < 9 {
            return false;
        }
        let Some(item_type) = BudgetItemType::parse_lenient(&c[1]) else {
            return false;
        };
        let Some(amount) = parse_decimal(&c[2]) else {
            return false;
        };
        let Some(start) = parse_date(&c[3], self.tz) else {
            return false;
        };
        let (end, f) = if c.len() >= 10 {
            (parse_date(&c[4], self.tz), 5)
        } else {
            (None, 4)
        };
        let Some(frequency) = BudgetFrequency::parse_lenient(&c[f]) else {
            return false;
        };
        let months = parse_months(&c[f + 1]);
        let mut item = BudgetItem::new(
            BudgetItemId::from(self.db.alloc_id()),
            item_type,
            &c[0],
            amount,
            start,
            frequency,
        );
        item.end_date = end;
        item.specific_months = months;
        item.notes = non_empty(&c[f + 2]);
        item.category_id = lookup(&self.categories, &c[f + 3]);
        item.account_id = lookup(&self.accounts, &c[f + 4]);
        self.db.put(Record::BudgetItem(item));
        true
    }

    /// Transactions: Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment.
    /// The account comes from the `###` subsection; `None` when missing or unknown (orphan).
    fn import_transaction(&mut self, c: &[String], sub: Option<&Sub>) -> bool {
        if c.len() < 8 {
            return false;
        }
        let Some(date) = parse_date(&c[0], self.tz) else {
            return false;
        };
        let Some(amount) = parse_decimal(&c[3]) else {
            return false;
        };
        let Ok(status) = TransactionStatus::from_str(&c[7]) else {
            return false;
        };
        let account = match sub {
            Some(Sub::Account(name)) => lookup(&self.accounts, name),
            _ => None,
        };
        // Split without trimming, as Swift does; unknown names are ignored.
        let contexts: Vec<ContextId> = c[6]
            .split(',')
            .filter(|n| !n.is_empty())
            .filter_map(|n| lookup(&self.contexts, n))
            .collect();
        // The placeholder id is replaced in `flush_transactions`.
        let mut tx = Transaction::new(TransactionId::from(0), date, amount, &c[1]);
        tx.category_id = lookup(&self.categories, &c[2]);
        tx.memo = non_empty(&c[4]);
        tx.scenario_id = lookup(&self.scenarios, &c[5]);
        tx.context_ids = contexts;
        tx.status = status;
        tx.account_id = account;
        tx.attachment_path = c.get(8).and_then(|a| non_empty(a));
        self.pending_txs.push(tx);
        true
    }

    /// Import mappings: Type | Source Value | Target Value | Negate Amounts, plus an optional
    /// 5th account column used only when the subsection is not an account (old layout).
    fn import_mapping(&mut self, c: &[String], sub: Option<&Sub>) -> bool {
        if c.len() < 4 {
            return false;
        }
        let account = match sub {
            Some(Sub::Account(name)) => lookup(&self.accounts, name),
            _ if c.len() >= 5 => lookup(&self.accounts, &c[4]),
            _ => None,
        };
        let mut m = ImportMapping::new(
            ImportMappingId::from(self.db.alloc_id()),
            &c[0],
            &c[1],
            &c[2],
            account,
        );
        m.negate_amounts = swift_bool(&c[3]);
        self.db.put(Record::ImportMapping(m));
        true
    }

    /// Settings: only four keys are used. Dates are stored as `YYYY-MM-DD`. The bookmark is
    /// kept only when it is an absolute path (Swift bookmark blobs are ignored).
    fn import_setting(&mut self, c: &[String]) -> bool {
        if c.len() < 2 {
            return false;
        }
        let value = &c[1];
        match c[0].as_str() {
            KEY_PROJ_SCENARIO => self.put_setting(KEY_PROJ_SCENARIO, value.clone()),
            KEY_PROJ_START | KEY_PROJ_END => match parse_date(value, self.tz) {
                Some(date) => self.put_setting(&c[0], date.to_string()),
                None => return false,
            },
            MD_KEY_BACKUP_BOOKMARK if value.starts_with('/') => {
                self.put_setting(KEY_BACKUP_FOLDER, value.clone());
            }
            _ => {}
        }
        true
    }

    fn put_setting(&mut self, key: &str, value: String) {
        self.db.put(Record::Setting {
            key: key.to_string(),
            value,
        });
    }
}
