//! Unit tests for `categorize`: auto-match, pattern validation, Rec button, picker and tree.

use super::*;
use pf_model::testing::TestDb;
use pf_model::{AccountType, CategoryType, RowKey};

#[test]
/// Covers: R-CAT-1
fn r_cat_1_substring_case_insensitive() {
    let mut t = TestDb::new();
    let coffee = t.category("Coffee", CategoryType::Expense);
    t.patterns(coffee, &["STARBUCKS"]);
    let db = t.build();
    assert_eq!(auto_categorize(&db, "Starbucks #123"), Some(coffee));
}

#[test]
/// Covers: R-CAT-1
fn r_cat_1_regex_pattern() {
    let mut t = TestDb::new();
    let amzn = t.category("Shopping", CategoryType::Expense);
    t.patterns(amzn, &["^AMZN.*Mktp"]);
    let db = t.build();
    assert_eq!(auto_categorize(&db, "AMZN Mktp US"), Some(amzn));
}

#[test]
/// Covers: R-CAT-1
fn r_cat_1_regex_is_case_sensitive() {
    let mut t = TestDb::new();
    let amzn = t.category("Shopping", CategoryType::Expense);
    t.patterns(amzn, &[r"^amzn\d"]);
    let db = t.build();
    assert_eq!(auto_categorize(&db, "AMZN1"), None);
}

#[test]
/// Covers: R-CAT-1
fn r_cat_1_first_category_by_name_wins() {
    let mut t = TestDb::new();
    let zoo = t.category("Zoo", CategoryType::Expense);
    let auto = t.category("Auto", CategoryType::Expense);
    t.patterns(zoo, &["fuel"]);
    t.patterns(auto, &["fuel"]);
    let db = t.build();
    assert_eq!(auto_categorize(&db, "Shell fuel"), Some(auto));
    assert_ne!(auto_categorize(&db, "Shell fuel"), Some(zoo));
}

#[test]
/// Covers: R-CAT-1
fn r_cat_1_blank_payee_none() {
    let mut t = TestDb::new();
    let c = t.category("Coffee", CategoryType::Expense);
    t.patterns(c, &[" "]);
    let db = t.build();
    assert_eq!(auto_categorize(&db, "   "), None);
}

#[test]
/// Covers: R-CAT-1
fn r_cat_1_deleted_category_ignored() {
    let mut t = TestDb::new();
    let gone = t.category("Gone", CategoryType::Expense);
    let live = t.category("Live", CategoryType::Expense);
    t.patterns(gone, &["coffee"]);
    t.patterns(live, &["coffee"]);
    t.delete(RowKey::Category(gone));
    let db = t.build();
    assert_eq!(auto_categorize(&db, "Coffee shop"), Some(live));
}

#[test]
/// Covers: R-CAT-1
fn r_cat_1_invalid_regex_falls_back_to_substring() {
    let mut t = TestDb::new();
    let c = t.category("Odd", CategoryType::Expense);
    t.patterns(c, &["("]);
    let db = t.build();
    assert_eq!(auto_categorize(&db, "a ( b"), Some(c));
    assert_eq!(auto_categorize(&db, "a b"), None);
}

#[test]
/// Covers: R-CAT-2
fn r_cat_2_looks_like_regex() {
    assert!(!looks_like_regex("abc"));
    assert!(looks_like_regex("a.b"));
    assert!(looks_like_regex("x|y"));
    assert!(looks_like_regex(r"\d"));
}

#[test]
/// Covers: R-CAT-2
fn r_cat_2_invalid_regex_removed() {
    let v = validate_patterns("coffee, [bad, ^ok$");
    assert_eq!(v.valid, vec!["coffee".to_string(), "^ok$".to_string()]);
    assert_eq!(v.invalid, vec!["[bad".to_string()]);
    assert_eq!(v.alert_text(), "• [bad");
}

#[test]
/// Covers: R-CAT-2
fn r_cat_2_trims_and_drops_empty() {
    let v = validate_patterns(" a , ,b ,");
    assert_eq!(v.valid, vec!["a".to_string(), "b".to_string()]);
    assert!(v.invalid.is_empty());
    assert_eq!(v.alert_text(), "");
}

#[test]
/// Covers: R-CAT-2
fn r_cat_2_patterns_to_input_joins_with_comma_space() {
    let p = vec!["coffee".to_string(), "^ok$".to_string()];
    assert_eq!(patterns_to_input(&p), "coffee, ^ok$");
}

