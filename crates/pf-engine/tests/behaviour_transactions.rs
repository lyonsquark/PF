//! L3 behaviour and rule tests for transactions (T10a: BEH-TXN-01..08, R-TXN-1..15, R-CAT-5).
//! Driven through the public engine API on the `world` fixture, as the UI would call it.

use pf_core::filter::{SearchState, account_view};
use pf_engine::commands::transactions::{
    NewTransaction, TxField, TxKind, add_payee_pattern, add_transaction, can_add_payee_pattern,
    change_status, delete_orphaned, delete_transactions, duplicate_transactions, edit_transaction,
    is_locked, is_safe_attachment, orphaned_transactions,
};
use pf_engine::testing::{TEST_TODAY, World, d, m};
use pf_engine::{EngineError, RowKey};
use pf_model::{AccountId, TimePeriod, Transaction, TransactionId, TransactionStatus};
use rust_decimal::Decimal;

/// A Cleared, uncategorized, context-free form for `kind` on `account`.
fn form(kind: TxKind, account: AccountId, date: &str, amount: &str, payee: &str) -> NewTransaction {
    NewTransaction {
        kind,
        account,
        date: d(date),
        amount: m(amount),
        payee: payee.into(),
        memo: String::new(),
        status: TransactionStatus::Cleared,
        category: None,
        contexts: Vec::new(),
    }
}

/// The plan A row of the fixture: the 2026-04-01 "Landlord" that belongs to a scenario.
fn plan_a_landlord(w: &World) -> TransactionId {
    let plan = w.scenario("Plan A");
    w.engine
        .db()
        .live_transactions()
        .find(|t| t.payee == "Landlord" && t.scenario_id == Some(plan))
        .map(|t| t.id)
        .expect("fixture has the Plan A landlord row")
}

/// A copy of one row as the Db holds it now.
fn row(w: &World, id: TransactionId) -> Transaction {
    w.engine.db().transaction(id).cloned().expect("row exists")
}

#[test]
/// R-TXN-1 — Covers: R-TXN-1
fn r_txn_1_reality_mode_nothing_locked() {
    // Given: no active scenario.
    let w = World::with_fixture("world");
    let reality = w.tx_by_payee("Starbucks");
    let scenario_row = plan_a_landlord(&w);
    // Then
    assert!(!is_locked(w.engine.db(), &w.engine.state, reality));
    assert!(!is_locked(w.engine.db(), &w.engine.state, scenario_row));
}

#[test]
/// R-TXN-1 — Covers: R-TXN-1
fn r_txn_1_scenario_mode_locks_reality_rows() {
    // Given
    let mut w = World::with_fixture("world");
    w.activate_scenario("Plan A");
    let reality = w.tx_by_payee("Starbucks");
    let scenario_row = plan_a_landlord(&w);
    // Then
    assert!(is_locked(w.engine.db(), &w.engine.state, reality));
    assert!(!is_locked(w.engine.db(), &w.engine.state, scenario_row));
}

#[test]
/// R-TXN-1 — Covers: R-TXN-1
fn r_txn_1_deleted_row_is_locked() {
    // Given
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    // When
    w.engine
        .apply("Delete Transaction", |e| {
            e.set_deleted(RowKey::Transaction(starbucks), true)
        })
        .expect("delete");
    // Then
    assert!(is_locked(w.engine.db(), &w.engine.state, starbucks));
}

#[test]
/// R-TXN-2 — Covers: R-TXN-2
fn r_txn_2_future_on_reality_rejected() {
    // Given
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    let before = row(&w, starbucks);
    // When
    let result = change_status(&mut w.engine, starbucks, TransactionStatus::Future);
    // Then
    assert_eq!(
        result,
        Err(EngineError::Validation(
            "Future status is only for scenario transactions".into()
        ))
    );
    assert_eq!(row(&w, starbucks), before);
    assert!(!w.engine.can_undo());
}

