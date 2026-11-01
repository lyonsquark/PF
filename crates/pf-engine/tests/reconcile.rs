//! L3 tests for the reconciliation commands (T29): R-REC-14 … R-REC-16, BEH-REC-01.

use pf_core::reconcile::{
    Choice, Reading, ReconcileInput, Report, Statement, StatementLine, reconcile,
};
use pf_engine::commands::reconcile::{
    Apply, apply_reconciliation, payee_aliases, preferred_reading,
};
use pf_engine::testing::World;
use pf_engine::{EngineError, RowKey};
use pf_model::testing::{TestDb, d, m};
use pf_model::{AccountId, AccountType, Db, TransactionId, TransactionStatus};

fn world(db: Db) -> World {
    let mut w = World::new();
    w.engine.replace_db(db).expect("load");
    w.engine.clear_history();
    w
}

fn line(date: &str, desc: &str, amount: &str) -> StatementLine {
    StatementLine {
        date: d(date),
        post_date: None,
        description: desc.into(),
        amount: m(amount),
        section: String::new(),
        reference: String::new(),
    }
}

fn statement(
    start: &str,
    end: &str,
    opening: &str,
    closing: &str,
    lines: Vec<StatementLine>,
) -> Statement {
    Statement {
        institution: String::new(),
        account_hint: String::new(),
        period_start: d(start),
        period_end: d(end),
        opening_balance: m(opening),
        closing_balance: m(closing),
        total_credits: None,
        total_debits: None,
        lines,
    }
}

fn report(w: &World, a: AccountId, st: &Statement) -> Report {
    let preferred = preferred_reading(w.engine.db(), a);
    reconcile(&ReconcileInput {
        db: w.engine.db(),
        account: a,
        statement: st,
        preferred,
    })
    .expect("report")
}

fn run(
    w: &mut World,
    a: AccountId,
    st: &Statement,
    r: &Report,
    c: &Choice,
    mark: bool,
) -> Result<usize, EngineError> {
    apply_reconciliation(
        &mut w.engine,
        &Apply {
            account: a,
            statement: st,
            report: r,
            choice: c,
            mark_before: mark,
        },
    )
}

fn status(w: &World, id: TransactionId) -> TransactionStatus {
    w.engine.db().transaction(id).expect("tx").status
}

struct Fx {
    w: World,
    a: AccountId,
    coffee: TransactionId,
    pizza: TransactionId,
    st: Statement,
}

/// Coffee matches; Pizza differs by a tip; the fee and the Blue Bottle line are missing from PF.
fn fixture() -> Fx {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let fees = t.category("Fees", pf_model::CategoryType::Expense);
    t.patterns(fees, &["MONTHLY FEE"]);
    t.import_mapping(
        "statementPayee",
        "BLUE BOTTLE",
        "Blue Bottle Coffee",
        Some(a),
    );
    let coffee = t.tx(a, "2026-02-03", "-5", "Coffee").done();
    let pizza = t.tx(a, "2026-02-05", "-41.17", "Pizza").done();
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "19.83",
        vec![
            line("2026-02-03", "COFFEE", "-5"),
            line("2026-02-05", "PIZZA", "-43.17"),
            line("2026-02-27", "MONTHLY FEE", "-12"),
            line("2026-02-10", "SQ *BLUE BOTTLE 12", "-20"),
        ],
    );
    Fx {
        w: world(t.build()),
        a,
        coffee,
        pizza,
        st,
    }
}

/// Covers: R-REC-14
#[test]
fn r_rec_14_marks_pairs_and_adds_lines() {
    let mut f = fixture();
    let r = report(&f.w, f.a, &f.st);
    let mut c = Choice::default_for(&r);
    for p in &mut c.pairs {
        p.2 = p.1 == f.pizza; // use the statement amount for the tip
    }
    let n = run(&mut f.w, f.a, &f.st, &r, &c, false).expect("apply");
    assert_eq!(n, 4);
    assert_eq!(status(&f.w, f.coffee), TransactionStatus::Reconciled);
    assert_eq!(status(&f.w, f.pizza), TransactionStatus::Reconciled);
    let db = f.w.engine.db();
    assert_eq!(db.transaction(f.pizza).expect("tx").amount, m("-43.17"));
    assert_eq!(db.transaction(f.coffee).expect("tx").amount, m("-5"));
    let fee = db
        .live_transactions()
        .find(|t| t.payee == "MONTHLY FEE")
        .expect("fee");
    assert_eq!(
        (fee.amount, fee.date, fee.account_id),
        (m("-12"), d("2026-02-27"), Some(f.a))
    );
    assert_eq!(fee.status, TransactionStatus::Reconciled);
    assert_eq!(fee.memo, None);
    assert!(fee.category_id.is_some(), "R-CAT-1 on the payee");
    assert!(fee.scenario_id.is_none());
    let blue = db
        .live_transactions()
        .find(|t| t.payee == "Blue Bottle Coffee")
        .expect("alias payee");
    assert_eq!(blue.memo.as_deref(), Some("SQ *BLUE BOTTLE 12"));
    assert_eq!(blue.status, TransactionStatus::Reconciled);
    let after = report(&f.w, f.a, &f.st);
    assert!(after.matched.is_empty() && after.amount_differs.is_empty());
}