#[test]
/// Covers: R-CAT-3
fn r_cat_3_uncategorized_matches_only_uncategorized() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let other = t.category("Other", CategoryType::Expense);
    let hit = t.tx(chk, "2026-01-05", "-3", "Coffee Shop").done();
    let _already = t
        .tx(chk, "2026-01-06", "-3", "Coffee Bar")
        .category(other)
        .done();
    let _blank = t.tx(chk, "2026-01-07", "-3", "").done();
    let _miss = t.tx(chk, "2026-01-08", "-3", "Rent").done();
    let _gone = t
        .tx(chk, "2026-01-09", "-3", "Coffee Gone")
        .deleted()
        .done();
    let db = t.build();
    assert_eq!(
        uncategorized_matches(&db, &["coffee".to_string()]),
        vec![hit]
    );
}

#[test]
/// Covers: R-CAT-3
fn r_cat_3_message() {
    assert_eq!(
        apply_rules_message(2),
        "Applied to 2 uncategorized transaction(s)."
    );
}

#[test]
/// Covers: R-CAT-4
fn r_cat_4_picker_sorted_by_full_name_ci() {
    let mut t = TestDb::new();
    let b = t.category("b", CategoryType::Expense);
    let food = t.category("A", CategoryType::Expense);
    let z = t.child_category(food, "z");
    let a = t.category("a", CategoryType::Expense);
    let db = t.build();
    let names: Vec<String> = categories_for_picker(&db)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    // Swift lists every live category, parents included. "A" and "a" tie case-insensitively,
    // so they fall back to id order (A was created first).
    assert_eq!(names, vec!["A", "a", "A/z", "b"]);
    let ids: Vec<_> = categories_for_picker(&db)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, vec![food, a, z, b]);
}

#[test]
/// Covers: R-CAT-5
fn r_cat_5_rec_disabled_cases() {
    let mut t = TestDb::new();
    let c = t.category("Coffee", CategoryType::Expense);
    t.patterns(c, &["Coffee"]);
    let db = t.build();
    assert!(rec_disabled(&db, None, "Joe's"));
    assert!(rec_disabled(&db, Some(c), "   "));
    assert!(rec_disabled(&db, Some(c), "coffee"));
    assert!(!rec_disabled(&db, Some(c), "Joe's"));
}

#[test]
/// Covers: R-CAT-5
fn r_cat_5_rec_message() {
    let mut t = TestDb::new();
    let food = t.category("Food", CategoryType::Expense);
    let dining = t.child_category(food, "Dining");
    let db = t.build();
    assert_eq!(
        rec_message(&db, dining, "Joe's"),
        r#"Added "Joe's" to Food/Dining auto-match"#
    );
}

#[test]
/// Covers: R-CAT-6
fn r_cat_6_substring_only_no_regex() {
    let mut t = TestDb::new();
    let fuel = t.category("Fuel", CategoryType::Expense);
    t.patterns(fuel, &["^Shell"]);
    let db = t.build();
    assert_eq!(auto_categorize_substring(&db, "Shell Oil"), None);
    assert_eq!(auto_categorize_substring(&db, ""), None);
}

#[test]
/// Covers: R-CAT-6
fn r_cat_6_substring_matches_case_insensitive() {
    let mut t = TestDb::new();
    let fuel = t.category("Fuel", CategoryType::Expense);
    t.patterns(fuel, &["shell"]);
    let db = t.build();
    assert_eq!(auto_categorize_substring(&db, "SHELL Oil"), Some(fuel));
}

#[test]
/// Covers: R-CAT-8
fn r_cat_8_tree_sections_and_depth() {
    let mut t = TestDb::new();
    let food = t.category("Food", CategoryType::Expense);
    let dining = t.child_category(food, "Dining");
    let groceries = t.child_category(food, "Groceries");
    let gone = t.child_category(food, "Gone");
    let salary = t.category("Salary", CategoryType::Income);
    t.delete(RowKey::Category(gone));
    let db = t.build();
    let tree = category_tree(&db);
    let titles: Vec<&str> = tree.iter().map(|s| s.title).collect();
    assert_eq!(titles, vec!["Income", "Expense"]);
    assert_eq!(tree[0].rows, vec![(salary, 0)]);
    assert_eq!(tree[1].rows, vec![(food, 0), (dining, 1), (groceries, 1)]);
}

#[test]
/// Covers: R-CAT-8
fn is_emoji_matches_swift_ranges() {
    assert!(is_emoji("🍔"));
    assert!(!is_emoji("cart"));
    assert!(is_emoji("☕"));
    assert!(!is_emoji("A"));
}