#[test]
/// R-TXN-3 — Covers: R-TXN-3
fn r_txn_3_clear_future_dated_scenario_rejected() {
    // Given: the Plan A landlord is dated 2026-04-01, after TEST_TODAY, and is Future.
    let mut w = World::with_fixture("world");
    w.activate_scenario("Plan A");
    let landlord = plan_a_landlord(&w);
    // When / Then: both clearing statuses are refused with the exact message.
    for status in [TransactionStatus::Cleared, TransactionStatus::Reconciled] {
        assert_eq!(
            change_status(&mut w.engine, landlord, status),
            Err(EngineError::Validation(
                "Cannot clear a future-dated scenario transaction".into()
            ))
        );
    }
    assert_eq!(row(&w, landlord).status, TransactionStatus::Future);
    assert!(!w.engine.can_undo());
}

#[test]
/// R-TXN-4 — Covers: R-TXN-4
fn r_txn_4_pending_promotes_scenario_row() {
    // Given
    let mut w = World::with_fixture("world");
    let plan = w.scenario("Plan A");
    w.activate_scenario("Plan A");
    let landlord = plan_a_landlord(&w);
    // When
    change_status(&mut w.engine, landlord, TransactionStatus::Pending).expect("pending");
    // Then: promoted to reality in the same step.
    assert_eq!(row(&w, landlord).scenario_id, None);
    assert_eq!(row(&w, landlord).status, TransactionStatus::Pending);
    // When: one undo.
    w.engine.undo().expect("undo");
    // Then: scenario and status both restored.
    assert_eq!(row(&w, landlord).scenario_id, Some(plan));
    assert_eq!(row(&w, landlord).status, TransactionStatus::Future);
}

#[test]
/// R-TXN-5 — Covers: R-TXN-5
fn r_txn_5_same_status_noop() {
    // Given: one real step so the undo name is known.
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    let cafe = w.tx_by_payee("Cafe Luna");
    edit_transaction(&mut w.engine, cafe, TxField::Memo(Some("coffee".into()))).expect("memo");
    // When: the same status again.
    change_status(&mut w.engine, starbucks, TransactionStatus::Cleared).expect("noop");
    // Then: no new step.
    assert_eq!(w.engine.undo_name(), Some("Change Memo"));
}

#[test]
/// R-TXN-6 — Covers: R-TXN-6
fn r_txn_6_duplicate_copies_fields_dated_today() {
    // Given: "Whole Foods" has a category and a status.
    let mut w = World::with_fixture("world");
    let src = w.tx_by_payee("Whole Foods");
    let original = row(&w, src);
    // When
    let ids = duplicate_transactions(&mut w.engine, &[src]).expect("duplicate");
    // Then: the copy equals the source except for id and date.
    assert_eq!(ids.len(), 1);
    let mut expected = original.clone();
    expected.id = ids[0];
    expected.date = TEST_TODAY;
    assert_eq!(row(&w, ids[0]), expected);
    assert_eq!(w.engine.undo_name(), Some("Duplicate Transaction"));
}

#[test]
/// R-TXN-7 — Covers: R-TXN-7
fn r_txn_7_delete_names() {
    // One row: singular name.
    let mut one = World::with_fixture("world");
    let starbucks = one.tx_by_payee("Starbucks");
    delete_transactions(&mut one.engine, &[starbucks]).expect("delete one");
    assert_eq!(one.engine.undo_name(), Some("Delete Transaction"));
    // Two rows: plural name.
    let mut two = World::with_fixture("world");
    let a = two.tx_by_payee("Starbucks");
    let b = two.tx_by_payee("Whole Foods");
    delete_transactions(&mut two.engine, &[a, b]).expect("delete two");
    assert_eq!(two.engine.undo_name(), Some("Delete Transactions"));
}

#[test]
/// R-TXN-8 — Covers: R-TXN-8
fn r_txn_8_signs_forced() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    // When: a deposit typed as negative, and a withdrawal typed as positive.
    let dep = add_transaction(
        &mut w.engine,
        form(TxKind::Deposit, chk, "2026-03-01", "-50", "Gift"),
    )
    .expect("deposit")[0];
    let wd = add_transaction(
        &mut w.engine,
        form(TxKind::Withdrawal, chk, "2026-03-01", "50", "Fee"),
    )
    .expect("withdrawal")[0];
    // Then
    assert_eq!(row(&w, dep).amount, m("50"));
    assert_eq!(row(&w, wd).amount, m("-50"));
}

