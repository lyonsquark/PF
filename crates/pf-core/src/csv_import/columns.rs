//! R-CSV-2 (auto-detect), R-CSV-3 (manual assignments, joined columns) and R-CSV-15 (saved column
//! layouts and the negate setting).

use pf_model::{AccountId, Db};

/// The `setting` mapping source that stores the per-account negate flag.
pub(crate) const NEGATE_SETTING: &str = "negateAmounts";

/// An app field a CSV column can be assigned to. `Skip` means the column is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportField {
    /// Transaction date.
    Date,
    /// Payee (several columns are joined with " | ").
    Payee,
    /// Signed amount.
    Amount,
    /// Category name.
    Category,
    /// Memo (several columns are joined with " | ").
    Memo,
    /// Monarch account name.
    Account,
    /// Monarch tags, split on commas into contexts.
    Tags,
    /// Monarch original statement; not offered in the picker (Swift quirk).
    OriginalStatement,
    /// Column is not imported.
    Skip,
}

impl ImportField {
    /// Picker order, exactly Swift's `allFields` (no OriginalStatement).
    pub const PICKER: [ImportField; 8] = [
        ImportField::Date,
        ImportField::Payee,
        ImportField::Amount,
        ImportField::Category,
        ImportField::Memo,
        ImportField::Account,
        ImportField::Tags,
        ImportField::Skip,
    ];

    /// The text Swift stores in `ImportMapping.target_value` and shows in the picker.
    pub fn as_str(self) -> &'static str {
        match self {
            ImportField::Date => "Date",
            ImportField::Payee => "Payee",
            ImportField::Amount => "Amount",
            ImportField::Category => "Category",
            ImportField::Memo => "Memo",
            ImportField::Account => "Account",
            ImportField::Tags => "Tags",
            ImportField::OriginalStatement => "Original Statement",
            ImportField::Skip => "(Skip)",
        }
    }

    /// Inverse of [`ImportField::as_str`]. `None` for any other text.
    pub fn parse(s: &str) -> Option<ImportField> {
        match s {
            "Date" => Some(ImportField::Date),
            "Payee" => Some(ImportField::Payee),
            "Amount" => Some(ImportField::Amount),
            "Category" => Some(ImportField::Category),
            "Memo" => Some(ImportField::Memo),
            "Account" => Some(ImportField::Account),
            "Tags" => Some(ImportField::Tags),
            "Original Statement" => Some(ImportField::OriginalStatement),
            "(Skip)" => Some(ImportField::Skip),
            _ => None,
        }
    }
}

/// Which CSV column (by index) feeds each field. Extra payee and memo columns are kept in the
/// `extra_*` lists, in the order they were assigned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnMapping {
    /// Date column.
    pub date: Option<usize>,
    /// First payee column.
    pub payee: Option<usize>,
    /// Amount column.
    pub amount: Option<usize>,
    /// Category column.
    pub category: Option<usize>,
    /// First memo column.
    pub memo: Option<usize>,
    /// Account column (Monarch only; its presence makes the source Monarch).
    pub account: Option<usize>,
    /// Tags column (Monarch only).
    pub tags: Option<usize>,
    /// Original statement column (Monarch only).
    pub original_statement: Option<usize>,
    /// Flip the sign of every amount.
    pub negate_amounts: bool,
    /// Memo columns after the first.
    pub extra_memo: Vec<usize>,
    /// Payee columns after the first.
    pub extra_payee: Vec<usize>,
}

impl ColumnMapping {
    /// R-CSV-3: primary and extra payee columns, sorted.
    pub fn all_payee_columns(&self) -> Vec<usize> {
        let mut cols: Vec<usize> = self
            .payee
            .into_iter()
            .chain(self.extra_payee.iter().copied())
            .collect();
        cols.sort_unstable();
        cols
    }

    /// R-CSV-3: primary and extra memo columns, sorted.
    pub fn all_memo_columns(&self) -> Vec<usize> {
        let mut cols: Vec<usize> = self
            .memo
            .into_iter()
            .chain(self.extra_memo.iter().copied())
            .collect();
        cols.sort_unstable();
        cols
    }

    /// True when an account column is mapped (Monarch source).
    pub fn is_monarch(&self) -> bool {
        self.account.is_some()
    }

