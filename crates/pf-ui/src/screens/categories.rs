//! Categories screen (T20, Swift `CategoriesView`): the categories by type (Income, Expense, Transfer),
//! as a tree (parents first, children indented). Double-click or right-click Edit opens the editor;
//! right-click Delete deletes (R-CAT-7).

use egui::{Align, Layout, RichText, Sense, Ui};
use pf_engine::commands::categories::{category_tree, delete_category};
use pf_model::{CategoryId, CategoryType, Db};

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::dialogs::category_edit::CategoryForm;

/// Indent per tree level, in points (R-CAT-8).
const INDENT: f32 = 20.0;

/// Per-screen UI state. The tree is cached per engine revision.
#[derive(Default)]
pub struct State {
    rows: Cached<(), Vec<(CategoryType, Vec<CategoryRow>)>>,
}

/// One row of the tree, resolved for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryRow {
    /// The category.
    pub id: CategoryId,
    /// Its name (without the parent prefix).
    pub name: String,
    /// The icon, only when it is an emoji (SF Symbol names draw nothing; see PLAN.md).
    pub emoji: Option<String>,
    /// Notes, if any.
    pub notes: Option<String>,
    /// 0 for a top-level category, 1 for a child, and so on.
    pub depth: usize,
}

/// True when an icon is an emoji: its first character is not ASCII (a simple rule; SF Symbol names
/// are ASCII).
pub fn is_emoji(icon: &str) -> bool {
    icon.chars().next().is_some_and(|c| !c.is_ascii())
}

/// The tree grouped by type, in the order Income, Expense, Transfer (R-CAT-8). Each type is listed
/// only when it has a live top-level category. Rows keep `category_tree` order (parents first).
pub fn build_rows(db: &Db) -> Vec<(CategoryType, Vec<CategoryRow>)> {
    let tree = category_tree(db);
    [
        CategoryType::Income,
        CategoryType::Expense,
        CategoryType::Transfer,
    ]
    .into_iter()
    .filter_map(|t| {
        let rows: Vec<CategoryRow> = tree
            .iter()
            .filter_map(|(id, depth)| {
                let c = db.category(*id).filter(|c| !c.is_deleted)?;
                (c.category_type == t).then(|| CategoryRow {
                    id: *id,
                    name: c.name.clone(),
                    emoji: c.icon.clone().filter(|i| is_emoji(i)),
                    notes: c.notes.clone().filter(|n| !n.trim().is_empty()),
                    depth: *depth,
                })
            })
            .collect();
        rows.iter().any(|r| r.depth == 0).then_some((t, rows))
    })
    .collect()
}

/// What the user asked for this frame, applied after the list is drawn.
enum Action {
    Add,
    Edit(CategoryId),
    Delete(CategoryId),
}

/// Draws the Categories screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let revision = cx.engine.revision();
    let mut action: Option<Action> = None;

    ui.horizontal(|ui| {
        ui.heading("Categories");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("+ Add Category").clicked() {
                action = Some(Action::Add);
            }
        });
    });
    ui.separator();

    let sections = st.rows.get(revision, &(), || build_rows(cx.engine.db()));
    if sections.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No categories yet").weak()));
    } else {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (t, rows) in sections {
                    ui.add_space(8.0);
                    ui.label(RichText::new(t.label()).strong());
                    for row in rows {
                        category_row(ui, row, &mut action);
                    }
                }
            });
    }

    match action {
        None => {}
        Some(Action::Add) => cx.dialogs.open(Dialog::CategoryEdit(Box::default())),
        Some(Action::Edit(id)) => {
            if let Some(form) = CategoryForm::edit(cx.engine.db(), id) {
                cx.dialogs.open(Dialog::CategoryEdit(Box::new(form)));
            }
        }
        Some(Action::Delete(id)) => {
            if let Err(e) = delete_category(cx.engine, id) {
                cx.toasts.info(e.to_string());
            }
        }
    }
}

/// One tree row: indented by depth, a "- " before children, the emoji, the name and notes. A double-click
/// or the context menu's Edit opens the editor (a single click does nothing, as in Swift).
fn category_row(ui: &mut Ui, row: &CategoryRow, action: &mut Option<Action>) {
    let resp = ui
        .horizontal(|ui| {
            ui.add_space(INDENT * row.depth as f32);
            if row.depth > 0 {
                ui.label(RichText::new("-").weak());
            }
            if let Some(emoji) = &row.emoji {
                ui.label(emoji);
            }
            ui.vertical(|ui| {
                ui.label(&row.name);
                if let Some(notes) = &row.notes {
                    ui.label(RichText::new(notes).small().weak());
                }
            });
        })
        .response;
    let click = ui.interact(
        resp.rect,
        ui.scope_id().with(("category_row", row.id.raw())),
        Sense::click(),
    );
    if click.double_clicked() {
        *action = Some(Action::Edit(row.id));
    }
    click.context_menu(|ui| {
        if ui.button("Edit").clicked() {
            *action = Some(Action::Edit(row.id));
            ui.close();
        }
        if ui.button("Delete").clicked() {
            *action = Some(Action::Delete(row.id));
            ui.close();
        }
    });
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;

    #[test]
    /// Covers: R-CAT-8. Sections follow Income, Expense, Transfer, and a type with no top-level
    /// category is left out. A child follows its parent at depth 1.
    fn r_cat_8_sections_by_type_and_tree_depth() {
        let mut t = TestDb::new();
        let dining = t.category("Dining Out", CategoryType::Expense);
        let _coffee = t.child_category(dining, "Coffee");
        t.category("Salary", CategoryType::Income);
        let db = t.build();
        let sections = build_rows(&db);
        let types: Vec<CategoryType> = sections.iter().map(|(t, _)| *t).collect();
        assert_eq!(types, vec![CategoryType::Income, CategoryType::Expense]);
        let expense = &sections[1].1;
        assert_eq!(expense[0].name, "Dining Out");
        assert_eq!(expense[0].depth, 0);
        assert_eq!(expense[1].name, "Coffee");
        assert_eq!(expense[1].depth, 1);
    }

    #[test]
    /// Only emoji icons are shown: a non-ASCII first character.
    fn emoji_icon_rule() {
        assert!(is_emoji("🛒"));
        assert!(!is_emoji("cart.fill"));
        assert!(!is_emoji(""));
    }
}
