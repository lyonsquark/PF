//! Step 2 of the CSV wizard (T22, Swift `TranslationMappingView`): the accounts (Monarch only), the
//! categories and the payees that the file has and the database does not know. Each has a decision:
//! an account to map to or skip, a category to leave unmapped, map to an existing one, or create, and
//! a payee name. Two sub-dialogs create a new account or a parent category right away.

use egui::{ComboBox, Id, Modal, RichText, Role, TextEdit, Ui, WidgetInfo};
use pf_core::csv_import::CategoryChoice;
use pf_engine::commands::accounts::{AccountInput, create_account};
use pf_engine::commands::categories::{CategoryInput, save_category};
use pf_model::{AccountType, CategoryType, Db};

use super::{Action, CsvWizard};
use crate::app::UiCtx;
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Width of the source column for accounts and categories, in points.
const SOURCE_WIDTH: f32 = 200.0;
/// Width of the source column for payees, in points.
const PAYEE_SOURCE_WIDTH: f32 = 250.0;
/// Text shown for the "don't import" choice of an account.
const SKIP_ACCOUNT: &str = "Skip (don't import)";

/// A sub-dialog open over the wizard.
#[derive(Debug, Clone, PartialEq)]
pub enum SubDialog {
    /// Create an account named after an unrecognized Monarch account.
    NewAccount(NewAccountForm),
    /// Create a top-level category to use as a parent. The text is the name typed so far.
    NewParentCategory(String),
}

/// The form of the New Account sub-dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAccountForm {
    /// The unrecognized value that prompted the dialog; the new account maps to it.
    pub source: String,
    /// The account name.
    pub name: String,
    /// The institution.
    pub institution: String,
    /// Checking, savings, and so on.
    pub account_type: AccountType,
    /// ISO currency code (default USD).
    pub currency: String,
    /// External accounts are left out of totals.
    pub is_external: bool,
    /// An error from the last Create, shown in red.
    pub error: Option<String>,
}

impl NewAccountForm {
    /// A new-account form named after `source`.
    pub fn for_source(source: &str) -> Self {
        Self {
            source: source.to_string(),
            name: source.to_string(),
            institution: String::new(),
            account_type: AccountType::default(),
            currency: "USD".to_string(),
            is_external: false,
            error: None,
        }
    }
}

/// The entry for `key` in a choice list, added with `default` when missing.
fn entry_mut<'a, T>(
    list: &'a mut Vec<(String, T)>,
    key: &str,
    default: impl FnOnce() -> T,
) -> &'a mut T {
    let index = match list.iter().position(|(k, _)| k == key) {
        Some(i) => i,
        None => {
            list.push((key.to_string(), default()));
            list.len() - 1
        }
    };
    &mut list[index].1
}

/// Draws step 2: the sections that have values, then Cancel and Import.
pub(super) fn show(cx: &mut UiCtx, ui: &mut Ui, w: &mut CsvWizard, action: &mut Action) {
    let db = cx.engine.db();
    let nothing = w.unmapped.accounts.is_empty()
        && w.unmapped.categories.is_empty()
        && w.unmapped.payees.is_empty();
    ui.label("The following values from your CSV weren't recognized. Map them to existing items or create new ones.");
    ui.separator();
    if nothing {
        ui.label(
            RichText::new("All values are already mapped! Ready to import.").color(colors::GREEN),
        );
    }
    egui::ScrollArea::vertical()
        .max_height(340.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if w.is_monarch && !w.unmapped.accounts.is_empty() {
                accounts_section(ui, w);
            }
            if !w.unmapped.categories.is_empty() {
                categories_section(ui, db, w);
            }
            if !w.unmapped.payees.is_empty() {
                payees_section(ui, w);
            }
        });
    if let Some(err) = &w.error {
        ui.label(RichText::new(err).color(colors::RED));
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *action = Action::Cancel;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Import").clicked() {
                *action = Action::Import;
            }
        });
    });
}

/// The accounts section (Monarch): each unrecognized account maps to a live account, or is skipped,
/// or gets a new account from the sub-dialog.
fn accounts_section(ui: &mut Ui, w: &mut CsvWizard) {
    ui.label(RichText::new("Accounts").strong());
    let sources = w.unmapped.accounts.clone();
    for source in sources {
        ui.horizontal(|ui| {
            ui.add_sized([SOURCE_WIDTH, 20.0], egui::Label::new(&source).truncate());
            let choice = entry_mut(&mut w.choices.accounts, &source, String::new);
            let shown = if choice.is_empty() {
                SKIP_ACCOUNT.to_string()
            } else {
                choice.clone()
            };
            let resp = ComboBox::from_id_salt(("csvacct", source.clone()))
                .width(220.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(choice, String::new(), SKIP_ACCOUNT);
                });
            resp.response.widget_info(|| {
                WidgetInfo::labeled(Role::ComboBox, true, format!("Account {source}"))
            });
            if ui.button("New…").clicked() {
                w.sub_dialog = Some(SubDialog::NewAccount(NewAccountForm::for_source(&source)));
            }
        });
    }
}

