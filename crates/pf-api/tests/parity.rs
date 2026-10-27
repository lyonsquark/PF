//! Parity tests against the Swift app (T26, L6). `fixtures/parity/` holds the replies the running
//! Swift app gave for `fixtures/SampleData2.md` (see its README). Each manifest line is one request.
//! These tests replay every line against PFRust and compare the status, the content type and the body.
//! They are `#[ignore]` until the captures exist; `scripts/capture_swift_parity.sh` makes them.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use common::get;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use pf_api::start;
use pf_engine::Engine;
use pf_engine::commands::backup::import_markdown_replacing;
use pretty_assertions::assert_eq;
use serde_json::Value;

/// One manifest line and its captured body.
#[derive(Debug, Clone)]
struct Capture {
    name: String,
    path: String,
    status: u16,
    content_type: String,
    body: String,
}

/// The `fixtures` directory at the repository root.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The `fixtures/parity` directory.
fn parity_dir() -> PathBuf {
    fixtures_dir().join("parity")
}

/// Reads `manifest.tsv` and every body it names. Panics with the T26 message when the captures
/// are missing.
fn load_captures() -> Vec<Capture> {
    let Ok(manifest) = std::fs::read_to_string(parity_dir().join("manifest.tsv")) else {
        panic!("parity captures missing; run scripts/capture_swift_parity.sh");
    };
    manifest
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 5, "manifest line has 5 columns: {line}");
            let name = cols[0].to_string();
            let body_path = parity_dir().join(format!("{name}.body"));
            let body = std::fs::read_to_string(&body_path)
                .unwrap_or_else(|e| panic!("capture {name}: {e}"));
            Capture {
                status: cols[3].parse().expect("status code in manifest"),
                path: cols[2].to_string(),
                content_type: cols[4].to_string(),
                name,
                body,
            }
        })
        .collect()
}

/// The test that replays a capture, by its name. `None` means no test covers it.
fn group_of(name: &str) -> Option<&'static str> {
    if name.starts_with("scenarios_") {
        Some("scenarios")
    } else if name.starts_with("balances_") {
        Some("balances")
    } else if name.starts_with("balance_") {
        Some("balance_single")
    } else if name.starts_with("tx_") {
        Some("transactions")
    } else if name == "unknown_route" {
        Some("errors")
    } else if name == "export_markdown" {
        Some("export_markdown")
    } else {
        None
    }
}

/// The capture date and time zone from `meta.txt`.
struct Meta {
    captured: Date,
    tz: TimeZone,
}

/// Reads `meta.txt` (`key=value` lines).
fn load_meta() -> Meta {
    let text = std::fs::read_to_string(parity_dir().join("meta.txt")).expect("meta.txt");
    let map: BTreeMap<&str, &str> = text.lines().filter_map(|l| l.split_once('=')).collect();
    let captured = map
        .get("captured")
        .expect("captured= in meta.txt")
        .parse()
        .expect("captured date");
    let tz = TimeZone::get(map.get("tz").expect("tz= in meta.txt")).expect("tz in the system tzdb");
    Meta { captured, tz }
}

/// Loads `fixtures/SampleData2.md` into a temp-file engine whose clock is the capture date and whose
/// time zone is the capture's, then runs `check` against the API on a free port.
fn with_api(check: impl FnOnce(u16)) {
    let meta = load_meta();
    let dir = tempfile::TempDir::new().expect("temp dir");
    let mut engine = Engine::open(&dir.path().join("pf.sqlite")).expect("open engine");
    let captured = meta.captured;
    let noon = captured
        .at(12, 0, 0, 0)
        .to_zoned(meta.tz.clone())
        .expect("noon exists on the capture date")
        .timestamp();
    engine.set_clock(Box::new(move || captured));
    engine.set_now_clock(Box::new(move || noon));
    engine.set_tz(meta.tz.clone());
    let text =
        std::fs::read_to_string(fixtures_dir().join("SampleData2.md")).expect("SampleData2.md");
    import_markdown_replacing(&mut engine, &text, &mut |_| {}).expect("import SampleData2.md");
    let handle = start(engine.shared(), 0, Arc::new(|| {})).expect("start the API on port 0");
    check(handle.port);
    drop(dir);
}

