//! L3 property test for undo and redo across the commands of T10a–c (T10c: BEH-UND-01, R-UND-2).
//! Random user actions run on the `world` fixture; undo-all must restore the start, redo-all must
//! restore the end, and a reload must show the end on disk.
//!
//! Rows are compared with `Db::all_records()` (every row and setting, soft-deleted rows included),
//! not with `Db` equality, because `Db` equality also covers the private id counter.

use pf_engine::commands::accounts::set_starting_balance;
use pf_engine::commands::budget::{ProjectionRequest, ProjectionTarget, generate_projection};
use pf_engine::commands::categories::{CategoryInput, save_category};
use pf_engine::commands::contexts::{
    ContextInput, create_context, delete_context, set_context_archived,
};
use pf_engine::commands::transactions::{
    NewTransaction, TxField, TxKind, add_transaction, change_status, delete_transactions,
    duplicate_transactions, edit_transaction,
};
use pf_engine::testing::{World, d, m};
use std::collections::HashSet;

use pf_model::{
    AccountId, BudgetItemId, CategoryType, ContextId, Db, Record, TransactionId, TransactionStatus,
};
use proptest::prelude::*;
use rust_decimal::Decimal;

/// Picks one element by `pick` (modulo length); `None` when the slice is empty.
fn pick<T: Copy>(items: &[T], pick: u32) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    items.get(pick as usize % items.len()).copied()
}

/// Runs one random action. `kind` chooses the command (0..=10), `pick` chooses the row.
/// Errors are ignored: a failed command makes no step (the engine rolls it back).
fn run_op(w: &mut World, i: usize, kind: u8, choice: u32) {
    let e = &mut w.engine;
    let accounts: Vec<AccountId> = e.db().live_accounts().map(|a| a.id).collect();
    let txs: Vec<TransactionId> = e.db().live_transactions().map(|t| t.id).collect();
    let contexts: Vec<ContextId> = e.db().live_contexts().map(|c| c.id).collect();
    let budget: Vec<BudgetItemId> = e.db().live_budget_items().map(|b| b.id).collect();
    let name = format!("Op {i}");
    match kind % 11 {
        0 => {
            if let Some(acc) = pick(&accounts, choice) {
                let _ = add_transaction(
                    e,
                    NewTransaction {
                        kind: TxKind::Withdrawal,
                        account: acc,
                        date: d("2026-03-10"),
                        amount: m("7.25"),
                        payee: name,
                        memo: String::new(),
                        status: TransactionStatus::Cleared,
                        category: None,
                        contexts: Vec::new(),
                    },
                );
            }
        }
        1 => {
            if let Some(t) = pick(&txs, choice) {
                let _ = edit_transaction(e, t, TxField::Payee(name));
            }
        }
        2 => {
            let statuses = [
                TransactionStatus::Cleared,
                TransactionStatus::Pending,
                TransactionStatus::Reconciled,
            ];
            if let (Some(t), Some(s)) = (pick(&txs, choice), pick(&statuses, choice / 3)) {
                let _ = change_status(e, t, s);
            }
        }
        3 => {
            if let Some(t) = pick(&txs, choice) {
                let _ = delete_transactions(e, &[t]);
            }
        }
        4 => {
            if let Some(t) = pick(&txs, choice) {
                let _ = duplicate_transactions(e, &[t]);
            }
        }
        5 => {
            let _ = create_context(
                e,
                ContextInput {
                    name,
                    color_hex: None,
                    notes: None,
                },
            );
        }
        6 => {
            if let Some(c) = pick(&contexts, choice) {
                let _ = delete_context(e, c);
            }
        }
        7 => {
            let _ = save_category(
                e,
                None,
                CategoryInput {
                    name,
                    category_type: CategoryType::Expense,
                    icon: String::new(),
                    notes: String::new(),
                    patterns_csv: String::new(),
                    parent: None,
                },
            );
        }
        8 => {
            if let Some(acc) = pick(&accounts, choice) {
                let _ = set_starting_balance(e, acc, Decimal::from(i64::from(choice % 1000)));
            }
        }
        9 => {
            if let Some(item) = pick(&budget, choice) {
                let _ = generate_projection(
                    e,
                    ProjectionRequest {
                        target: ProjectionTarget::New(name),
                        start: d("2026-03-15"),
                        end: d("2026-06-30"),
                        items: vec![item],
                    },
                );
            }
        }
        _ => {
            if let Some(c) = pick(&contexts, choice) {
                let archived = e.db().context(c).is_some_and(|ctx| ctx.is_archived);
                let _ = set_context_archived(e, c, !archived);
            }
        }
    }
}

/// The rows a fresh launch reads back from disk: R-PRG-1 hard-deletes soft-deleted rows and drops
/// context links to rows that no longer exist. The random actions here never delete accounts,
/// categories or scenarios, so no other reference can dangle.
fn persisted_view(db: &Db) -> Vec<Record> {
    let mut v = db.clone();
    v.transactions.retain(|_, t| !t.is_deleted);
    v.budget_items.retain(|_, b| !b.is_deleted);
    v.obligations.retain(|_, o| !o.is_deleted);
    v.accounts.retain(|_, a| !a.is_deleted);
    v.categories.retain(|_, c| !c.is_deleted);
    v.contexts.retain(|_, c| !c.is_deleted);
    v.scenarios.retain(|_, s| !s.is_deleted);
    let live: HashSet<ContextId> = v.contexts.keys().copied().collect();
    for t in v.transactions.values_mut() {
        t.context_ids.retain(|c| live.contains(c));
    }
    v.all_records()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    /// BEH-UND-01 — Covers: BEH-UND-01, R-UND-2
    fn beh_und_01_random_edits_undo_all_redo_all(
        ops in proptest::collection::vec((any::<u8>(), any::<u32>()), 20)
    ) {
        // Given: the world, with its start state recorded.
        let mut w = World::with_fixture("world");
        let db0 = w.engine.db().all_records();
        // When: 20 random actions.
        for (i, (kind, choice)) in ops.iter().enumerate() {
            run_op(&mut w, i, *kind, *choice);
        }
        let db1_db = w.engine.db().clone();
        let db1 = db1_db.all_records();
        // Then: undo everything gives the start, redo everything gives the end.
        while w.engine.can_undo() {
            w.engine.undo().expect("undo");
        }
        prop_assert_eq!(w.engine.db().all_records(), db0);
        while w.engine.can_redo() {
            w.engine.redo().expect("redo");
        }
        prop_assert_eq!(w.engine.db().all_records(), db1.clone());
        // And the end state is what is on disk, after the R-PRG-1 purge that a restart runs.
        w.reload();
        prop_assert_eq!(w.engine.db().all_records(), persisted_view(&db1_db));
    }
}