/// The categories section: each unrecognized category is left unmapped, mapped to an existing one, or
/// created under an optional parent.
fn categories_section(ui: &mut Ui, db: &Db, w: &mut CsvWizard) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Categories").strong());
        if ui.button("Create Parent Category…").clicked() {
            w.sub_dialog = Some(SubDialog::NewParentCategory(String::new()));
        }
    });
    let sources = w.unmapped.categories.clone();
    for source in sources {
        let choice = entry_mut(&mut w.choices.categories, &source, || {
            CategoryChoice::CreateNew {
                name: source.clone(),
                parent: None,
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([SOURCE_WIDTH, 20.0], egui::Label::new(&source).truncate());
            if ui
                .radio(matches!(choice, CategoryChoice::Unmapped), "Unmapped")
                .clicked()
            {
                *choice = CategoryChoice::Unmapped;
            }
            if ui
                .radio(
                    matches!(choice, CategoryChoice::MapToExisting(_)),
                    "Existing:",
                )
                .clicked()
                && !matches!(choice, CategoryChoice::MapToExisting(_))
            {
                *choice = CategoryChoice::MapToExisting(String::new());
            }
            if let CategoryChoice::MapToExisting(full) = choice {
                existing_picker(ui, db, &source, full);
            }
            if ui
                .radio(matches!(choice, CategoryChoice::CreateNew { .. }), "New:")
                .clicked()
                && !matches!(choice, CategoryChoice::CreateNew { .. })
            {
                *choice = CategoryChoice::CreateNew {
                    name: source.clone(),
                    parent: None,
                };
            }
            if let CategoryChoice::CreateNew { name, parent } = choice {
                let field = ui.add(TextEdit::singleline(name).desired_width(140.0));
                field.widget_info(|| {
                    WidgetInfo::labeled(
                        Role::TextInput,
                        true,
                        format!("New Category Name {source}"),
                    )
                });
                parent_picker(ui, db, &source, parent);
                if would_duplicate(db, name, parent.as_deref()) {
                    ui.label(RichText::new("⚠").color(colors::ORANGE))
                        .on_hover_text(
                            "This category already exists — it will be mapped to the existing one.",
                        );
                }
            }
        });
    }
}

/// The "Existing" choice: a picker over every live category by full name, case-insensitive.
fn existing_picker(ui: &mut Ui, db: &Db, source: &str, full: &mut String) {
    let mut names: Vec<String> = db
        .live_categories()
        .map(|c| db.category_full_name(c.id))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    let shown = if full.is_empty() {
        "Select…".to_string()
    } else {
        full.clone()
    };
    let resp = ComboBox::from_id_salt(("csvcat_existing", source.to_string()))
        .width(160.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for name in &names {
                ui.selectable_value(full, name.clone(), name);
            }
        });
    resp.response.widget_info(|| {
        WidgetInfo::labeled(Role::ComboBox, true, format!("Existing category {source}"))
    });
}

/// The parent choice of a new category: "None", then the top-level categories by name.
fn parent_picker(ui: &mut Ui, db: &Db, source: &str, parent: &mut Option<String>) {
    let mut tops: Vec<String> = db
        .live_categories()
        .filter(|c| c.parent_id.is_none())
        .map(|c| c.name.clone())
        .collect();
    tops.sort_by_key(|n| n.to_lowercase());
    let shown = parent.clone().unwrap_or_else(|| "None".to_string());
    let resp = ComboBox::from_id_salt(("csvcat_parent", source.to_string()))
        .width(120.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(parent, None, "None");
            for name in &tops {
                ui.selectable_value(parent, Some(name.clone()), name);
            }
        });
    resp.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, format!("Parent of {source}")));
}

/// True when a new category would duplicate a live one: the same name (ignoring case) under the same
/// parent, or both top level.
fn would_duplicate(db: &Db, name: &str, parent: Option<&str>) -> bool {
    let want = name.trim().to_lowercase();
    if want.is_empty() {
        return false;
    }
    db.live_categories().any(|c| {
        let same_name = c.name.trim().to_lowercase() == want;
        let same_parent = match (c.parent_id, parent) {
            (None, None) => true,
            (Some(p), Some(full)) => db.category_full_name(p) == full,
            _ => false,
        };
        same_name && same_parent
    })
}