#[test]
/// R-TXN-8 — Covers: R-TXN-8
fn r_txn_8_added_tx_gets_active_scenario() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let plan = w.scenario("Plan A");
    w.activate_scenario("Plan A");
    // When: a pending row, so the date rule does not apply.
    let mut f = form(TxKind::Withdrawal, chk, "2026-03-20", "30", "Books");
    f.status = TransactionStatus::Pending;
    let id = add_transaction(&mut w.engine, f).expect("add")[0];
    // Then
    assert_eq!(row(&w, id).scenario_id, Some(plan));
}

#[test]
/// R-TXN-8 — Covers: R-TXN-8
fn r_txn_8_empty_memo_is_none() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    // When: one empty memo, one with text.
    let empty = add_transaction(
        &mut w.engine,
        form(TxKind::Withdrawal, chk, "2026-03-01", "5", "A"),
    )
    .expect("add")[0];
    let mut with_memo = form(TxKind::Withdrawal, chk, "2026-03-01", "5", "B");
    with_memo.memo = "lunch".into();
    let filled = add_transaction(&mut w.engine, with_memo).expect("add")[0];
    // Then
    assert_eq!(row(&w, empty).memo, None);
    assert_eq!(row(&w, filled).memo.as_deref(), Some("lunch"));
}

#[test]
/// R-TXN-9 — Covers: R-TXN-9
fn r_txn_9_cleared_reality_cannot_be_future() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let msg = EngineError::Validation("Cleared transactions cannot be dated in the future".into());
    // When / Then: adding a cleared or reconciled future row is refused.
    for status in [TransactionStatus::Cleared, TransactionStatus::Reconciled] {
        let mut f = form(TxKind::Withdrawal, chk, "2026-03-16", "5", "Later");
        f.status = status;
        assert_eq!(add_transaction(&mut w.engine, f), Err(msg.clone()));
    }
    // And: editing a cleared reality row's date into the future is refused too.
    let starbucks = w.tx_by_payee("Starbucks");
    assert_eq!(
        edit_transaction(&mut w.engine, starbucks, TxField::Date(d("2026-03-16"))),
        Err(msg)
    );
    assert_eq!(row(&w, starbucks).date, d("2026-01-20"));
    // Then: a pending future row is allowed, and so is a future date on a pending row.
    let mut pending = form(TxKind::Withdrawal, chk, "2026-03-16", "5", "Soon");
    pending.status = TransactionStatus::Pending;
    assert!(add_transaction(&mut w.engine, pending).is_ok());
    let whole_foods = w.tx_by_payee("Whole Foods");
    edit_transaction(&mut w.engine, whole_foods, TxField::Date(d("2026-03-16")))
        .expect("pending row may be future-dated");
}

#[test]
/// R-TXN-12 — Covers: R-TXN-12
fn r_txn_12_unchanged_edit_no_step() {
    // Given
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    // When: the payee is set to its current text.
    let auto = edit_transaction(&mut w.engine, starbucks, TxField::Payee("Starbucks".into()));
    // Then
    assert_eq!(auto, Ok(false));
    assert!(!w.engine.can_undo());
}

#[test]
/// R-TXN-12 — Covers: R-TXN-12
fn r_txn_12_undo_names_per_field() {
    // Given
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    let taxes = w.context("Taxes");
    let groceries = w.category("Groceries");
    let cases: Vec<(TxField, &str)> = vec![
        (TxField::Payee("Starbucks Coffee".into()), "Change Payee"),
        (TxField::Memo(Some("latte".into())), "Change Memo"),
        (TxField::Amount(m("-6.25")), "Change Amount"),
        (TxField::Date(d("2026-03-01")), "Change Date"),
        (TxField::Category(Some(groceries)), "Change Category"),
        (TxField::Contexts(vec![taxes]), "Change Contexts"),
        (
            TxField::Attachment(Some("/Users/x/receipt.pdf".into())),
            "Change Attachment",
        ),
    ];
    for (field, name) in cases {
        // When
        edit_transaction(&mut w.engine, starbucks, field).expect("edit");
        // Then
        assert_eq!(w.engine.undo_name(), Some(name));
    }
}

