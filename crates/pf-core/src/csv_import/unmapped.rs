//! R-CSV-5: values in the file that have no existing match and no mapping yet.

use std::collections::BTreeSet;

use pf_model::{AccountId, Db};

use super::columns::ColumnMapping;
use super::lookup::lookup_mapping;

/// Values that the translation step must resolve. Each list is sorted and deduplicated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnmappedValues {
    /// Monarch account names with no account and no mapping.
    pub accounts: Vec<String>,
    /// Category values with no full-name or name match and no mapping.
    pub categories: Vec<String>,
    /// Payee values (Monarch: original statement) with no mapping.
    pub payees: Vec<String>,
}

/// The trimmed text of `row[col]`, or `None` when the column is unmapped or past the row end.
fn cell(row: &[String], col: Option<usize>) -> Option<&str> {
    col.and_then(|c| row.get(c)).map(|v| v.trim())
}

/// R-CSV-5: scans the rows for values that are not existing accounts or categories and have no
/// mapping. Category matching uses full names only (Swift quirk). Accounts apply to Monarch only.
/// Payees are the original statement for Monarch and the primary payee column for banks.
pub fn collect_unmapped(
    db: &Db,
    rows: &[Vec<String>],
    mapping: &ColumnMapping,
    account: Option<AccountId>,
    is_monarch: bool,
) -> UnmappedValues {
    let mappings: Vec<_> = db.import_mappings.values().cloned().collect();
    let account_names: BTreeSet<String> =
        db.live_accounts().map(|a| a.name.to_lowercase()).collect();
    let category_names: BTreeSet<String> = db
        .live_categories()
        .map(|c| db.category_full_name(c.id).to_lowercase())
        .collect();
    let payee_col = if is_monarch {
        mapping.original_statement.or(mapping.payee)
    } else {
        mapping.payee
    };

    let mut accounts = BTreeSet::new();
    let mut categories = BTreeSet::new();
    let mut payees = BTreeSet::new();
    for row in rows {
        if is_monarch
            && let Some(val) = cell(row, mapping.account)
            && !val.is_empty()
            && !account_names.contains(&val.to_lowercase())
            && lookup_mapping(&mappings, "account", val, None).is_none()
        {
            accounts.insert(val.to_string());
        }
        if let Some(val) = cell(row, mapping.category)
            && !val.is_empty()
            && !category_names.contains(&val.to_lowercase())
            && lookup_mapping(&mappings, "category", val, account).is_none()
        {
            categories.insert(val.to_string());
        }
        if let Some(val) = cell(row, payee_col)
            && !val.is_empty()
            && lookup_mapping(&mappings, "payee", val, account).is_none()
        {
            payees.insert(val.to_string());
        }
    }
    UnmappedValues {
        accounts: accounts.into_iter().collect(),
        categories: categories.into_iter().collect(),
        payees: payees.into_iter().collect(),
    }
}
