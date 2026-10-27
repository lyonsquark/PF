//! API integration tests (T12, L4). Each test runs its own server on an OS-assigned loopback port
//! over a temp SQLite file (`World`), in America/New_York time. Nothing binds port 8989 and
//! nothing touches the app's real database.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{get, post, send_bytes, send_raw};
use pf_api::{ApiHandle, start};
use pf_core::csv_import::{TranslationChoices, detect_columns, parse_csv, plan_mappings};
use pf_engine::SharedEngine;
use pf_engine::commands::backup::export_markdown;
use pf_engine::commands::import::{CsvImportRequest, run_csv_import};
use pf_engine::testing::World;

/// The bank export used to save Checking's column mappings and to re-import.
const BANK_CSV: &str = "Date,Description,Amount\n03/01/2026,Corner Deli,-12.00\n03/02/2026,\"ACME, INC PAYROLL\",4000.00\n";

/// A running API over a fresh engine, with a repaint counter.
struct Running {
    handle: ApiHandle,
    engine: SharedEngine,
    repaints: Arc<AtomicUsize>,
    file: PathBuf,
}

impl Running {
    /// The bound loopback port.
    fn port(&self) -> u16 {
        self.handle.port
    }
}

/// Starts the API on port 0 over the `fixture` world. The engine is not reachable from outside
/// the test except through `engine`.
fn running(fixture: &str) -> Running {
    let w = World::with_fixture(fixture);
    let file = w.path().to_path_buf();
    let engine = w.into_engine().shared();
    start_on(engine, file)
}

/// Starts the API on port 0 over an existing shared engine.
fn start_on(engine: SharedEngine, file: PathBuf) -> Running {
    let repaints = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&repaints);
    let handle = start(
        Arc::clone(&engine),
        0,
        Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .expect("start the API on an OS-assigned port");
    Running {
        handle,
        engine,
        repaints,
        file,
    }
}

/// Saves Checking's bank column mappings the way the wizard does, then clears undo history so
/// the next undo step is the API's own.
fn fixture_with_saved_checking_mappings() -> Running {
    let mut w = World::with_fixture("world");
    let file = w.path().to_path_buf();
    let parsed = parse_csv(BANK_CSV);
    let mapping = detect_columns(&parsed.headers);
    let checking = w.account("Checking");
    let mapping_plan = plan_mappings(
        w.engine.db(),
        &parsed.headers,
        &mapping,
        &TranslationChoices::default(),
        Some(checking),
        false,
    );
    run_csv_import(
        &mut w.engine,
        CsvImportRequest {
            is_monarch: false,
            target_account: Some(checking),
            rows: parsed.rows,
            mapping,
            new_accounts: Vec::new(),
            mapping_plan,
        },
    )
    .expect("bank import saves the column mappings");
    w.engine.clear_history();
    let engine = w.into_engine().shared();
    start_on(engine, file)
}

#[test]
/// Covers: R-API-1
fn r_api_1_binds_loopback_only() {
    // Given: a server on an OS-assigned port.
    let r = running("world");
    // Then: the real port is reported (not 0).
    assert_ne!(r.port(), 0);
    // Then: a non-loopback address of this machine refuses the connection (skipped when the
    // machine has no non-loopback IPv4 address).
    if let Some(lan) = lan_address() {
        let addr = std::net::SocketAddr::new(lan, r.port());
        let attempt = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(1));
        assert!(attempt.is_err(), "API answered on {addr}");
    }
    // Then: a second server on the same port fails to bind.
    let again = start(Arc::clone(&r.engine), r.port(), Arc::new(|| {}));
    assert!(again.is_err(), "binding the same port twice must fail");
}

/// This machine's first non-loopback IPv4 address, found without sending any packet.
fn lan_address() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[test]
/// Covers: R-API-2
fn r_api_2_unknown_route_404() {
    let r = running("world");
    let (status, content_type, body) = get(r.port(), "/nope?x=1");
    assert_eq!(status, 404);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert_eq!(body, "Not Found: GET /nope");
}

#[test]
/// Covers: R-API-2
fn r_api_2_db_path() {
    let r = running("world");
    let (status, content_type, body) = get(r.port(), "/api/db-path");
    assert_eq!(status, 200);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    let file_name = r.file.file_name().expect("file name").to_string_lossy();
    assert!(
        body.ends_with(file_name.as_ref()),
        "{body} should end with {file_name}"
    );
}

