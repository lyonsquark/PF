//! L3 behaviour and rule tests for categories (T10b: BEH-CAT-01..03, R-CAT-2..4, R-CAT-7, R-CAT-8).
//! Driven through the public engine API on the `world` fixture, as the UI would call it.

use pf_engine::EngineError;
use pf_engine::commands::categories::{
    CategoryInput, apply_rules_to_uncategorized, categories_for_picker, category_tree,
    delete_category, save_category,
};
use pf_engine::testing::World;
use pf_model::testing::TestDb;
use pf_model::{CategoryId, CategoryType, Db, RowKey, TransactionId};

/// Form values for a category with no icon, no notes and no parent.
fn input(name: &str, ty: CategoryType, patterns: &str) -> CategoryInput {
    CategoryInput {
        name: name.into(),
        category_type: ty,
        icon: String::new(),
        notes: String::new(),
        patterns_csv: patterns.into(),
        parent: None,
    }
}

/// Every live transaction id with its category, for before/after comparisons.
fn snapshot(db: &Db) -> Vec<(TransactionId, Option<CategoryId>)> {
    let mut v: Vec<_> = db
        .live_transactions()
        .map(|t| (t.id, t.category_id))
        .collect();
    v.sort();
    v
}

/// The category of the live transaction with this payee (exactly one must exist).
fn category_of(w: &World, payee: &str) -> Option<CategoryId> {
    w.engine
        .db()
        .transaction(w.tx_by_payee(payee))
        .and_then(|t| t.category_id)
}

/// The categories of every live transaction with this payee (the fixture has three Landlord rows).
fn categories_of(w: &World, payee: &str) -> Vec<Option<CategoryId>> {
    w.engine
        .db()
        .live_transactions()
        .filter(|t| t.payee == payee)
        .map(|t| t.category_id)
        .collect()
}

#[test]
/// R-CAT-2 — Covers: R-CAT-2
fn r_cat_2_invalid_regex_dropped_and_reported() {
    // Given
    let mut w = World::with_fixture("world");
    // When
    let saved = save_category(
        &mut w.engine,
        None,
        input("Coffee", CategoryType::Expense, "Coffee, [abc, ^Star.*"),
    )
    .expect("save");
    // Then
    assert_eq!(saved.rejected_patterns, vec!["[abc".to_string()]);
    let row = w.engine.db().category(saved.id).expect("created");
    assert_eq!(row.match_patterns, vec!["Coffee", "^Star.*"]);
}

#[test]
/// R-CAT-3 — Covers: R-CAT-3
fn r_cat_3_apply_rules_counts_only_uncategorized() {
    // Given
    let mut w = World::with_fixture("world");
    let dining = w.category("Dining");
    // When
    let n = apply_rules_to_uncategorized(&mut w.engine, dining, "Cafe").expect("apply");
    // Then: only the uncategorized Cafe Luna matches; Starbucks keeps Dining
    assert_eq!(n, 1);
    assert_eq!(category_of(&w, "Cafe Luna"), Some(dining));
    assert_eq!(category_of(&w, "Starbucks"), Some(dining));
}

#[test]
/// R-CAT-4 — Covers: R-CAT-4
fn r_cat_4_picker_sorted_by_full_name_ci() {
    // Given
    let w = World::with_fixture("world");
    // When
    let names: Vec<String> = categories_for_picker(w.engine.db())
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    // Then
    assert_eq!(
        names,
        vec![
            "Dining",
            "Groceries",
            "Rent",
            "Rent/Utilities",
            "Salary",
            "Transfer"
        ]
    );
}

#[test]
/// R-CAT-7 — Covers: R-CAT-7
fn r_cat_7_delete_parent_cascades_to_children() {
    // Given
    let mut w = World::with_fixture("world");
    let rent = w.category("Rent");
    let utilities = w.category("Utilities");
    let before = snapshot(w.engine.db());
    // When
    delete_category(&mut w.engine, rent).expect("delete");
    // Then: parent and child are soft-deleted; the Rent rows lose their category
    assert!(w.engine.db().category(rent).is_some_and(|c| c.is_deleted));
    assert!(
        w.engine
            .db()
            .category(utilities)
            .is_some_and(|c| c.is_deleted)
    );
    assert!(categories_of(&w, "Landlord").iter().all(Option::is_none));
    // When: undo restores both and the transaction categories
    w.engine.undo().expect("undo");
    assert!(w.engine.db().category(rent).is_some_and(|c| !c.is_deleted));
    assert!(
        w.engine
            .db()
            .category(utilities)
            .is_some_and(|c| !c.is_deleted)
    );
    assert_eq!(snapshot(w.engine.db()), before);
}

#[test]
/// R-CAT-8 — Covers: R-CAT-8
fn r_cat_8_tree_order_and_depth() {
    // Given
    let w = World::with_fixture("world");
    let rent = w.category("Rent");
    let utilities = w.category("Utilities");
    // When
    let tree = category_tree(w.engine.db());
    // Then: Income, then Expense (Rent 0 followed by Utilities 1), then Transfer
    let rent_pos = tree
        .iter()
        .position(|(id, _)| *id == rent)
        .expect("Rent listed");
    assert_eq!(tree[rent_pos], (rent, 0));
    assert_eq!(tree[rent_pos + 1], (utilities, 1));
    let names: Vec<(String, usize)> = tree
        .iter()
        .map(|(id, depth)| (w.engine.db().category_full_name(*id), *depth))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Salary".to_string(), 0),
            ("Dining".to_string(), 0),
            ("Groceries".to_string(), 0),
            ("Rent".to_string(), 0),
            ("Rent/Utilities".to_string(), 1),
            ("Transfer".to_string(), 0),
        ]
    );
}

