//! Category commands (T10b): save, apply rules, delete, tree and picker order. Rules R-CAT-2..4,
//! R-CAT-7, R-CAT-8 and BEH-CAT-01..03; the Swift source is `Views/CategoriesView.swift`
//! (`saveCategory`, `applyRulesToUncategorized`, the delete context menu).

use std::collections::HashSet;

use pf_core::categorize::{self as core, uncategorized_matches, validate_patterns};
use pf_model::{Category, CategoryId, CategoryType, Db, RowKey, TransactionId};

use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// Form values for Add and Edit Category.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryInput {
    /// Name; trimmed, must not be empty.
    pub name: String,
    /// Income, expense or transfer.
    pub category_type: CategoryType,
    /// Emoji or SF Symbol name; blank is stored as `None`.
    pub icon: String,
    /// Notes; blank is stored as `None`.
    pub notes: String,
    /// The comma-separated match-pattern text field (R-CAT-2).
    pub patterns_csv: String,
    /// Parent category, if any.
    pub parent: Option<CategoryId>,
}

/// Result of `save_category`: the stored id and the patterns that were dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveCategoryResult {
    /// Id of the saved (new or edited) category.
    pub id: CategoryId,
    /// Regex-looking patterns that did not compile; shown to the user as "• p".
    pub rejected_patterns: Vec<String>,
}

/// Trimmed text, or `None` when blank.
fn trimmed_or_none(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// The live category with this id, or `NotFound`.
fn live_category(db: &Db, id: CategoryId) -> Result<&Category> {
    db.category(id)
        .filter(|c| !c.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("category {}", id.raw())))
}

/// True if `target` is `start` or one of its ancestors (walks `parent_id`, guarding cycles).
fn is_at_or_above(db: &Db, start: CategoryId, target: CategoryId) -> bool {
    let mut seen = HashSet::new();
    let mut cur = Some(start);
    while let Some(c) = cur {
        if c == target {
            return true;
        }
        if !seen.insert(c) {
            return false;
        }
        cur = db.category(c).and_then(|cat| cat.parent_id);
    }
    false
}

/// "Add Category" / "Edit Category". R-CAT-2: patterns are split, trimmed and validated; invalid
/// ones are dropped and returned. A category cannot be its own parent, nor be moved under one of
/// its own descendants (fix: Swift did not check). `NotFound` for an unknown id or parent.
pub fn save_category(
    e: &mut Engine,
    id: Option<CategoryId>,
    input: CategoryInput,
) -> Result<SaveCategoryResult> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Validation("Category name is required".into()));
    }
    if let Some(cid) = id {
        live_category(e.db(), cid)?;
    }
    if let Some(parent) = input.parent {
        live_category(e.db(), parent)?;
        if Some(parent) == id {
            return Err(EngineError::Validation(
                "A category cannot be its own parent".into(),
            ));
        }
        if let Some(cid) = id
            && is_at_or_above(e.db(), parent, cid)
        {
            return Err(EngineError::Validation(
                "A category cannot be moved under its own subcategory".into(),
            ));
        }
    }
    let patterns = validate_patterns(&input.patterns_csv);
    let icon = trimmed_or_none(&input.icon);
    let notes = trimmed_or_none(&input.notes);
    let (ty, parent, valid) = (input.category_type, input.parent, patterns.valid);
    let step = if id.is_some() {
        "Edit Category"
    } else {
        "Add Category"
    };
    let saved = e.apply(step, |w| match id {
        Some(cid) => {
            w.update_category(cid, |c| {
                c.name = name;
                c.category_type = ty;
                c.icon = icon;
                c.notes = notes;
                c.match_patterns = valid;
                c.parent_id = parent;
            })?;
            Ok(cid)
        }
        None => {
            let mut c = Category::new(CategoryId(0), &name, ty);
            c.icon = icon;
            c.notes = notes;
            c.match_patterns = valid;
            c.parent_id = parent;
            Ok(w.insert_category(c))
        }
    })?;
    Ok(SaveCategoryResult {
        id: saved,
        rejected_patterns: patterns.invalid,
    })
}

/// "Apply Rules". R-CAT-3: the valid patterns from `patterns_csv` are applied to live transactions
/// with no category and a non-empty payee; matches get `category`. One undo step. Returns the
/// count. No valid patterns gives 0 and no step. `NotFound` if the category is not live.
pub fn apply_rules_to_uncategorized(
    e: &mut Engine,
    category: CategoryId,
    patterns_csv: &str,
) -> Result<usize> {
    live_category(e.db(), category)?;
    let patterns = validate_patterns(patterns_csv).valid;
    if patterns.is_empty() {
        return Ok(0);
    }
    let matches: Vec<TransactionId> = uncategorized_matches(e.db(), &patterns);
    let count = matches.len();
    e.apply("Apply Rules", |w| {
        for t in matches {
            w.update_transaction(t, |tx| tx.category_id = Some(category))?;
        }
        Ok(())
    })?;
    Ok(count)
}

/// The category and all its live descendants, parents first.
fn subtree(db: &Db, root: CategoryId) -> Vec<CategoryId> {
    let mut out = vec![root];
    let mut next = 0;
    while next < out.len() {
        let current = out[next];
        next += 1;
        for child in db.subcategories(current) {
            if !out.contains(&child.id) {
                out.push(child.id);
            }
        }
    }
    out
}

/// "Delete Category". R-CAT-7: the category and every live descendant are soft-deleted, and every
/// live transaction that pointed at one of them loses its category, as one step. Undo restores
/// both. `NotFound` if the category is not live.
pub fn delete_category(e: &mut Engine, id: CategoryId) -> Result<()> {
    live_category(e.db(), id)?;
    let cats = subtree(e.db(), id);
    let affected: Vec<TransactionId> = e
        .db()
        .live_transactions()
        .filter(|t| t.category_id.is_some_and(|c| cats.contains(&c)))
        .map(|t| t.id)
        .collect();
    e.apply("Delete Category", |w| {
        for c in cats {
            w.set_deleted(RowKey::Category(c), true)?;
        }
        for t in affected {
            w.update_transaction(t, |tx| tx.category_id = None)?;
        }
        Ok(())
    })
}

/// R-CAT-8: the Categories list flattened to (id, depth), in section order (Income, Expense,
/// Transfer), parents before their children, siblings by name.
pub fn category_tree(db: &Db) -> Vec<(CategoryId, usize)> {
    core::category_tree(db)
        .into_iter()
        .flat_map(|section| {
            section
                .rows
                .into_iter()
                .map(|(id, depth)| (id, usize::from(depth)))
        })
        .collect()
}

/// R-CAT-4: live categories for pickers as (id, full name), sorted case-insensitively.
pub fn categories_for_picker(db: &Db) -> Vec<(CategoryId, String)> {
    core::categories_for_picker(db)
}
