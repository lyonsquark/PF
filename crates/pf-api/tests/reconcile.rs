//! API tests for date ranges and reconciliation proposals (T31): R-API-11 … R-API-14. Each test runs
//! its own server on an OS-assigned port over a temp SQLite file.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{delete, get, post};
use pf_api::start;
use pf_engine::SharedEngine;
use pf_engine::testing::World;
use pf_model::testing::TestDb;
use pf_model::{AccountType, TransactionId, TransactionStatus};

struct Running {
    port: u16,
    engine: SharedEngine,
    repaints: Arc<AtomicUsize>,
    _handle: pf_api::ApiHandle,
    coffee: TransactionId,
}

/// "Checking" with a Coffee (-5, Feb 3) and a Lunch (-7, Feb 20) transaction.
fn running() -> Running {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    let coffee = t.tx(a, "2026-02-03", "-5", "Coffee").done();
    t.tx(a, "2026-02-20", "-7", "Lunch").done();
    t.tx(a, "2026-03-20", "-9", "Later").done();
    let mut w = World::new();
    w.engine.replace_db(t.build()).expect("load");
    w.engine.clear_history();
    let engine = w.into_engine().shared();
    let repaints = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&repaints);
    let handle = start(
        Arc::clone(&engine),
        0,
        Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .expect("start");
    Running {
        port: handle.port,
        engine,
        repaints,
        _handle: handle,
        coffee,
    }
}

/// A statement that matches Coffee and lists a fee PF lacks. Opening 100, -5, -12 ⇒ 83.
const STATEMENT: &str = r#"{"period_start":"2026-02-01","period_end":"2026-02-28",
 "opening_balance":"100","closing_balance":"83",
 "lines":[{"date":"2026-02-03","description":"COFFEE","amount":"-5"},
          {"date":"2026-02-27","description":"FEE","amount":"-12"}]}"#;

const PROPOSAL: &str = "/api/accounts/checking/reconcile/proposal";

#[test]
/// Covers: R-API-11
fn r_api_11_range_with_ids() {
    let r = running();
    let (status, _, body) = get(
        r.port,
        "/api/accounts/Checking/transactions?from=2026-02-03&to=2026-02-20&n=1",
    );
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).expect("json");
    let txs = v["transactions"].as_array().expect("array");
    assert_eq!(txs.len(), 2, "both bounds inclusive; n ignored");
    assert_eq!(txs[0]["payee"], "Lunch");
    assert_eq!(txs[1]["id"], r.coffee.raw());
    let (_, _, only_from) = get(
        r.port,
        "/api/accounts/Checking/transactions?from=2026-02-20",
    );
    let v: serde_json::Value = serde_json::from_str(&only_from).expect("json");
    assert_eq!(v["count"], 2);
    let (_, _, only_to) = get(r.port, "/api/accounts/Checking/transactions?to=2026-02-03");
    let v: serde_json::Value = serde_json::from_str(&only_to).expect("json");
    assert_eq!(v["count"], 1);
    let (status, _, body) = get(r.port, "/api/accounts/Checking/transactions?from=soon");
    assert_eq!(status, 400);
    assert!(body.contains("Invalid date: soon"), "{body}");
}

#[test]
/// Covers: R-API-11, R-API-7
fn r_api_11_no_bounds_unchanged() {
    let r = running();
    let (status, _, body) = get(r.port, "/api/accounts/Checking/transactions?n=5");
    assert_eq!(status, 200);
    assert!(!body.contains("\"id\""), "no id without bounds: {body}");
}