/// Covers: R-REC-14, R-UND-1
#[test]
fn r_rec_14_one_undo_step() {
    let mut f = fixture();
    let before = f.w.engine.db().all_records();
    let r = report(&f.w, f.a, &f.st);
    let c = Choice {
        adjustment: Some(m("-1")),
        ..Choice::default_for(&r)
    };
    run(&mut f.w, f.a, &f.st, &r, &c, false).expect("apply");
    assert_eq!(f.w.engine.undo_name(), Some("Reconcile Statement"));
    let applied = f.w.engine.db().all_records();
    assert_ne!(before, applied);
    f.w.engine.undo().expect("undo");
    assert_eq!(f.w.engine.db().all_records(), before);
    assert!(!f.w.engine.can_undo());
    f.w.engine.redo().expect("redo");
    assert_eq!(f.w.engine.db().all_records(), applied);
}

/// Covers: R-REC-14
#[test]
fn r_rec_14_refusals() {
    let mut f = fixture();
    let r = report(&f.w, f.a, &f.st);
    let c = Choice::default_for(&r);

    // A deleted pair.
    f.w.engine
        .apply("Delete", |e| {
            e.set_deleted(RowKey::Transaction(f.coffee), true)
        })
        .expect("delete");
    let steps = f.w.engine.undo_name().map(str::to_string);
    let err = run(&mut f.w, f.a, &f.st, &r, &c, false).expect_err("deleted");
    assert_eq!(
        err.to_string(),
        format!("Transaction {} cannot be reconciled.", f.coffee)
    );
    assert_eq!(f.w.engine.undo_name().map(str::to_string), steps);
    f.w.engine.undo().expect("undo");

    // A pair in another account.
    let mut other = fixture();
    {
        let mut db = other.w.engine.db().clone();
        let id = pf_model::AccountId(db.alloc_id());
        db.accounts.insert(
            id,
            pf_model::Account::new(id, "Other", AccountType::Checking, "X"),
        );
        db.transactions
            .get_mut(&other.coffee)
            .expect("tx")
            .account_id = Some(id);
        other.w.engine.replace_db(db).expect("load");
        other.w.engine.clear_history();
    }
    let r2 = report(&other.w, other.a, &other.st);
    let c2 = Choice {
        pairs: vec![(0, other.coffee, false)],
        add_lines: vec![],
        adjustment: None,
    };
    let err = run(&mut other.w, other.a, &other.st, &r2, &c2, false).expect_err("other account");
    assert_eq!(
        err.to_string(),
        format!("Transaction {} cannot be reconciled.", other.coffee)
    );
    assert!(!other.w.engine.can_undo());

    // A scenario is active.
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    t.scenario("Plan");
    let tx = t.tx(a, "2026-02-03", "-5", "Coffee").done();
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "95",
        vec![line("2026-02-03", "COFFEE", "-5")],
    );
    let mut w = world(t.build());
    let r3 = report(&w, a, &st);
    w.activate_scenario("Plan");
    let c3 = Choice {
        pairs: vec![(0, tx, false)],
        add_lines: vec![],
        adjustment: None,
    };
    let err = run(&mut w, a, &st, &r3, &c3, false).expect_err("scenario");
    assert_eq!(err.to_string(), "Switch to Reality to reconcile.");
    assert!(!w.engine.can_undo());
}

/// Covers: R-REC-14
#[test]
fn r_rec_14_adjustment_and_mark_before() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let b = t.account("Other", AccountType::Checking);
    let old = t.tx(a, "2026-01-10", "-3", "Old").done();
    let old_rec = t
        .tx(a, "2026-01-11", "-4", "OldRec")
        .status(TransactionStatus::Reconciled)
        .done();
    let other_acct = t.tx(b, "2026-01-12", "-6", "Elsewhere").done();
    let inside = t.tx(a, "2026-02-20", "-9", "Inside").done();
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "95",
        vec![line("2026-02-03", "COFFEE", "-5")],
    );
    let mut w = world(t.build());
    let r = report(&w, a, &st);
    let c = Choice {
        pairs: vec![],
        add_lines: vec![0],
        adjustment: Some(m("2.50")),
    };
    let n = run(&mut w, a, &st, &r, &c, true).expect("apply");
    assert_eq!(
        n, 2,
        "added line + one older row; the adjustment is not counted"
    );
    assert_eq!(status(&w, old), TransactionStatus::Reconciled);
    assert_eq!(status(&w, old_rec), TransactionStatus::Reconciled);
    assert_eq!(status(&w, other_acct), TransactionStatus::Cleared);
    assert_eq!(status(&w, inside), TransactionStatus::Cleared);
    let adj = w
        .engine
        .db()
        .live_transactions()
        .find(|t| t.payee == "Reconciliation adjustment")
        .expect("adjustment");
    assert_eq!((adj.amount, adj.date), (m("2.50"), d("2026-02-28")));
    assert_eq!(
        adj.memo.as_deref(),
        Some("Statement 2026-02-01 to 2026-02-28")
    );
    assert_eq!(
        (adj.status, adj.category_id),
        (TransactionStatus::Reconciled, None)
    );
}