#[test]
/// R-TXN-13 — Covers: R-TXN-13
fn r_txn_13_payee_edit_autocategorizes_uncategorized() {
    // Given: "Cafe Luna" has no category; Dining matches "Cafe".
    let mut w = World::with_fixture("world");
    let cafe = w.tx_by_payee("Cafe Luna");
    // When
    let fired = edit_transaction(&mut w.engine, cafe, TxField::Payee("Cafe Luna 2".into()));
    // Then
    assert_eq!(fired, Ok(true));
    assert_eq!(row(&w, cafe).category_id, Some(w.category("Dining")));
    assert_eq!(w.engine.undo_name(), Some("Auto-categorize"));
}

#[test]
/// R-TXN-13 — Covers: R-TXN-13
fn r_txn_13_no_autocategorize_when_category_set() {
    // Given: "Whole Foods" is already Groceries, and its new payee matches Dining.
    let mut w = World::with_fixture("world");
    let wf = w.tx_by_payee("Whole Foods");
    let groceries = w.category("Groceries");
    // When
    let fired = edit_transaction(&mut w.engine, wf, TxField::Payee("Cafe Whole".into()));
    // Then
    assert_eq!(fired, Ok(false));
    assert_eq!(row(&w, wf).category_id, Some(groceries));
    assert_eq!(w.engine.undo_name(), Some("Change Payee"));
}

#[test]
/// R-TXN-14 — Covers: R-TXN-14
fn r_txn_14_attachment_allow_list() {
    assert!(is_safe_attachment("a.PDF"));
    assert!(is_safe_attachment("/Users/x/pic.heic"));
    assert!(!is_safe_attachment("x.exe"));
    assert!(!is_safe_attachment("noext"));
}

#[test]
/// R-TXN-15 — Covers: R-TXN-15
fn r_txn_15_orphans_listed_and_deleted() {
    // Given: a transaction with no account.
    let mut w = World::with_fixture("world");
    assert!(orphaned_transactions(w.engine.db()).is_empty());
    let orphan = w
        .engine
        .apply("Add Transaction", |e| {
            Ok(e.insert_transaction(Transaction::new(
                TransactionId(0),
                d("2026-02-02"),
                m("-12"),
                "Lost",
            )))
        })
        .expect("insert orphan");
    // Then: listed.
    assert_eq!(orphaned_transactions(w.engine.db()), vec![orphan]);
    // When: delete all orphans.
    let count = delete_orphaned(&mut w.engine).expect("delete orphans");
    // Then: one row, one undoable step.
    assert_eq!(count, 1);
    assert!(orphaned_transactions(w.engine.db()).is_empty());
    assert_eq!(w.engine.undo_name(), Some("Delete Orphaned Transactions"));
    w.engine.undo().expect("undo");
    assert_eq!(orphaned_transactions(w.engine.db()), vec![orphan]);
}

