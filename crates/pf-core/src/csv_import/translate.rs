//! R-CSV-15 and R-CSV-16: turns the user's translation choices into new categories and mapping
//! rows. Nothing is written; the engine applies the plan.

use pf_model::{AccountId, CategoryId, Db, ImportMappingId};

use super::columns::{ColumnMapping, ImportField, NEGATE_SETTING};
use super::unmapped::UnmappedValues;

/// What the user chose for one unmapped category value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryChoice {
    /// Leave the value unmapped (no mapping row).
    Unmapped,
    /// Map to an existing category, by full name.
    MapToExisting(String),
    /// Create a category with this name, optionally under a top-level parent by name.
    CreateNew {
        /// Name of the new category (without parent).
        name: String,
        /// Top-level parent name, if any.
        parent: Option<String>,
    },
}

/// The user's answers on the translation screen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranslationChoices {
    /// Source account name to existing account name. An empty target means skip.
    pub accounts: Vec<(String, String)>,
    /// Source category to what the user chose.
    pub categories: Vec<(String, CategoryChoice)>,
    /// Source payee to the cleaned payee. An equal or empty target means no mapping.
    pub payees: Vec<(String, String)>,
}

impl TranslationChoices {
    /// Swift `onAppear` defaults: a payee maps to itself, a category is created with its own
    /// name and no parent, and an account is skipped.
    pub fn defaults(unmapped: &UnmappedValues) -> TranslationChoices {
        TranslationChoices {
            accounts: unmapped
                .accounts
                .iter()
                .map(|a| (a.clone(), String::new()))
                .collect(),
            categories: unmapped
                .categories
                .iter()
                .map(|c| {
                    (
                        c.clone(),
                        CategoryChoice::CreateNew {
                            name: c.clone(),
                            parent: None,
                        },
                    )
                })
                .collect(),
            payees: unmapped
                .payees
                .iter()
                .map(|p| (p.clone(), p.clone()))
                .collect(),
        }
    }
}

/// A category to create. Its type is Expense and it has no icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCategory {
    /// Name without the parent prefix.
    pub name: String,
    /// The live top-level parent, if one matched.
    pub parent_id: Option<CategoryId>,
}

/// A mapping row to insert. `kind` is "account", "category", "payee", "column" or "setting".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMapping {
    /// Mapping type.
    pub kind: String,
    /// Source value (raw text).
    pub source: String,
    /// Target value (app text).
    pub target: String,
    /// `None` is global; `Some` is per bank account.
    pub account_id: Option<AccountId>,
    /// Negate flag, used by "setting" rows only.
    pub negate: bool,
}

/// Everything the engine must do to the mappings before planning rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MappingPlan {
    /// Categories to insert first.
    pub new_categories: Vec<NewCategory>,
    /// Mapping rows to insert after the categories.
    pub new_mappings: Vec<NewMapping>,
    /// Old "column" and "setting" rows being replaced.
    pub remove_mapping_ids: Vec<ImportMappingId>,
}

fn new_mapping(
    kind: &str,
    source: &str,
    target: &str,
    account_id: Option<AccountId>,
) -> NewMapping {
    NewMapping {
        kind: kind.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        account_id,
        negate: false,
    }
}

/// R-CSV-16: resolves one `CreateNew` choice to the full category name it maps to. Returns `None`
/// for an empty name. An existing category (full or plain name, ignoring case) is reused. Otherwise
/// a new category is planned, once per (name, parent), under the live top-level parent whose name
/// matches (if none does, the category has no parent).
fn resolve_create_new(
    db: &Db,
    name: &str,
    parent: Option<&str>,
    planned: &mut Vec<NewCategory>,
) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let wanted = name.to_lowercase();
    for cat in db.categories_by_name() {
        let full = db.category_full_name(cat.id);
        if full.to_lowercase() == wanted || cat.name.to_lowercase() == wanted {
            return Some(full);
        }
    }

    let parent_cat = parent
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .and_then(|p| {
            let wanted_parent = p.to_lowercase();
            db.live_categories()
                .find(|c| c.parent_id.is_none() && c.name.to_lowercase() == wanted_parent)
        });
    let parent_id = parent_cat.map(|c| c.id);
    // A repeat of an already-planned category keeps the spelling of its first planned name.
    let stored_name = match planned
        .iter()
        .find(|p| p.parent_id == parent_id && p.name.to_lowercase() == wanted)
    {
        Some(existing) => existing.name.clone(),
        None => {
            planned.push(NewCategory {
                name: name.to_string(),
                parent_id,
            });
            name.to_string()
        }
    };
    Some(match parent_cat {
        Some(p) => format!("{}/{}", p.name, stored_name),
        None => stored_name,
    })
}

/// R-CSV-15, R-CSV-16: plans the mapping rows and the categories they need. Accounts map
/// globally. Categories and payees map to `target_account`. A bank (`target_account` set) gets a
/// "setting" row for its negate flag. Column rows for the source are replaced: Monarch sources
/// use the account-less rows, banks use the rows for their account. `db` is only read.
pub fn plan_mappings(
    db: &Db,
    headers: &[String],
    mapping: &ColumnMapping,
    choices: &TranslationChoices,
    target_account: Option<AccountId>,
    is_monarch: bool,
) -> MappingPlan {
    let mut plan = MappingPlan::default();

    let mut category_targets: Vec<(String, String)> = Vec::new();
    for (source, choice) in &choices.categories {
        match choice {
            CategoryChoice::Unmapped => {}
            CategoryChoice::MapToExisting(full) => {
                if !full.is_empty() {
                    category_targets.push((source.clone(), full.clone()));
                }
            }
            CategoryChoice::CreateNew { name, parent } => {
                if let Some(full) =
                    resolve_create_new(db, name, parent.as_deref(), &mut plan.new_categories)
                {
                    category_targets.push((source.clone(), full));
                }
            }
        }
    }

    for (source, target) in &choices.accounts {
        if !target.is_empty() {
            plan.new_mappings
                .push(new_mapping("account", source, target, None));
        }
    }
    for (source, target) in &category_targets {
        plan.new_mappings
            .push(new_mapping("category", source, target, target_account));
    }
    for (source, target) in &choices.payees {
        if !target.is_empty() && target != source {
            plan.new_mappings
                .push(new_mapping("payee", source, target, target_account));
        }
    }

    let column_scope = if is_monarch { None } else { target_account };
    for m in db.import_mappings.values() {
        let old_setting = target_account.is_some()
            && m.mapping_type == "setting"
            && m.source_value == NEGATE_SETTING
            && m.account_id == target_account;
        let old_column = m.mapping_type == "column" && m.account_id == column_scope;
        if old_setting || old_column {
            plan.remove_mapping_ids.push(m.id);
        }
    }

    if target_account.is_some() {
        plan.new_mappings.push(NewMapping {
            kind: "setting".to_string(),
            source: NEGATE_SETTING.to_string(),
            target: mapping.negate_amounts.to_string(),
            account_id: target_account,
            negate: mapping.negate_amounts,
        });
    }
    for (_, header, field) in mapping.assignments(headers) {
        if field != ImportField::Skip {
            plan.new_mappings
                .push(column_mapping(&header, field, target_account));
        }
    }
    plan
}

/// R-CSV-15: one "column" row, source = header text, target = field text.
fn column_mapping(header: &str, field: ImportField, account_id: Option<AccountId>) -> NewMapping {
    new_mapping("column", header, field.as_str(), account_id)
}