#[test]
/// Covers: R-API-3
fn r_api_3_headers() {
    let r = running("world");
    let raw = send_raw(
        r.port(),
        "GET /api/accounts/balances HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(raw.status, 200);
    assert_eq!(raw.content_type(), "application/json; charset=utf-8");
    assert_eq!(raw.header("Connection"), Some("close"));
    assert_eq!(raw.header("Access-Control-Allow-Origin"), Some("*"));
}

#[test]
/// Covers: R-API-3
fn r_api_3_uppercase_json_quirk() {
    let r = running("world");
    let (status, content_type, body) = get(r.port(), "/api/accounts/balances?format=JSON");
    assert_eq!(status, 200);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert!(body.starts_with("{\n"), "JSON body expected, got {body}");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    assert!(parsed["accounts"].is_array());
}

#[test]
/// Covers: R-API-4
fn r_api_4_unknown_scenario_200_error_body() {
    let r = running("world");
    let (status, _, body) = get(r.port(), "/api/accounts/balances?scenario=Nope");
    assert_eq!(status, 200);
    assert_eq!(body, r#"{"error":"Scenario 'Nope' not found."}"#);
    let (status, content_type, body) =
        get(r.port(), "/api/accounts/balances?scenario=Nope&format=text");
    assert_eq!(status, 200);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert_eq!(body, "Error: Scenario 'Nope' not found.");
}

#[test]
/// Covers: R-API-4
fn r_api_4_reality_keyword() {
    let r = running("world");
    let (_, _, plain) = get(r.port(), "/api/accounts/balances");
    let (_, _, reality) = get(r.port(), "/api/accounts/balances?scenario=reality");
    let (_, _, upper) = get(r.port(), "/api/accounts/balances?scenario=REALITY");
    assert_eq!(reality, plain);
    assert_eq!(upper, plain);
}

#[test]
/// Covers: R-API-5
fn r_api_5_percent_and_plus_decoding() {
    let r = running("world");
    // Percent-decoded account name: "External 401k" has a space.
    let (status, _, body) = get(r.port(), "/api/accounts/External%20401k/balance");
    assert_eq!(status, 200);
    assert!(body.contains("\"name\" : \"External 401k\""), "{body}");
    // Query '+' is a space: the scenario "Plan A" is found.
    let (status, _, body) = get(r.port(), "/api/accounts/Checking/balance?scenario=Plan+A");
    assert_eq!(status, 200);
    assert!(body.contains("551.95"), "{body}");
    // Names match case-insensitively.
    let (status, _, _) = get(r.port(), "/api/accounts/CHECKING/balance");
    assert_eq!(status, 200);
}

#[test]
/// Covers: R-API-5
fn r_api_5_account_not_found_404() {
    let r = running("world");
    let (status, content_type, body) = get(r.port(), "/api/accounts/Nope/balance");
    assert_eq!(status, 404);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert_eq!(body, "Account not found: Nope");
}

#[test]
/// Covers: R-API-6
fn r_api_6_balances_all_formats() {
    let r = running("world");
    let (_, _, json) = get(r.port(), "/api/accounts/balances?format=json");
    insta::assert_snapshot!("r_api_6_balances_json", json);
    let (_, _, text) = get(r.port(), "/api/accounts/balances?format=text");
    insta::assert_snapshot!("r_api_6_balances_text", text);
    let (_, _, markdown) = get(r.port(), "/api/accounts/balances?format=markdown");
    insta::assert_snapshot!("r_api_6_balances_markdown", markdown);
}

#[test]
/// Covers: R-API-6
fn r_api_6_balance_single_account() {
    let r = running("world");
    let (status, content_type, body) = get(
        r.port(),
        "/api/accounts/Checking/balance?scenario=Plan+A&format=text",
    );
    assert_eq!(status, 200);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert_eq!(body, "Checking (Chase): $551.95\n\nTotal: $551.95");
}

#[test]
/// Covers: R-API-7
fn r_api_7_transactions_all_formats() {
    let r = running("world");
    let (_, _, json) = get(
        r.port(),
        "/api/accounts/Checking/transactions?n=5&format=json",
    );
    insta::assert_snapshot!("r_api_7_transactions_json", json);
    let (_, _, text) = get(
        r.port(),
        "/api/accounts/Checking/transactions?n=5&format=text",
    );
    insta::assert_snapshot!("r_api_7_transactions_text", text);
    let (_, _, markdown) = get(
        r.port(),
        "/api/accounts/Checking/transactions?n=5&format=markdown",
    );
    insta::assert_snapshot!("r_api_7_transactions_markdown", markdown);
}

#[test]
/// Covers: R-API-7
fn r_api_7_default_n_is_10() {
    let r = running("world");
    let count = |path: &str| -> u64 {
        let (_, _, body) = get(r.port(), path);
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("JSON body");
        parsed["count"].as_u64().expect("count")
    };
    // Checking has 8 reality rows, all under the default of 10.
    assert_eq!(count("/api/accounts/Checking/transactions"), 8);
    // With Plan A, 9 rows are visible.
    assert_eq!(
        count("/api/accounts/Checking/transactions?scenario=Plan+A"),
        9
    );
    // A non-integer n falls back to 10.
    assert_eq!(count("/api/accounts/Checking/transactions?n=many"), 8);
}

#[test]
/// Covers: R-API-8
fn r_api_8_import_without_mappings_400() {
    let r = running("world");
    let (status, content_type, body) = post(r.port(), "/api/accounts/Checking/import", BANK_CSV);
    assert_eq!(status, 400);
    assert_eq!(content_type, "application/json; charset=utf-8");
    assert_eq!(
        body,
        r#"{"error":"No saved column mappings for account 'Checking'. Please import via the UI first to set up column mappings."}"#
    );
}

#[test]
/// Covers: R-API-8, R-CSV-11
fn r_api_8_import_success_then_duplicates() {
    let r = fixture_with_saved_checking_mappings();
    // Re-posting the same export: both rows are duplicates.
    let (status, _, body) = post(r.port(), "/api/accounts/Checking/import", BANK_CSV);
    assert_eq!(status, 200);
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("JSON body");
    assert_eq!(parsed["imported"], 0);
    assert_eq!(parsed["skippedDuplicates"], 2);
    // A new row is imported.
    let csv = "Date,Description,Amount\n03/03/2026,Night Market,-7.00\n";
    let (status, _, body) = post(r.port(), "/api/accounts/Checking/import", csv);
    assert_eq!(status, 200);
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("JSON body");
    assert_eq!(parsed["imported"], 1);
    assert_eq!(parsed["account"], "Checking");
}

#[test]
/// Covers: R-API-9
fn r_api_9_mutation_goes_through_engine_and_repaints() {
    let r = fixture_with_saved_checking_mappings();
    assert_eq!(r.repaints.load(Ordering::SeqCst), 0);
    let csv = "Date,Description,Amount\n03/03/2026,Night Market,-7.00\n";
    let (status, _, _) = post(r.port(), "/api/accounts/Checking/import", csv);
    assert_eq!(status, 200);
    // The repaint follows the reply; poll briefly for it.
    let deadline = Instant::now() + Duration::from_secs(2);
    while r.repaints.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(r.repaints.load(Ordering::SeqCst), 1);
    // The write is one undoable step named like the UI's.
    let engine = r.engine.lock().expect("engine lock");
    assert!(engine.can_undo());
    assert_eq!(engine.undo_name(), Some("Import CSV"));
}

#[test]
/// Covers: R-API-10
fn r_api_10_oversized_body_400() {
    let r = running("world");
    // Only the header is sent: the limit is decided from Content-Length alone.
    let raw = send_bytes(
        r.port(),
        b"POST /api/accounts/Checking/import HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 10485761\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(raw.status, 400);
    assert_eq!(raw.body, "Bad Request");
    assert_eq!(raw.content_type(), "text/plain; charset=utf-8");
}

#[test]
/// Covers: R-API-2
fn export_markdown_route() {
    let r = running("world");
    let raw = send_raw(
        r.port(),
        "GET /api/export/markdown HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(raw.status, 200);
    assert_eq!(raw.content_type(), "text/markdown; charset=utf-8");
    let expected = {
        let engine = r.engine.lock().expect("engine lock");
        export_markdown(&engine)
    };
    assert_eq!(raw.body, expected);
}

#[test]
/// Eight client threads, twenty GETs each, all answered within five seconds.
fn concurrent_requests_do_not_deadlock() {
    let r = running("world");
    let port = r.port();
    let started = Instant::now();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..20 {
                    let (status, _, _) = get(port, "/api/accounts/balances");
                    assert_eq!(status, 200);
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("worker finished");
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "160 requests took {:?}",
        started.elapsed()
    );
}