/// Covers: R-REC-15
#[test]
fn r_rec_15_aliases_saved_and_updated() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    t.tx(a, "2026-02-03", "-5", "Blue Bottle Coffee").done();
    t.tx(a, "2026-02-05", "-7", "Trader Joe's").done();
    t.tx(a, "2026-02-07", "-8", "").done();
    t.tx(a, "2026-02-09", "-9", "Amazon").done();
    t.import_mapping("statementPayee", "AMZN MKTP US", "Old Name", Some(a));
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "71",
        vec![
            line("2026-02-03", "SQ *BBC ROASTERS 88", "-5"),
            line("2026-02-05", "TRADER JOE'S #552", "-7"),
            line("2026-02-07", "MYSTERY SHOP", "-8"),
            line("2026-02-09", "AMZN Mktp US*1A2", "-9"),
        ],
    );
    let mut w = world(t.build());
    let r = report(&w, a, &st);
    let c = Choice::default_for(&r);
    assert_eq!(c.pairs.len(), 4);
    run(&mut w, a, &st, &r, &c, false).expect("apply");
    let mut maps: Vec<_> = w
        .engine
        .db()
        .import_mappings
        .values()
        .filter(|m| m.mapping_type == "statementPayee")
        .map(|m| (m.source_value.clone(), m.target_value.clone(), m.account_id))
        .collect();
    maps.sort();
    assert_eq!(
        maps,
        vec![
            ("AMZN MKTP US".to_string(), "Amazon".to_string(), Some(a)),
            (
                "BBC ROASTERS".to_string(),
                "Blue Bottle Coffee".to_string(),
                Some(a)
            ),
        ]
    );
    assert_eq!(payee_aliases(w.engine.db(), a)[0].0, "AMZN MKTP US");
}

/// Covers: R-REC-16
#[test]
fn r_rec_16_reading_remembered() {
    let mut f = fixture();
    assert_eq!(preferred_reading(f.w.engine.db(), f.a), None);
    let r = report(&f.w, f.a, &f.st);
    run(&mut f.w, f.a, &f.st, &r, &Choice::default_for(&r), false).expect("apply");
    assert_eq!(
        f.w.engine
            .db()
            .setting(&format!("reconcileReading.{}", f.a.raw())),
        Some("asPrinted")
    );
    assert_eq!(
        preferred_reading(f.w.engine.db(), f.a),
        Some(Reading::AsPrinted)
    );
    f.w.engine
        .apply("Set", |e| {
            e.set_setting(&format!("reconcileReading.{}", f.a.raw()), "flipped");
            Ok(())
        })
        .expect("set");
    assert_eq!(
        preferred_reading(f.w.engine.db(), f.a),
        Some(Reading::Flipped)
    );
    f.w.engine
        .apply("Set", |e| {
            e.set_setting(&format!("reconcileReading.{}", f.a.raw()), "garbage");
            Ok(())
        })
        .expect("set");
    assert_eq!(preferred_reading(f.w.engine.db(), f.a), None);
}

/// Covers: R-REC-13, R-REC-14
#[test]
fn beh_rec_01_reconcile_then_next_statement() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    t.tx(a, "2026-08-31", "100", "Starting balance").done();
    t.tx(a, "2026-09-05", "-10", "Coffee").done();
    t.tx(a, "2026-10-06", "-20", "Lunch").done();
    let sept = statement(
        "2026-09-01",
        "2026-09-30",
        "100",
        "90",
        vec![line("2026-09-05", "COFFEE", "-10")],
    );
    let oct = statement(
        "2026-10-01",
        "2026-10-31",
        "90",
        "70",
        vec![line("2026-10-06", "LUNCH", "-20")],
    );
    let mut w = world(t.build());
    let r = report(&w, a, &sept);
    assert!(r.first_time);
    assert_eq!(r.opening_difference, m("100"));
    run(&mut w, a, &sept, &r, &Choice::default_for(&r), true).expect("september");
    let r = report(&w, a, &oct);
    assert!(!r.first_time);
    assert_eq!(r.opening_difference, m("0"));
    assert_eq!(
        pf_core::reconcile::difference(&r, &Choice::default_for(&r)),
        m("0")
    );
}