#[test]
/// R-CAT-2 / R-CAT-7 — Covers: R-CAT-7 (a category cannot be its own parent, nor moved under its child)
fn category_cannot_be_own_parent_or_descendant() {
    // Given
    let mut w = World::with_fixture("world");
    let rent = w.category("Rent");
    let utilities = w.category("Utilities");
    // When: own parent
    let mut own = input("Rent", CategoryType::Expense, "");
    own.parent = Some(rent);
    let err = save_category(&mut w.engine, Some(rent), own).expect_err("own parent");
    // Then
    assert_eq!(
        err,
        EngineError::Validation("A category cannot be its own parent".into())
    );
    // When: moved under its own subcategory
    let mut under_child = input("Rent", CategoryType::Expense, "");
    under_child.parent = Some(utilities);
    let err = save_category(&mut w.engine, Some(rent), under_child).expect_err("descendant");
    // Then
    assert_eq!(
        err,
        EngineError::Validation("A category cannot be moved under its own subcategory".into())
    );
}

#[test]
/// BEH-CAT-01 — Covers: BEH-CAT-01, R-CAT-2
fn beh_cat_01_invalid_pattern_dropped_valid_kept() {
    // Given
    let mut w = World::with_fixture("world");
    let dining = w.category("Dining");
    // When
    let saved = save_category(
        &mut w.engine,
        Some(dining),
        input(
            "Dining",
            CategoryType::Expense,
            "Starbucks, Cafe, (unclosed",
        ),
    )
    .expect("save");
    // Then
    assert_eq!(saved.id, dining);
    assert_eq!(saved.rejected_patterns, vec!["(unclosed".to_string()]);
    let row = w.engine.db().category(dining).expect("row");
    assert_eq!(row.match_patterns, vec!["Starbucks", "Cafe"]);
}

#[test]
/// BEH-CAT-02 — Covers: BEH-CAT-02, R-CAT-3
fn beh_cat_02_apply_rules_one_undo() {
    // Given
    let mut w = World::with_fixture("world");
    let dining = w.category("Dining");
    let luna = w.tx_by_payee("Cafe Luna");
    let before = snapshot(w.engine.db());
    // When
    let n = apply_rules_to_uncategorized(&mut w.engine, dining, "Cafe").expect("apply");
    // Then: one count, and only Cafe Luna changed
    assert_eq!(n, 1);
    let changed: Vec<TransactionId> = snapshot(w.engine.db())
        .into_iter()
        .zip(before.iter())
        .filter(|(after, prior)| after != *prior)
        .map(|(after, _)| after.0)
        .collect();
    assert_eq!(changed, vec![luna]);
    // When: one undo restores it
    assert_eq!(w.engine.undo().expect("undo"), Some("Apply Rules".into()));
    assert_eq!(category_of(&w, "Cafe Luna"), None);
    assert_eq!(snapshot(w.engine.db()), before);
}

#[test]
/// BEH-CAT-03 — Covers: BEH-CAT-03, R-CAT-7, R-CAT-8, R-CAT-4
fn beh_cat_03_delete_parent_hides_children_undo_restores() {
    // Given
    let mut w = World::with_fixture("world");
    let rent = w.category("Rent");
    // When
    delete_category(&mut w.engine, rent).expect("delete");
    // Then: both hidden from the tree and the picker
    let tree_ids: Vec<CategoryId> = category_tree(w.engine.db())
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(!tree_ids.contains(&rent));
    let picker: Vec<String> = categories_for_picker(w.engine.db())
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert!(!picker.iter().any(|n| n.starts_with("Rent")));
    // Then: the Landlord rows are uncategorized now
    assert!(categories_of(&w, "Landlord").iter().all(Option::is_none));
    // When: undo
    w.engine.undo().expect("undo");
    // Then: tree and transaction categories are restored
    let tree_ids: Vec<CategoryId> = category_tree(w.engine.db())
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(tree_ids.contains(&rent));
    assert_eq!(categories_for_picker(w.engine.db()).len(), 6);
    assert!(
        categories_of(&w, "Landlord")
            .iter()
            .all(|c| *c == Some(rent))
    );
}

#[test]
/// Unit: categories_for_picker over TestDb (R-CAT-4): case-insensitive order, full names, live only.
fn picker_is_case_insensitive_and_skips_deleted() {
    // Given
    let mut t = TestDb::new();
    let banana = t.category("banana", CategoryType::Expense);
    let apple = t.category("Apple", CategoryType::Expense);
    let cherry = t.category("cherry", CategoryType::Expense);
    let fruit_child = t.child_category(apple, "Green");
    t.delete(RowKey::Category(cherry));
    let db = t.build();
    // When
    let got = categories_for_picker(&db);
    // Then
    assert_eq!(
        got,
        vec![
            (apple, "Apple".to_string()),
            (fruit_child, "Apple/Green".to_string()),
            (banana, "banana".to_string()),
        ]
    );
}

#[test]
/// Unit: category_tree over TestDb (R-CAT-8): parents before children, siblings by name.
fn tree_puts_children_after_parent() {
    // Given
    let mut t = TestDb::new();
    let zed = t.category("Zed", CategoryType::Expense);
    let amy = t.category("Amy", CategoryType::Expense);
    let amy_kid = t.child_category(amy, "Kid");
    let amy_art = t.child_category(amy, "Art");
    let db = t.build();
    // When
    let tree = category_tree(&db);
    // Then
    assert_eq!(tree, vec![(amy, 0), (amy_art, 1), (amy_kid, 1), (zed, 0)]);
}
