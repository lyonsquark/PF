//! R-CSV-6 … R-CSV-14: value parsers and the row-by-row import plan. Nothing is written; the
//! engine applies the plan in one undo step.

use std::collections::HashSet;
use std::str::FromStr;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use pf_model::{AccountId, CategoryId, ContextId, Db, ImportMapping};
use rust_decimal::Decimal;

use super::columns::ColumnMapping;
use super::lookup::lookup_mapping;
use crate::markdown::values::parse_date;

/// Inputs to [`plan_import`].
#[derive(Debug, Clone, Copy)]
pub struct ImportRequest<'a> {
    /// Data rows (headers excluded).
    pub rows: &'a [Vec<String>],
    /// Which column feeds which field.
    pub mapping: &'a ColumnMapping,
    /// The bank account; `None` for Monarch, which resolves an account per row.
    pub target_account: Option<AccountId>,
    /// Import rows whose payee has no mapping (UI bank and API: true; UI Monarch: false).
    pub allow_unmapped_payees: bool,
    /// Time zone for ISO-8601 timestamps in the date column.
    pub tz: &'a TimeZone,
}

/// Where a planned transaction's context comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextRef {
    /// A live context that already exists.
    Existing(ContextId),
    /// Index into [`ImportPlan::new_contexts`].
    New(usize),
}

/// One transaction to insert. Status is always Cleared and scenario always None (R-CSV-14).
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedTransaction {
    /// Transaction date.
    pub date: Date,
    /// Signed amount, after negation.
    pub amount: Decimal,
    /// Payee after translation.
    pub payee: String,
    /// Joined memo columns, if any were non-empty.
    pub memo: Option<String>,
    /// The account the transaction belongs to.
    pub account_id: AccountId,
    /// Category, if the raw value matched one.
    pub category_id: Option<CategoryId>,
    /// Contexts from the tags column.
    pub contexts: Vec<ContextRef>,
}

/// Counts reported to the user. Rows with an unparseable date or amount are not counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportResult {
    /// Transactions that will be inserted.
    pub imported: usize,
    /// Rows matching a live transaction.
    pub skipped_duplicates: usize,
    /// Rows with an unknown account or an unmapped payee.
    pub skipped_unmapped: usize,
}

/// "" for 1 and "s" otherwise.
fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

impl ImportResult {
    /// The Swift summary lines, omitting zero counts.
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.imported > 0 {
            lines.push(format!(
                "\u{2705} {} transaction{} imported",
                self.imported,
                plural(self.imported)
            ));
        }
        if self.skipped_duplicates > 0 {
            lines.push(format!(
                "\u{23ED}\u{FE0F} {} duplicate{} skipped",
                self.skipped_duplicates,
                plural(self.skipped_duplicates)
            ));
        }
        if self.skipped_unmapped > 0 {
            lines.push(format!(
                "\u{26A0}\u{FE0F} {} skipped (unmapped account)",
                self.skipped_unmapped
            ));
        }
        lines
    }
}

/// The result of planning one import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportPlan {
    /// Transactions to insert, in row order.
    pub transactions: Vec<PlannedTransaction>,
    /// Context names to create, in first-seen order.
    pub new_contexts: Vec<String>,
    /// Counts for the summary.
    pub result: ImportResult,
}

/// R-CSV-6: `M/D/YYYY` with a 1–2 digit month and day and a 4-digit year. The date must be valid.
fn parse_us_date(s: &str) -> Option<Date> {
    let mut parts = s.split('/');
    let (month, day, year) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let digits =
        |p: &str, max: usize| (1..=max).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
    if !digits(month, 2)
        || !digits(day, 2)
        || year.len() != 4
        || !year.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let month: i8 = month.parse().ok()?;
    let day: i8 = day.parse().ok()?;
    let year: i16 = year.parse().ok()?;
    Date::new(year, month, day).ok()
}

/// R-CSV-6: a CSV date. Tries `M/D/YYYY`, then `YYYY-MM-DD`, then an ISO-8601 timestamp, which
/// becomes its day in `tz`. Surrounding whitespace is ignored. `None` if nothing parses.
pub fn parse_csv_date(s: &str, tz: &TimeZone) -> Option<Date> {
    let s = s.trim();
    parse_us_date(s).or_else(|| parse_date(s, tz))
}

