//! Parity tests against output captured from the Swift app (T12, L6). Ignored until the captures
//! exist (T26). Each case compares the API reply on `fixtures/SampleData2.md` with
//! `fixtures/parity/<route>__<format>.txt`. JSON is compared as parsed values, numbers within 0.005.

mod common;

use common::get;
use pf_api::start;
use pf_engine::testing::World;
use serde_json::Value;

/// Route, query suffix and format name for each capture file.
const CASES: &[(&str, &str, &str)] = &[
    ("balances", "/api/accounts/balances?format=json", "json"),
    ("balances", "/api/accounts/balances?format=text", "text"),
    (
        "balances",
        "/api/accounts/balances?format=markdown",
        "markdown",
    ),
    ("scenarios", "/api/scenarios?format=json", "json"),
    ("scenarios", "/api/scenarios?format=text", "text"),
    ("scenarios", "/api/scenarios?format=markdown", "markdown"),
    (
        "transactions",
        "/api/accounts/Checking/transactions?n=5&format=json",
        "json",
    ),
    (
        "transactions",
        "/api/accounts/Checking/transactions?n=5&format=text",
        "text",
    ),
    (
        "transactions",
        "/api/accounts/Checking/transactions?n=5&format=markdown",
        "markdown",
    ),
    ("export", "/api/export/markdown", "markdown"),
];

/// Reads a capture file from `fixtures/parity`.
fn capture(route: &str, format: &str) -> String {
    let p = format!(
        "{}/../../fixtures/parity/{route}__{format}.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("capture {p}: {e}"))
}

/// Equal as JSON values, with numbers within 0.005 of each other.
fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(p), Some(q)) => (p - q).abs() <= 0.005,
            _ => false,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w)))
        }
        _ => a == b,
    }
}

#[test]
#[ignore = "needs fixtures/parity (T26)"]
/// Covers: R-API-2, R-API-3, R-API-6, R-API-7
fn parity_all_routes_against_swift_captures() {
    let w = World::with_fixture("SampleData2");
    let engine = w.into_engine().shared();
    let handle = start(engine, 0, std::sync::Arc::new(|| {})).expect("start on port 0");
    for (route, path, format) in CASES {
        let (_, _, body) = get(handle.port, path);
        let expected = capture(route, format);
        if *format == "json" {
            let ours: Value = serde_json::from_str(&body).expect("our JSON");
            let theirs: Value = serde_json::from_str(&expected).expect("captured JSON");
            assert!(same_json(&ours, &theirs), "{route} json differs");
        } else {
            assert_eq!(body, expected, "{route} {format} differs");
        }
    }
}