/// The payees section: each unrecognized payee can be renamed; the text defaults to the source.
fn payees_section(ui: &mut Ui, w: &mut CsvWizard) {
    ui.label(RichText::new("Payees").strong());
    let sources = w.unmapped.payees.clone();
    for source in sources {
        let target = entry_mut(&mut w.choices.payees, &source, || source.clone());
        ui.horizontal(|ui| {
            ui.add_sized(
                [PAYEE_SOURCE_WIDTH, 20.0],
                egui::Label::new(&source).truncate(),
            )
            .on_hover_text(&source);
            let resp = ui.add(TextEdit::singleline(target).desired_width(250.0));
            resp.widget_info(|| {
                WidgetInfo::labeled(Role::TextInput, true, format!("Payee {source}"))
            });
        });
    }
}

/// Draws the open sub-dialog (if any) as a modal over the wizard.
pub(super) fn show_sub_dialog(cx: &mut UiCtx, ui: &mut Ui, w: &mut CsvWizard) {
    let Some(mut sub) = w.sub_dialog.take() else {
        return;
    };
    let close = match &mut sub {
        SubDialog::NewAccount(form) => new_account_modal(cx, ui, form, &mut w.choices.accounts),
        SubDialog::NewParentCategory(name) => parent_category_modal(cx, ui, name),
    };
    if !close {
        w.sub_dialog = Some(sub);
    }
}

/// The New Account sub-dialog. Create makes the account now (its own undo step) and maps the source
/// to it. Returns true when the sub-dialog should close.
fn new_account_modal(
    cx: &mut UiCtx,
    ui: &mut Ui,
    form: &mut NewAccountForm,
    accounts: &mut Vec<(String, String)>,
) -> bool {
    let mut cancel = false;
    let mut create = false;
    let response = Modal::new(Id::unique(("csv_new_account",))).show(ui.ctx(), |ui| {
        ui.set_min_width(360.0);
        ui.heading("New Account");
        labeled_text(ui, "Account Name", &mut form.name);
        labeled_text(ui, "Institution", &mut form.institution);
        ui.horizontal(|ui| {
            ui.label("Type");
            let resp = ComboBox::from_id_salt("csv_new_account_type")
                .selected_text(form.account_type.label())
                .show_ui(ui, |ui| {
                    for t in AccountType::ALL {
                        ui.selectable_value(&mut form.account_type, t, t.label());
                    }
                });
            resp.response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Account Type"));
        });
        labeled_text(ui, "Currency", &mut form.currency);
        ui.checkbox(&mut form.is_external, "External Account");
        if let Some(err) = &form.error {
            ui.label(RichText::new(err).color(colors::RED));
        }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let blocked = form.name.trim().is_empty();
                if ui
                    .add_enabled(!blocked, egui::Button::new("Create"))
                    .clicked()
                {
                    create = true;
                }
            });
        });
    });
    if create {
        let input = AccountInput {
            name: form.name.clone(),
            account_type: form.account_type,
            institution: form.institution.clone(),
            currency: form.currency.clone(),
            is_external: form.is_external,
        };
        match create_account(cx.engine, input) {
            Ok(_) => {
                let name = form.name.trim().to_string();
                *entry_mut(accounts, &form.source, String::new) = name;
                return true;
            }
            Err(e) => form.error = Some(e.to_string()),
        }
    }
    cancel || response.should_close()
}

/// The Create Parent Category sub-dialog. Create is disabled for a blank or existing name.
/// Returns true when the sub-dialog should close.
fn parent_category_modal(cx: &mut UiCtx, ui: &mut Ui, name: &mut String) -> bool {
    let mut cancel = false;
    let mut create = false;
    let existing = cx
        .engine
        .db()
        .live_categories()
        .any(|c| c.name.trim().eq_ignore_ascii_case(name.trim()));
    let response = Modal::new(Id::unique(("csv_parent_category",))).show(ui.ctx(), |ui| {
        ui.set_min_width(360.0);
        ui.heading("Create Parent Category");
        ui.label(
            "Create a top-level category to use as a parent when mapping CSV categories below.",
        );
        labeled_text(ui, "Category Name", name);
        if existing && !name.trim().is_empty() {
            ui.label(
                RichText::new(format!(
                    "A category named \"{}\" already exists.",
                    name.trim()
                ))
                .color(colors::RED),
            );
        }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let blocked = name.trim().is_empty() || existing;
                if ui
                    .add_enabled(!blocked, egui::Button::new("Create"))
                    .clicked()
                {
                    create = true;
                }
            });
        });
    });
    if create {
        let input = CategoryInput {
            name: name.trim().to_string(),
            category_type: CategoryType::Expense,
            icon: String::new(),
            notes: String::new(),
            patterns_csv: String::new(),
            parent: None,
        };
        if let Err(e) = save_category(cx.engine, None, input) {
            cx.toasts.info(e.to_string());
        }
        return true;
    }
    cancel || response.should_close()
}