#[test]
/// R-CAT-5 — Covers: R-CAT-5
fn r_cat_5_rec_button_rules() {
    // Given: Rent has no patterns; Groceries has "Whole Foods"; Utilities is a child of Rent.
    let mut w = World::with_fixture("world");
    let rent = w.category("Rent");
    let groceries = w.category("Groceries");
    let utilities = w.category("Utilities");
    // Then: disabled for an existing pattern (any case) and for a blank payee.
    assert!(!can_add_payee_pattern(
        w.engine.db(),
        groceries,
        "WHOLE foods"
    ));
    assert!(!can_add_payee_pattern(w.engine.db(), rent, "   "));
    assert!(can_add_payee_pattern(w.engine.db(), rent, "Landlord"));
    // When: add with surrounding spaces.
    let toast = add_payee_pattern(&mut w.engine, rent, "  Landlord  ").expect("add");
    // Then: the toast text is exact and the pattern is trimmed and stored.
    assert_eq!(toast, r#"Added "Landlord" to Rent auto-match"#);
    assert_eq!(
        w.engine
            .db()
            .category(rent)
            .map(|c| c.match_patterns.clone()),
        Some(vec!["Landlord".to_string()])
    );
    assert_eq!(w.engine.undo_name(), Some("Add Match Pattern"));
    assert!(!can_add_payee_pattern(w.engine.db(), rent, "landlord"));
    // And: a child category uses its full name in the toast.
    let child = add_payee_pattern(&mut w.engine, utilities, "PG&E").expect("add child");
    assert_eq!(child, r#"Added "PG&E" to Rent/Utilities auto-match"#);
}

#[test]
/// BEH-TXN-01 — Covers: BEH-TXN-01, R-TXN-8
fn beh_txn_01_signs_forced() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let before = w.balance(chk);
    // When: deposit "-25" then withdrawal "25".
    add_transaction(
        &mut w.engine,
        form(TxKind::Deposit, chk, "2026-03-01", "-25", "Refund"),
    )
    .expect("deposit");
    assert_eq!(w.balance(chk), before + m("25"));
    add_transaction(
        &mut w.engine,
        form(TxKind::Withdrawal, chk, "2026-03-01", "25", "Snack"),
    )
    .expect("withdrawal");
    // Then
    assert_eq!(w.balance(chk), before);
}

#[test]
/// BEH-TXN-02 — Covers: BEH-TXN-02, R-TXN-8
fn beh_txn_02_transfer_two_rows_one_undo() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let sav = w.account("Savings");
    let (chk_before, sav_before) = (w.balance(chk), w.balance(sav));
    // When: transfer 200 from Checking to Savings.
    let ids = add_transaction(
        &mut w.engine,
        form(TxKind::Transfer { to: sav }, chk, "2026-03-01", "200", ""),
    )
    .expect("transfer");
    // Then: two linked rows with the generated payees and the balances moved both ways.
    assert_eq!(ids.len(), 2);
    let out = row(&w, ids[0]);
    let inn = row(&w, ids[1]);
    assert_eq!(out.payee, "Transfer to Savings");
    assert_eq!(out.amount, m("-200"));
    assert_eq!(out.account_id, Some(chk));
    assert_eq!(inn.payee, "Transfer from Checking");
    assert_eq!(inn.amount, m("200"));
    assert_eq!(inn.account_id, Some(sav));
    assert_eq!(w.balance(chk), chk_before - m("200"));
    assert_eq!(w.balance(sav), sav_before + m("200"));
    // When: one undo.
    w.engine.undo().expect("undo");
    // Then: both rows gone.
    assert!(w.engine.db().transaction(ids[0]).is_none());
    assert!(w.engine.db().transaction(ids[1]).is_none());
    assert_eq!(w.balance(chk), chk_before);
    assert_eq!(w.balance(sav), sav_before);
}

#[test]
/// BEH-TXN-03 — Covers: BEH-TXN-03, R-TXN-13, R-TXN-12
fn beh_txn_03_payee_edit_autocategorize_two_undo_steps() {
    // Given: "Cafe Luna" is uncategorized.
    let mut w = World::with_fixture("world");
    let cafe = w.tx_by_payee("Cafe Luna");
    // When: payee edit fires the auto-categorize step.
    let fired = edit_transaction(
        &mut w.engine,
        cafe,
        TxField::Payee("Cafe Luna Downtown".into()),
    );
    // Then
    assert_eq!(fired, Ok(true));
    assert_eq!(row(&w, cafe).category_id, Some(w.category("Dining")));
    // When: first undo.
    w.engine.undo().expect("undo category");
    // Then: category reverted, payee kept.
    assert_eq!(row(&w, cafe).category_id, None);
    assert_eq!(row(&w, cafe).payee, "Cafe Luna Downtown");
    // When: second undo.
    w.engine.undo().expect("undo payee");
    // Then
    assert_eq!(row(&w, cafe).payee, "Cafe Luna");
}

#[test]
/// BEH-TXN-04 — Covers: BEH-TXN-04, R-TXN-1
fn beh_txn_04_locked_edit_refused() {
    // Given: Plan A is active, so the reality row "Starbucks" is locked.
    let mut w = World::with_fixture("world");
    w.activate_scenario("Plan A");
    let starbucks = w.tx_by_payee("Starbucks");
    let db_before = w.engine.db().clone();
    // When
    let result = edit_transaction(&mut w.engine, starbucks, TxField::Amount(m("-9")));
    // Then: refused, nothing changed, no undo step.
    assert_eq!(result, Err(EngineError::Locked));
    assert_eq!(w.engine.db(), &db_before);
    assert!(!w.engine.can_undo());
}