#[test]
/// Covers: R-API-12, R-API-13
fn r_api_12_post_proposal() {
    let r = running();
    let before = r.engine.lock().expect("lock").db().all_records();
    let repaints = r.repaints.load(Ordering::SeqCst);
    let (status, ct, body) = post(r.port, PROPOSAL, STATEMENT);
    assert_eq!(status, 200, "{body}");
    assert!(ct.starts_with("application/json"));
    let v: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(v["account"], "Checking");
    assert_eq!(v["period"]["start"], "2026-02-01");
    assert_eq!(v["reading"], "asPrinted");
    assert_eq!(v["style"], "asset");
    assert_eq!(v["firstTime"], true);
    assert_eq!(v["difference"], 0);
    let matched = v["sections"]["matched"].as_array().expect("matched");
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0]["transaction"]["id"], r.coffee.raw());
    assert_eq!(matched[0]["confidence"], "high");
    assert_eq!(v["sections"]["statementOnly"][0]["description"], "FEE");
    assert_eq!(v["sections"]["pfOnlyInPeriod"][0]["payee"], "Lunch");
    let e = r.engine.lock().expect("lock");
    assert_eq!(e.proposals.len(), 1);
    assert_eq!(e.db().all_records(), before, "nothing in the books changed");
    assert!(!e.can_undo());
    assert!(
        r.repaints.load(Ordering::SeqCst) > repaints,
        "repaint requested"
    );
}

#[test]
/// Covers: R-API-12
fn r_api_12_post_errors() {
    let r = running();
    let (status, _, body) = post(r.port, PROPOSAL, "{nope");
    assert_eq!(status, 400);
    assert!(body.contains("Statement is not valid JSON"), "{body}");
    let bad = STATEMENT.replace("\"83\"", "\"90\"");
    let (status, _, body) = post(r.port, PROPOSAL, &bad);
    assert_eq!(status, 400);
    assert!(body.contains("Statement does not add up"), "{body}");
    let (status, _, text) = post(r.port, &format!("{PROPOSAL}?format=text"), "{nope");
    assert_eq!(status, 400);
    assert!(
        text.starts_with("Error: Statement is not valid JSON"),
        "{text}"
    );
    let (status, _, body) = post(
        r.port,
        "/api/accounts/Nowhere/reconcile/proposal",
        STATEMENT,
    );
    assert_eq!(status, 404);
    assert_eq!(body, "Account not found: Nowhere");
    assert!(r.engine.lock().expect("lock").proposals.is_empty());
}

#[test]
/// Covers: R-API-14
fn r_api_14_get_and_delete() {
    let r = running();
    let none = "No reconciliation proposal for account 'Checking'.";
    assert_eq!(get(r.port, PROPOSAL).0, 404);
    assert_eq!(get(r.port, PROPOSAL).2, none);
    assert_eq!(
        delete(r.port, PROPOSAL),
        (404, "text/plain; charset=utf-8".into(), none.into())
    );
    post(r.port, PROPOSAL, STATEMENT);
    // An edit after posting shows up when the proposal is read again.
    {
        let mut e = r.engine.lock().expect("lock");
        let id = r.coffee;
        e.apply("Reconcile by hand", |w| {
            w.update_transaction(id, |t| t.status = TransactionStatus::Reconciled)
        })
        .expect("edit");
    }
    let (status, _, body) = get(r.port, PROPOSAL);
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        v["sections"]["matched"].as_array().expect("matched").len(),
        0
    );
    assert_eq!(
        v["sections"]["alreadyReconciled"]
            .as_array()
            .expect("array")
            .len(),
        1
    );
    let repaints = r.repaints.load(Ordering::SeqCst);
    let (status, _, body) = delete(r.port, PROPOSAL);
    assert_eq!((status, body.as_str()), (200, "Proposal removed."));
    assert!(r.repaints.load(Ordering::SeqCst) > repaints);
    assert_eq!(get(r.port, PROPOSAL).0, 404);
}

#[test]
/// Covers: R-API-13
fn r_api_13_text_format() {
    let r = running();
    let (status, ct, body) = post(r.port, &format!("{PROPOSAL}?format=text"), STATEMENT);
    assert_eq!(status, 200, "{body}");
    assert!(ct.starts_with("text/plain"));
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines[0], "Checking — statement 2026-02-01 to 2026-02-28");
    assert_eq!(
        lines[1],
        "Read as: asset balances, signs as printed (1 of 2 lines match)"
    );
    assert!(body.contains("Difference: $0.00"));
    assert!(body.contains("Matched (1)"));
    assert!(body.contains("On statement only (1)"));
    assert!(body.contains("2026-02-27  -$12.00       FEE"));
    assert!(body.contains("In PF only, in this period (1)"));
}
