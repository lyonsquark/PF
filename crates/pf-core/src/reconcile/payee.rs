//! R-REC-11: statement descriptions against PF payees.

use pf_model::{AccountId, Db};

const STOP_WORDS: &[&str] = &[
    "POS",
    "DEBIT",
    "CREDIT",
    "ACH",
    "SQ",
    "TST",
    "PURCHASE",
    "PAYMENT",
    "CARD",
    "CHECKCARD",
    "VISA",
    "MC",
    "ONLINE",
    "RECURRING",
    "WWW",
    "COM",
    "INC",
    "LLC",
    "CO",
    "THE",
];

fn tokens(s: &str) -> Vec<String> {
    let cleaned: String = s
        .to_uppercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|t| !t.chars().any(|c| c.is_ascii_digit()) && !STOP_WORDS.contains(t))
        .map(str::to_string)
        .collect()
}

/// R-REC-11: the comparable form of a description.
pub fn normalize_description(s: &str) -> String {
    tokens(s).join(" ")
}

/// R-REC-11: 1.0 for a known alias, else shared tokens ÷ the smaller token count.
pub fn payee_score(description: &str, payee: &str, aliases: &[(String, String)]) -> f64 {
    let desc = normalize_description(description);
    if !desc.is_empty()
        && aliases
            .iter()
            .any(|(d, p)| *d == desc && p.to_lowercase() == payee.to_lowercase())
    {
        return 1.0;
    }
    let a = tokens(description);
    let b = tokens(payee);
    let smaller = a.len().min(b.len());
    if smaller == 0 {
        return 0.0;
    }
    let shared = a.iter().filter(|t| b.contains(t)).count();
    shared.min(smaller) as f64 / smaller as f64
}

/// R-REC-11: the account's `statementPayee` mappings, then every `payee` mapping (source normalized).
pub fn aliases_for(db: &Db, account: AccountId) -> Vec<(String, String)> {
    let statement = db
        .import_mappings
        .values()
        .filter(|m| m.mapping_type == "statementPayee" && m.account_id == Some(account))
        .map(|m| (m.source_value.clone(), m.target_value.clone()));
    let payee = db
        .import_mappings
        .values()
        .filter(|m| m.mapping_type == "payee")
        .map(|m| {
            (
                normalize_description(&m.source_value),
                m.target_value.clone(),
            )
        });
    statement.chain(payee).collect()
}
