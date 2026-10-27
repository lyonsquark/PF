//! Auto-categorization, match-pattern validation, the Rec button rules, the category picker
//! order and the Categories screen tree. Pure functions over [`Db`].

use pf_model::{CategoryId, CategoryType, Db, TransactionId};
use regex::Regex;

#[cfg(test)]
mod tests;

/// Characters that make a pattern "look like a regex" (R-CAT-2).
const REGEX_CHARS: &str = "^$.*+?[]{}()|\\";

/// R-CAT-2: contains any of the characters  ^ $ . * + ? [ ] { } ( ) | \
pub fn looks_like_regex(pattern: &str) -> bool {
    pattern.chars().any(|c| REGEX_CHARS.contains(c))
}

/// Shared matcher (R-CAT-1): true if any pattern matches `payee`. Per pattern, the case-insensitive
/// substring test runs first, then the pattern as a case-sensitive regex. An empty pattern never
/// matches (Swift's `localizedCaseInsensitiveContains("")` is false). A pattern that the Rust
/// `regex` crate rejects (for example one using look-around, which Swift accepts) is treated as
/// plain text only; R-CAT-2 still reports it as invalid when the user saves it.
pub fn matches_any(patterns: &[String], payee: &str) -> bool {
    let payee_lower = payee.to_lowercase();
    patterns.iter().any(|pattern| {
        if pattern.is_empty() {
            return false;
        }
        if payee_lower.contains(&pattern.to_lowercase()) {
            return true;
        }
        Regex::new(pattern).is_ok_and(|re| re.is_match(payee))
    })
}

/// R-CAT-1: first matching live category for `payee`. Categories in name order (Db::categories_by_name),
/// patterns in stored order. Match = payee contains the pattern case-insensitively, OR the pattern
/// compiles as a `regex::Regex` and `is_match(payee)` (case-SENSITIVE, like Swift's Regex).
/// Blank (whitespace-only) payee → None.
pub fn auto_categorize(db: &Db, payee: &str) -> Option<CategoryId> {
    if payee.trim().is_empty() {
        return None;
    }
    db.categories_by_name()
        .into_iter()
        .find(|cat| matches_any(&cat.match_patterns, payee))
        .map(|cat| cat.id)
}

/// R-CAT-6: add-transaction dialog variant. Substring-only (no regex), case-insensitive, same order.
/// Empty text → None.
pub fn auto_categorize_substring(db: &Db, text: &str) -> Option<CategoryId> {
    if text.is_empty() {
        return None;
    }
    let text_lower = text.to_lowercase();
    db.categories_by_name()
        .into_iter()
        .find(|cat| {
            cat.match_patterns
                .iter()
                .any(|p| !p.is_empty() && text_lower.contains(&p.to_lowercase()))
        })
        .map(|cat| cat.id)
}

/// Result of R-CAT-2 validation: the patterns kept and the ones rejected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatternValidation {
    /// Patterns to store, in input order.
    pub valid: Vec<String>,
    /// Regex-looking patterns that failed to compile.
    pub invalid: Vec<String>,
}

