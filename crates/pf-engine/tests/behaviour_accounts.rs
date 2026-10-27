//! L3 behaviour and rule tests for accounts (T10a: BEH-ACC-01..04, R-ACC-1..4). Driven through
//! the public engine API on the `world` fixture, as the UI would call it.

use pf_engine::EngineError;
use pf_engine::commands::accounts::{
    AccountInput, calculate_starting_balance, create_account, delete_account, set_starting_balance,
    starting_balance_info, update_account,
};
use pf_engine::testing::{TEST_TODAY, World, d, m};
use pf_model::{AccountId, AccountType, Transaction, TransactionId, TransactionStatus};

/// Form values for an account with no institution, an empty currency and not external.
fn input(name: &str, account_type: AccountType) -> AccountInput {
    AccountInput {
        name: name.into(),
        account_type,
        institution: "Bank".into(),
        currency: String::new(),
        is_external: false,
    }
}

/// Names of the live accounts, sorted as the sidebar shows them.
fn live_names(w: &World) -> Vec<String> {
    w.engine
        .db()
        .accounts_by_name()
        .iter()
        .map(|a| a.name.clone())
        .collect()
}

/// Live transactions of one account, by id.
fn live_tx_ids(w: &World, account: AccountId) -> Vec<TransactionId> {
    w.engine
        .db()
        .transactions_for_account(account)
        .map(|t| t.id)
        .collect()
}

/// The live starting-balance rows of one account (payee "Starting Balance", any scenario).
fn starting_rows(w: &World, account: AccountId) -> Vec<Transaction> {
    w.engine
        .db()
        .transactions_for_account(account)
        .filter(|t| t.payee == "Starting Balance")
        .cloned()
        .collect()
}

#[test]
/// BEH-ACC-01 — Covers: BEH-ACC-01, R-ACC-4
fn beh_acc_01_create_account_sorted_and_persisted() {
    // Given
    let mut w = World::with_fixture("world");
    // When
    let amex = create_account(&mut w.engine, input("Amex", AccountType::Credit)).expect("create");
    // Then
    let expected = vec![
        "Amex",
        "Checking",
        "External 401k",
        "Mortgage",
        "Savings",
        "Visa",
    ];
    assert_eq!(live_names(&w), expected);
    assert_eq!(w.account("Amex"), amex);
    // Persisted: a restart still shows it.
    w.reload();
    assert_eq!(live_names(&w), expected);
    let row = w.engine.db().account(amex).expect("persisted account");
    assert_eq!(row.account_type, AccountType::Credit);
    assert_eq!(row.currency, "USD", "an empty currency becomes USD");
}

#[test]
/// BEH-ACC-02 — Covers: BEH-ACC-02, R-ACC-1
fn beh_acc_02_delete_account_undo_redo() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let tax = w.obligation("Property Tax");
    assert_eq!(live_tx_ids(&w, chk).len(), 9, "Checking has nine rows");
    // When
    delete_account(&mut w.engine, chk).expect("delete");
    // Then: the account, its rows and its obligation are hidden.
    assert!(!live_names(&w).contains(&"Checking".to_string()));
    assert!(live_tx_ids(&w, chk).is_empty());
    assert!(w.engine.db().obligation(tax).is_some_and(|o| o.is_deleted));
    // When: undo.
    w.engine.undo().expect("undo");
    // Then: everything is back and the balance is restored.
    assert!(live_names(&w).contains(&"Checking".to_string()));
    assert_eq!(live_tx_ids(&w, chk).len(), 9);
    assert!(w.engine.db().obligation(tax).is_some_and(|o| !o.is_deleted));
    assert_eq!(w.balance(chk), m("2051.95"));
    // When: redo.
    w.engine.redo().expect("redo");
    // Then: hidden again.
    assert!(!live_names(&w).contains(&"Checking".to_string()));
    assert!(live_tx_ids(&w, chk).is_empty());
}

#[test]
/// BEH-ACC-03 — Covers: BEH-ACC-03, R-ACC-2
fn beh_acc_03_starting_balance_set_then_updated() {
    // Given: Visa has no starting-balance row; its earliest other row is 2026-01-25.
    let mut w = World::with_fixture("world");
    let visa = w.account("Visa");
    assert!(
        starting_balance_info(w.engine.db(), visa, TEST_TODAY)
            .existing
            .is_none()
    );
    // When
    set_starting_balance(&mut w.engine, visa, m("100")).expect("set 100");
    // Then
    let rows = starting_rows(&w, visa);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].date, d("2026-01-24"));
    assert_eq!(rows[0].amount, m("100"));
    let first_id = rows[0].id;
    // When: edit again.
    set_starting_balance(&mut w.engine, visa, m("150")).expect("set 150");
    // Then: the same row now holds 150, and there is still one starting row.
    let rows = starting_rows(&w, visa);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, first_id);
    assert_eq!(rows[0].amount, m("150"));
}

