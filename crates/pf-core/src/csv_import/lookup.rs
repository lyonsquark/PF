//! R-CSV-4: translation-table lookup.

use pf_model::{AccountId, ImportMapping};

/// R-CSV-4: the first mapping of `kind` whose source equals `source` (both trimmed and
/// lower-cased) and whose account equals `account` (`None` is global). Returns its target.
pub fn lookup_mapping<'a>(
    mappings: &'a [ImportMapping],
    kind: &str,
    source: &str,
    account: Option<AccountId>,
) -> Option<&'a str> {
    let wanted = source.trim().to_lowercase();
    mappings
        .iter()
        .find(|m| {
            m.mapping_type == kind
                && m.account_id == account
                && m.source_value.to_lowercase() == wanted
        })
        .map(|m| m.target_value.as_str())
}