#[test]
/// BEH-TXN-05 — Covers: BEH-TXN-05, R-TXN-2, R-TXN-3, R-TXN-4
fn beh_txn_05_status_rules() {
    // Given
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    let plan = w.scenario("Plan A");
    // When / Then: future on a reality row is refused.
    assert_eq!(
        change_status(&mut w.engine, starbucks, TransactionStatus::Future),
        Err(EngineError::Validation(
            "Future status is only for scenario transactions".into()
        ))
    );
    // When / Then: clearing the future-dated Plan A row is refused.
    w.activate_scenario("Plan A");
    let landlord = plan_a_landlord(&w);
    assert!(change_status(&mut w.engine, landlord, TransactionStatus::Cleared).is_err());
    // When: pending promotes it to reality.
    change_status(&mut w.engine, landlord, TransactionStatus::Pending).expect("pending");
    assert_eq!(row(&w, landlord).scenario_id, None);
    // When: undo restores scenario and status.
    w.engine.undo().expect("undo");
    assert_eq!(row(&w, landlord).scenario_id, Some(plan));
    assert_eq!(row(&w, landlord).status, TransactionStatus::Future);
}

#[test]
/// BEH-TXN-06 — Covers: BEH-TXN-06, R-TXN-6
fn beh_txn_06_duplicate_three_one_undo() {
    // Given
    let mut w = World::with_fixture("world");
    let sources = [
        w.tx_by_payee("Starbucks"),
        w.tx_by_payee("Whole Foods"),
        w.tx_by_payee("Acme Payroll"),
    ];
    let before = w.live_tx_count();
    // When
    let ids = duplicate_transactions(&mut w.engine, &sources).expect("duplicate");
    // Then: three new rows dated today, in input order.
    assert_eq!(ids.len(), 3);
    assert_eq!(w.live_tx_count(), before + 3);
    for (copy, source) in ids.iter().zip(sources) {
        assert_eq!(row(&w, *copy).date, TEST_TODAY);
        assert_eq!(row(&w, *copy).payee, row(&w, source).payee);
    }
    // When: one undo removes all three.
    w.engine.undo().expect("undo");
    assert_eq!(w.live_tx_count(), before);
}

#[test]
/// BEH-TXN-07 — Covers: BEH-TXN-07, R-TXN-7
fn beh_txn_07_delete_multiple_one_step() {
    // Given
    let mut w = World::with_fixture("world");
    let a = w.tx_by_payee("Starbucks");
    let b = w.tx_by_payee("Whole Foods");
    let before = w.live_tx_count();
    // When
    delete_transactions(&mut w.engine, &[a, b]).expect("delete");
    // Then: one named step.
    assert_eq!(w.engine.undo_name(), Some("Delete Transactions"));
    assert_eq!(w.live_tx_count(), before - 2);
    w.engine.undo().expect("undo");
    assert!(w.engine.db().transaction(a).is_some_and(|t| !t.is_deleted));
    assert!(w.engine.db().transaction(b).is_some_and(|t| !t.is_deleted));
}

#[test]
/// BEH-TXN-08 — Covers: BEH-TXN-08, R-BAL-7
fn beh_txn_08_running_balance_independent_of_window() {
    // Given
    let w = World::with_fixture("world");
    let chk = w.account("Checking");
    let whole_foods = w.tx_by_payee("Whole Foods");
    let running = |period: TimePeriod| {
        let search = SearchState::new(TEST_TODAY);
        account_view(w.engine.db(), chk, None, period, &search, TEST_TODAY).running[&whole_foods]
    };
    // Then: the same value in the narrow and the widest window.
    let past_month: Decimal = running(TimePeriod::PastMonth);
    assert_eq!(past_month, running(TimePeriod::All));
    assert_eq!(past_month, m("2551.95"));
}