/// Replays the captures of one group against the API.
fn run_group(group: &str) {
    let captures: Vec<Capture> = load_captures()
        .into_iter()
        .filter(|c| group_of(&c.name) == Some(group))
        .collect();
    assert!(!captures.is_empty(), "no manifest lines for {group}");
    with_api(|port| {
        for c in &captures {
            check_capture(port, c);
        }
    });
}

/// Equal as JSON values, with numbers within 0.005 of each other (Swift converts Decimal to Double).
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

/// The content type with case and spaces removed, for comparison.
fn normal_type(content_type: &str) -> String {
    content_type.to_ascii_lowercase().replace(' ', "")
}

/// Line endings as `\n`.
fn normal_lines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The text without its `## Settings` section (the bookmark values differ between the apps).
fn without_settings(text: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with("## ") {
            skipping = line.trim_end() == "## Settings";
        }
        if !skipping {
            out.push_str(line);
        }
    }
    out
}

/// Checks one capture against the live reply. A mismatch names the manifest line.
fn check_capture(port: u16, c: &Capture) {
    let (status, content_type, body) = get(port, &c.path);
    assert_eq!(status, c.status, "{}: status for {}", c.name, c.path);
    assert_eq!(
        normal_type(&content_type),
        normal_type(&c.content_type),
        "{}: content type",
        c.name
    );
    if normal_type(&c.content_type).contains("json") {
        let ours: Value = serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("{}: our body is not JSON: {e}", c.name));
        let theirs: Value = serde_json::from_str(&c.body)
            .unwrap_or_else(|e| panic!("{}: captured body is not JSON: {e}", c.name));
        if !same_json(&ours, &theirs) {
            assert_eq!(
                serde_json::to_string_pretty(&ours).expect("pretty JSON"),
                serde_json::to_string_pretty(&theirs).expect("pretty JSON"),
                "{}: JSON body differs",
                c.name
            );
            panic!("{}: JSON body differs", c.name);
        }
    } else {
        let (ours, theirs) = (normal_lines(&body), normal_lines(&c.body));
        let (ours, theirs) = if c.name == "export_markdown" {
            (without_settings(&ours), without_settings(&theirs))
        } else {
            (ours, theirs)
        };
        assert_eq!(ours, theirs, "{}: body differs", c.name);
    }
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Covers: R-API-2, R-API-3
fn parity_scenarios() {
    run_group("scenarios");
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Covers: R-API-4, R-API-6
fn parity_balances() {
    run_group("balances");
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Covers: R-API-5
fn parity_balance_single() {
    run_group("balance_single");
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Covers: R-API-7
fn parity_transactions() {
    run_group("transactions");
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Covers: R-API-2
fn parity_errors() {
    run_group("errors");
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Covers: R-MD-1
fn parity_export_markdown() {
    run_group("export_markdown");
}

#[test]
#[ignore = "needs captures from the Swift app (T26)"]
/// Every manifest line is covered by a test group, and every `.body` file is listed in the manifest.
fn parity_manifest_complete() {
    let captures = load_captures();
    for c in &captures {
        assert!(
            group_of(&c.name).is_some(),
            "{}: no parity test covers this manifest line",
            c.name
        );
    }
    let listed: BTreeSet<String> = captures
        .iter()
        .map(|c| format!("{}.body", c.name))
        .collect();
    let on_disk: BTreeSet<String> = std::fs::read_dir(parity_dir())
        .expect("read fixtures/parity")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".body"))
        .collect();
    assert_eq!(
        on_disk, listed,
        "every .body file is listed in manifest.tsv"
    );
}