#[test]
/// BEH-ACC-04 — Covers: BEH-ACC-04, R-ACC-3
fn beh_acc_04_calculate_matches_entered_current() {
    // Given: Checking with a starting row of 2000 and other rows summing to 51.95.
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    // When: "Calculate" from a current balance of 5000, then save.
    let info = starting_balance_info(w.engine.db(), chk, TEST_TODAY);
    let amount = calculate_starting_balance(m("5000"), info.other_sum);
    set_starting_balance(&mut w.engine, chk, amount).expect("save");
    // Then
    assert_eq!(w.reality_balance(chk), m("5000"));
}

#[test]
/// R-ACC-1 — Covers: R-ACC-1
fn r_acc_1_delete_account_cascades() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let tax = w.obligation("Property Tax");
    let rows = live_tx_ids(&w, chk);
    // When
    delete_account(&mut w.engine, chk).expect("delete");
    // Then: the account, every row it owned and the obligation are soft-deleted (not removed).
    assert!(w.engine.db().account(chk).is_some_and(|a| a.is_deleted));
    for id in rows {
        assert!(w.engine.db().transaction(id).is_some_and(|t| t.is_deleted));
    }
    assert!(w.engine.db().obligation(tax).is_some_and(|o| o.is_deleted));
    assert_eq!(w.engine.undo_name(), Some("Delete Account"));
    // Then: Savings is untouched, including its transfer row from Checking.
    let savings = w.account("Savings");
    assert_eq!(live_tx_ids(&w, savings).len(), 2);
}

#[test]
/// R-ACC-2 — Covers: R-ACC-2
fn r_acc_2_starting_balance_date_is_day_before_earliest_other() {
    // Given: Checking's earliest other row is 2026-01-02 (Landlord).
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    // And: a Plan A row dated before everything else must be ignored.
    let plan = w.scenario("Plan A");
    w.engine
        .apply("Add Transaction", |e| {
            let mut t = Transaction::new(TransactionId(0), d("2025-06-01"), m("-7"), "Plan early");
            t.account_id = Some(chk);
            t.scenario_id = Some(plan);
            e.insert_transaction(t);
            Ok(())
        })
        .expect("insert scenario row");
    // When
    let info = starting_balance_info(w.engine.db(), chk, TEST_TODAY);
    // Then
    assert_eq!(info.balance_date, d("2026-01-01"));
    assert_eq!(info.earliest_other, Some(d("2026-01-02")));
    let existing = info.existing.expect("the 2000 starting row exists");
    assert_eq!(
        w.engine.db().transaction(existing).map(|t| t.amount),
        Some(m("2000"))
    );
}

#[test]
/// R-ACC-2 — Covers: R-ACC-2
fn r_acc_2_no_transactions_zero_amount_noop() {
    // Given: Mortgage has no transactions.
    let mut w = World::with_fixture("world");
    let mortgage = w.account("Mortgage");
    // When
    set_starting_balance(&mut w.engine, mortgage, m("0")).expect("noop");
    // Then: no row, no undo step.
    assert!(starting_rows(&w, mortgage).is_empty());
    assert!(!w.engine.can_undo());
}

#[test]
/// R-ACC-2 — Covers: R-ACC-2
fn r_acc_2_no_transactions_uses_today_minus_one() {
    // Given
    let mut w = World::with_fixture("world");
    let mortgage = w.account("Mortgage");
    // When
    set_starting_balance(&mut w.engine, mortgage, m("-250000")).expect("set");
    // Then: dated the day before today (TEST_TODAY is 2026-03-15) with the Swift payee and memo.
    let rows = starting_rows(&w, mortgage);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].date, d("2026-03-14"));
    assert_eq!(rows[0].amount, m("-250000"));
    assert_eq!(rows[0].memo.as_deref(), Some("Initial account balance"));
    assert_eq!(rows[0].status, TransactionStatus::Cleared);
    assert_eq!(rows[0].scenario_id, None);
    assert_eq!(w.engine.undo_name(), Some("Set Starting Balance"));
}

#[test]
/// R-ACC-3 — Covers: R-ACC-3
fn r_acc_3_calculate() {
    assert_eq!(
        calculate_starting_balance(m("3000"), m("51.95")),
        m("2948.05")
    );
}

#[test]
/// R-ACC-4 — Covers: R-ACC-4. Validation of the Add and Edit Account forms (the spec's name).
fn r_acc_4_validation_empty_name() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    // When / Then: create and update both refuse a blank name, with no undo step.
    let blank = create_account(&mut w.engine, input("   ", AccountType::Cash));
    assert_eq!(
        blank,
        Err(EngineError::Validation("Account name is required".into()))
    );
    let edit = update_account(&mut w.engine, chk, input("", AccountType::Checking));
    assert_eq!(
        edit,
        Err(EngineError::Validation("Account name is required".into()))
    );
    assert!(!w.engine.can_undo());
    // And: the name is trimmed on save.
    let id = create_account(&mut w.engine, input("  Cash box  ", AccountType::Cash)).expect("ok");
    assert_eq!(
        w.engine.db().account(id).map(|a| a.name.as_str()),
        Some("Cash box")
    );
}