    /// R-CSV-3: one entry per header, `(index, header, field)`. Date, Amount, Category, Account
    /// and Tags win over Payee and Memo; Original Statement shows as Skip (Swift quirk).
    pub fn assignments(&self, headers: &[String]) -> Vec<(usize, String, ImportField)> {
        let payee_cols = self.all_payee_columns();
        let memo_cols = self.all_memo_columns();
        headers
            .iter()
            .enumerate()
            .map(|(i, header)| {
                let field = if self.date == Some(i) {
                    ImportField::Date
                } else if self.amount == Some(i) {
                    ImportField::Amount
                } else if self.category == Some(i) {
                    ImportField::Category
                } else if self.account == Some(i) {
                    ImportField::Account
                } else if self.tags == Some(i) {
                    ImportField::Tags
                } else if payee_cols.contains(&i) {
                    ImportField::Payee
                } else if memo_cols.contains(&i) {
                    ImportField::Memo
                } else {
                    ImportField::Skip
                };
                (i, header.clone(), field)
            })
            .collect()
    }

    /// R-CSV-3: clears every column and extra list (not `negate_amounts`), then applies the
    /// assignments. A second Payee or Memo becomes an extra column. Skip does nothing.
    pub fn apply_assignments(&mut self, assignments: &[(usize, ImportField)]) {
        let negate = self.negate_amounts;
        *self = ColumnMapping {
            negate_amounts: negate,
            ..ColumnMapping::default()
        };
        for &(i, field) in assignments {
            match field {
                ImportField::Date => self.date = Some(i),
                ImportField::Payee => match self.payee {
                    None => self.payee = Some(i),
                    Some(_) => self.extra_payee.push(i),
                },
                ImportField::Amount => self.amount = Some(i),
                ImportField::Category => self.category = Some(i),
                ImportField::Memo => match self.memo {
                    None => self.memo = Some(i),
                    Some(_) => self.extra_memo.push(i),
                },
                ImportField::Account => self.account = Some(i),
                ImportField::Tags => self.tags = Some(i),
                ImportField::OriginalStatement => self.original_statement = Some(i),
                ImportField::Skip => {}
            }
        }
    }
}

/// R-CSV-2: auto-detects columns from lower-cased, trimmed headers. The first header that
/// matches a field wins; each field is set at most once.
pub fn detect_columns(headers: &[String]) -> ColumnMapping {
    let mut m = ColumnMapping::default();
    for (i, header) in headers.iter().enumerate() {
        let h = header.to_lowercase();
        let h = h.trim();
        if h == "date" || h.contains("transaction date") {
            m.date = m.date.or(Some(i));
        } else if matches!(h, "merchant" | "payee" | "description" | "name") {
            m.payee = m.payee.or(Some(i));
        } else if h == "amount" {
            m.amount = m.amount.or(Some(i));
        } else if h == "category" {
            m.category = m.category.or(Some(i));
        } else if matches!(h, "memo" | "notes" | "note") {
            m.memo = m.memo.or(Some(i));
        } else if h == "account" || h == "account name" {
            m.account = m.account.or(Some(i));
        } else if matches!(h, "tags" | "tag" | "labels") {
            m.tags = m.tags.or(Some(i));
        } else if h == "original statement" || h.contains("original") {
            m.original_statement = m.original_statement.or(Some(i));
        }
    }
    m
}

/// R-CSV-15: the required fields are Date, Payee and Amount, checked in that order. The error text
/// is the Swift alert message.
pub fn validate_required(assignments: &[(usize, String, ImportField)]) -> Result<(), String> {
    let has = |field: ImportField| assignments.iter().any(|(_, _, f)| *f == field);
    let missing: Vec<&str> = [ImportField::Date, ImportField::Payee, ImportField::Amount]
        .into_iter()
        .filter(|f| !has(*f))
        .map(ImportField::as_str)
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Required columns not mapped: {}. Every import needs at least Date, Payee, and Amount.",
            missing.join(", ")
        ))
    }
}

/// R-CSV-15: the saved `column` rows for this source, as `(index, field)` pairs in mapping id
/// order. Monarch reads the account-less rows; a bank reads the rows for its account. A saved
/// header matches only by exact text. Unknown saved fields are dropped.
pub fn saved_column_assignments(
    db: &Db,
    headers: &[String],
    account: Option<AccountId>,
    is_monarch: bool,
) -> Vec<(usize, ImportField)> {
    let scope = if is_monarch { None } else { account };
    db.import_mappings
        .values()
        .filter(|m| m.mapping_type == "column" && m.account_id == scope)
        .filter_map(|m| {
            let index = headers.iter().position(|h| *h == m.source_value)?;
            let field = ImportField::parse(&m.target_value)?;
            Some((index, field))
        })
        .collect()
}

/// R-CSV-15: the saved negate flag for `account` (its `setting` row), if one exists.
pub fn saved_negate(db: &Db, account: AccountId) -> Option<bool> {
    db.import_mappings
        .values()
        .find(|m| {
            m.mapping_type == "setting"
                && m.source_value == NEGATE_SETTING
                && m.account_id == Some(account)
        })
        .map(|m| m.negate_amounts)
}