impl PatternValidation {
    /// The alert body Swift shows: each invalid pattern as "• {p}", joined by "\n". Empty if none.
    pub fn alert_text(&self) -> String {
        self.invalid
            .iter()
            .map(|p| format!("• {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// R-CAT-2: split `input` on ',', trim, drop empties; a pattern is invalid iff it looks like a regex
/// and `regex::Regex::new` fails. Order preserved. Duplicates kept (Swift keeps them).
pub fn validate_patterns(input: &str) -> PatternValidation {
    let mut out = PatternValidation::default();
    for raw in input.split(',') {
        let p = raw.trim();
        if p.is_empty() {
            continue;
        }
        if looks_like_regex(p) && Regex::new(p).is_err() {
            out.invalid.push(p.to_string());
        } else {
            out.valid.push(p.to_string());
        }
    }
    out
}

/// Patterns as the editor shows them: joined with ", " (Swift `joined(separator: ", ")`).
pub fn patterns_to_input(patterns: &[String]) -> String {
    patterns.join(", ")
}

/// R-CAT-3 (pure part): ids of live transactions with no category and non-empty payee that match
/// any of `patterns` (use only VALID patterns, per R-CAT-2). The write happens in T10b.
pub fn uncategorized_matches(db: &Db, patterns: &[String]) -> Vec<TransactionId> {
    db.live_transactions()
        .filter(|t| t.category_id.is_none() && !t.payee.is_empty())
        .filter(|t| matches_any(patterns, &t.payee))
        .map(|t| t.id)
        .collect()
}

/// R-CAT-3 message: "Applied to {n} uncategorized transaction(s)."
pub fn apply_rules_message(n: usize) -> String {
    format!("Applied to {n} uncategorized transaction(s).")
}

/// R-CAT-5: the "Rec" button is disabled when no category, the trimmed payee is empty, or the
/// category already has a pattern equal to the trimmed payee case-insensitively.
pub fn rec_disabled(db: &Db, category: Option<CategoryId>, payee: &str) -> bool {
    let Some(cat) = category else {
        return true;
    };
    let trimmed = payee.trim();
    if trimmed.is_empty() {
        return true;
    }
    let want = trimmed.to_lowercase();
    match db.category(cat) {
        Some(c) => c.match_patterns.iter().any(|p| p.to_lowercase() == want),
        None => false,
    }
}

/// R-CAT-5 confirmation: `Added "{payee}" to {fullName} auto-match`.
pub fn rec_message(db: &Db, category: CategoryId, payee: &str) -> String {
    format!(
        "Added \"{}\" to {} auto-match",
        payee.trim(),
        db.category_full_name(category)
    )
}

/// R-CAT-4: live categories for pickers, sorted by full name, case-insensitive (to_lowercase), ties by id.
pub fn categories_for_picker(db: &Db) -> Vec<(CategoryId, String)> {
    let mut v: Vec<(CategoryId, String)> = db
        .live_categories()
        .map(|c| (c.id, db.category_full_name(c.id)))
        .collect();
    v.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    v
}

/// One section of the Categories screen (R-CAT-8).
#[derive(Debug, Clone, PartialEq)]
pub struct CategorySection {
    /// "Income", "Expense" or "Transfer".
    pub title: &'static str,
    /// (id, depth) where depth is 0 for a top-level category and 1 for its child.
    pub rows: Vec<(CategoryId, u8)>,
}

/// R-CAT-8: the Categories screen. Sections in CategoryType::ALL order (Income, Expense, Transfer),
/// each with top-level live categories of that type sorted by name, each followed by its live
/// subcategories (depth 1) sorted by name. Empty sections omitted. Title = "Income" / "Expense" / "Transfer".
pub fn category_tree(db: &Db) -> Vec<CategorySection> {
    let mut sections = Vec::new();
    for ty in CategoryType::ALL {
        let mut rows: Vec<(CategoryId, u8)> = Vec::new();
        let parents = db
            .categories_by_name()
            .into_iter()
            .filter(|c| c.category_type == ty && c.parent_id.is_none());
        for parent in parents {
            rows.push((parent.id, 0));
            rows.extend(db.subcategories(parent.id).into_iter().map(|c| (c.id, 1)));
        }
        if !rows.is_empty() {
            sections.push(CategorySection {
                title: ty.label(),
                rows,
            });
        }
    }
    sections
}

/// Emoji scalar ranges copied from Swift `String.isEmoji` (Views/CategoriesView.swift).
const EMOJI_RANGES: &[(u32, u32)] = &[
    (0x1F600, 0x1F64F),
    (0x1F300, 0x1F5FF),
    (0x1F680, 0x1F6FF),
    (0x1F1E6, 0x1F1FF),
    (0x2600, 0x26FF),
    (0x2700, 0x27BF),
    (0xE0020, 0xE007F),
    (0xFE00, 0xFE0F),
    (0x1F900, 0x1F9FF),
    (0x1FA70, 0x1FAFF),
];

/// Swift `String.isEmoji`: true if ANY scalar is in the listed ranges (see below).
pub fn is_emoji(s: &str) -> bool {
    s.chars().any(|c| {
        let v = c as u32;
        EMOJI_RANGES.iter().any(|&(lo, hi)| (lo..=hi).contains(&v))
    })
}