/// R-CSV-7: removes every `$` and `,`, trims, then parses an exact decimal. Negation is applied
/// by the caller.
pub fn parse_csv_amount(s: &str) -> Option<Decimal> {
    let cleaned: String = s.chars().filter(|c| *c != '$' && *c != ',').collect();
    Decimal::from_str(cleaned.trim()).ok()
}

/// Why a row is not planned.
enum Skip {
    /// Unparseable date or amount, or a missing column: not counted anywhere.
    Silent,
    /// Unknown account or unmapped payee.
    Unmapped,
    /// Matches a live transaction.
    Duplicate,
}

/// Key for the duplicate check (R-CSV-11): account, day, amount and lower-cased payee.
type DupKey = (AccountId, Date, Decimal, String);

/// Everything the row loop needs, computed once per plan.
struct Env<'a> {
    db: &'a Db,
    req: &'a ImportRequest<'a>,
    mappings: Vec<ImportMapping>,
    is_monarch: bool,
    /// Live categories in name order: `(id, lower-cased name, lower-cased full name)`.
    categories: Vec<(CategoryId, String, String)>,
    /// Keys of live transactions, for the duplicate check.
    existing: HashSet<DupKey>,
}

/// The trimmed text of `row[col]`, or `None` when the column is unmapped or past the row end.
fn cell(row: &[String], col: Option<usize>) -> Option<&str> {
    col.and_then(|c| row.get(c)).map(|v| v.trim())
}

/// R-CSV-3: the non-empty trimmed values of `cols`, joined with " | ". `None` when all are empty.
fn joined(row: &[String], cols: &[usize]) -> Option<String> {
    let parts: Vec<&str> = cols
        .iter()
        .filter_map(|c| cell(row, Some(*c)))
        .filter(|v| !v.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

impl Env<'_> {
    /// R-CSV-8: the account for this row. Monarch resolves the raw account name through a global
    /// mapping, then by case-insensitive name. A bank uses its live target account.
    fn resolve_account(&self, row: &[String]) -> Result<AccountId, Skip> {
        if !self.is_monarch {
            let id = self.req.target_account.ok_or(Skip::Unmapped)?;
            return match self.db.account(id) {
                Some(a) if !a.is_deleted => Ok(id),
                _ => Err(Skip::Unmapped),
            };
        }
        let raw = cell(row, self.req.mapping.account).ok_or(Skip::Unmapped)?;
        let name = lookup_mapping(&self.mappings, "account", raw, None).unwrap_or(raw);
        let wanted = name.to_lowercase();
        self.db
            .live_accounts()
            .find(|a| a.name.to_lowercase() == wanted)
            .map(|a| a.id)
            .ok_or(Skip::Unmapped)
    }

    /// R-CSV-12: the category for a raw value, matched against full names then names, ignoring case.
    fn find_category(&self, name: &str) -> Option<CategoryId> {
        let wanted = name.to_lowercase();
        self.categories
            .iter()
            .find(|(_, short, full)| *short == wanted || *full == wanted)
            .map(|(id, _, _)| *id)
    }

    /// R-CSV-13: tags split on commas. Each tag matches a live context by name (ignoring case), or
    /// reuses or appends a new context name in `new_contexts`.
    fn context_refs(&self, row: &[String], new_contexts: &mut Vec<String>) -> Vec<ContextRef> {
        let Some(raw) = cell(row, self.req.mapping.tags) else {
            return Vec::new();
        };
        raw.split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(|tag| {
                if let Some(ctx) = self.db.context_by_name_ci(tag) {
                    return ContextRef::Existing(ctx.id);
                }
                let wanted = tag.to_lowercase();
                let index = match new_contexts.iter().position(|n| n.to_lowercase() == wanted) {
                    Some(i) => i,
                    None => {
                        new_contexts.push(tag.to_string());
                        new_contexts.len() - 1
                    }
                };
                ContextRef::New(index)
            })
            .collect()
    }

    /// Plans one row, checking in Swift's order: date, amount, negate, account, payee mapping,
    /// payee value, duplicate, category, memo, tags.
    fn plan_row(
        &self,
        row: &[String],
        new_contexts: &mut Vec<String>,
    ) -> Result<PlannedTransaction, Skip> {
        let req = self.req;
        let m = req.mapping;

        // R-CSV-6, R-CSV-7: unparseable rows are skipped silently.
        let date = cell(row, m.date)
            .and_then(|s| parse_csv_date(s, req.tz))
            .ok_or(Skip::Silent)?;
        let mut amount = cell(row, m.amount)
            .and_then(parse_csv_amount)
            .ok_or(Skip::Silent)?;
        if m.negate_amounts {
            amount = -amount;
        }

        let account_id = self.resolve_account(row)?;

        // R-CSV-9: the payee source for the mapping check.
        let joined_payee = joined(row, &m.all_payee_columns()).unwrap_or_default();
        if !req.allow_unmapped_payees {
            let source = if self.is_monarch {
                let col = m.original_statement.or(m.payee).unwrap_or(0);
                row.get(col).map(String::as_str).unwrap_or("")
            } else {
                joined_payee.as_str()
            };
            if lookup_mapping(&self.mappings, "payee", source, req.target_account).is_none() {
                return Err(Skip::Unmapped);
            }
        }

        // R-CSV-10: the payee value.
        let payee = match cell(row, m.original_statement) {
            Some(os) if self.is_monarch => lookup_mapping(&self.mappings, "payee", os, None),
            _ => lookup_mapping(&self.mappings, "payee", &joined_payee, req.target_account),
        }
        .map(str::to_string)
        .unwrap_or_else(|| joined_payee.clone());

        // R-CSV-11: duplicates are checked against live transactions only, not this batch.
        let key = (account_id, date, amount.normalize(), payee.to_lowercase());
        if self.existing.contains(&key) {
            return Err(Skip::Duplicate);
        }

        // R-CSV-12: the category. Monarch mappings are global, a bank's are per account.
        let category_id = match cell(row, m.category).filter(|raw| !raw.is_empty()) {
            Some(raw) => {
                let scope = if self.is_monarch {
                    None
                } else {
                    req.target_account
                };
                let mapped = lookup_mapping(&self.mappings, "category", raw, scope).unwrap_or(raw);
                self.find_category(mapped)
            }
            None => None,
        };

        // R-CSV-3: the memo.
        let memo = joined(row, &m.all_memo_columns());

        // R-CSV-13: the contexts.
        let contexts = self.context_refs(row, new_contexts);

        Ok(PlannedTransaction {
            date,
            amount,
            payee,
            memo,
            account_id,
            category_id,
            contexts,
        })
    }
}

/// R-CSV-6 … R-CSV-14: plans every row. Reads `db` only. Mappings come from `db.import_mappings`,
/// so the engine must apply the [`super::MappingPlan`] first. Rows that fail to parse are skipped
/// silently. Unknown accounts and unmapped payees count as `skipped_unmapped`. Duplicates count as
/// `skipped_duplicates`.
pub fn plan_import(db: &Db, req: &ImportRequest<'_>) -> ImportPlan {
    let categories = db
        .categories_by_name()
        .into_iter()
        .map(|c| {
            (
                c.id,
                c.name.to_lowercase(),
                db.category_full_name(c.id).to_lowercase(),
            )
        })
        .collect();
    let existing = db
        .live_transactions()
        .filter_map(|t| {
            Some((
                t.account_id?,
                t.date,
                t.amount.normalize(),
                t.payee.to_lowercase(),
            ))
        })
        .collect();
    let env = Env {
        db,
        req,
        mappings: db.import_mappings.values().cloned().collect(),
        is_monarch: req.mapping.is_monarch(),
        categories,
        existing,
    };

    let mut plan = ImportPlan::default();
    for row in req.rows {
        match env.plan_row(row, &mut plan.new_contexts) {
            Ok(tx) => plan.transactions.push(tx),
            Err(Skip::Silent) => {}
            Err(Skip::Unmapped) => plan.result.skipped_unmapped += 1,
            Err(Skip::Duplicate) => plan.result.skipped_duplicates += 1,
        }
    }
    plan.result.imported = plan.transactions.len();
    plan
}
