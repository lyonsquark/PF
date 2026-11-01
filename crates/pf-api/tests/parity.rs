//! Parity tests against the Swift app (T26, L6). `fixtures/parity/` holds the replies the running
//! Swift app gave for `fixtures/SampleData2.md` (see its README). Each manifest line is one request.
//! These tests replay every line against PF and compare the status, the content type and the body.
//! The captures come from `scripts/capture_swift_parity.sh`.

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

/// Markdown transaction rows ("| 2026-01-03T06:00:00Z | ...") with the same date may come in either order
/// (Swift does not order same-day rows, T26): each run of same-date rows is sorted, on both sides.
fn same_day_rows_sorted(text: &str) -> String {
    let date_of = |line: &str| -> Option<String> {
        line.starts_with("| 20")
            .then(|| line.split('|').nth(1).unwrap_or("").trim().to_string())
    };
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let mut run_date: Option<String> = None;
    for line in text.split('\n') {
        match (date_of(line), &run_date) {
            (Some(d), Some(r)) if &d == r => run.push(line.to_string()),
            (date, _) => {
                run.sort();
                out.append(&mut run);
                run_date = date;
                if run_date.is_some() {
                    run.push(line.to_string());
                } else {
                    out.push(line.to_string());
                }
            }
        }
    }
    run.sort();
    out.append(&mut run);
    out.join("\n")
}

/// The JSON transaction list with each run of same-date transactions compared as a set. Swift does not
/// order same-day rows, so their order is not compared, and a row's running balance depends on that order.
/// The first row of a run carries the balance after the whole day, which does not depend on the order, so
/// it is kept on the run's first row; the other rows lose their running balance.
fn same_day_transactions_sorted(v: &mut Value) {
    let Some(Value::Array(list)) = v.get_mut("transactions") else {
        return;
    };
    let date = |t: &Value| {
        t.get("date")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let mut start = 0;
    while start < list.len() {
        let day = date(&list[start]);
        let end = (start..list.len())
            .find(|&i| date(&list[i]) != day)
            .unwrap_or(list.len());
        let day_end = list[start].get("runningBalance").cloned();
        let run = &mut list[start..end];
        for t in run.iter_mut() {
            if let Some(obj) = t.as_object_mut() {
                obj.remove("runningBalance");
            }
        }
        run.sort_by_key(|t| t.to_string());
        if let (Some(first), Some(balance)) = (run.first_mut(), day_end) {
            first["runningBalance"] = balance;
        }
        start = end;
    }
}

/// The text and Markdown transaction lists with each run of same-date rows compared as a set, as
/// `same_day_transactions_sorted` does for JSON: the first row of a run keeps its running balance (the
/// balance after the whole day), the others lose it, and the rows are sorted. The balance is the third
/// field of a text row (`date  amount  balance ...`) and the fourth cell of a Markdown row.
fn same_day_text_rows(text: &str, markdown: bool) -> String {
    // (date, fields without the balance, balance) for a transaction row, else None.
    let parse = |line: &str| -> Option<(String, Vec<String>, String)> {
        let (date, mut fields, balance_at) = if markdown {
            let cells: Vec<String> = line
                .split('|')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect();
            (cells.first()?.clone(), cells.clone(), 3)
        } else {
            let fields: Vec<String> = line
                .split("  ")
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect();
            (fields.first()?.clone(), fields.clone(), 2)
        };
        let is_date = date.len() == 10
            && date.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            });
        if !is_date || fields.len() <= balance_at {
            return None;
        }
        let balance = fields.remove(balance_at);
        Some((date, fields, balance))
    };
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<(String, Vec<String>, String)> = Vec::new();
    let flush = |run: &mut Vec<(String, Vec<String>, String)>, out: &mut Vec<String>| {
        if run.is_empty() {
            return;
        }
        let day_end = run[0].2.clone();
        run.sort_by_key(|(_, fields, _)| fields.join("\u{1}"));
        for (i, (date, fields, _)) in run.drain(..).enumerate() {
            let balance = if i == 0 {
                day_end.clone()
            } else {
                String::new()
            };
            out.push(format!("{date}\u{1}{}\u{1}{balance}", fields.join("\u{1}")));
        }
    };
    for line in text.split('\n') {
        match parse(line) {
            Some(row) => {
                if run.last().is_some_and(|last| last.0 != row.0) {
                    flush(&mut run, &mut out);
                }
                run.push(row);
            }
            None => {
                flush(&mut run, &mut out);
                out.push(line.to_string());
            }
        }
    }
    flush(&mut run, &mut out);
    out.join("\n")
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
        let mut ours: Value = serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("{}: our body is not JSON: {e}", c.name));
        let mut theirs: Value = serde_json::from_str(&c.body)
            .unwrap_or_else(|e| panic!("{}: captured body is not JSON: {e}", c.name));
        same_day_transactions_sorted(&mut ours);
        same_day_transactions_sorted(&mut theirs);
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
            (
                same_day_rows_sorted(without_settings(&ours).trim_end()),
                same_day_rows_sorted(without_settings(&theirs).trim_end()),
            )
        } else if c.name.starts_with("tx_") {
            // Markdown captures start with a "## " heading; the text ones do not.
            let markdown = c.body.trim_start().starts_with("## ");
            (
                same_day_text_rows(&ours, markdown),
                same_day_text_rows(&theirs, markdown),
            )
        } else {
            (ours, theirs)
        };
        assert_eq!(ours, theirs, "{}: body differs", c.name);
    }
}

#[test]
/// Covers: R-API-2, R-API-3
fn parity_scenarios() {
    run_group("scenarios");
}

#[test]
/// Covers: R-API-4, R-API-6
fn parity_balances() {
    run_group("balances");
}

#[test]
/// Covers: R-API-5
fn parity_balance_single() {
    run_group("balance_single");
}

#[test]
/// Covers: R-API-7
fn parity_transactions() {
    run_group("transactions");
}

#[test]
/// Covers: R-API-2
fn parity_errors() {
    run_group("errors");
}

#[test]
/// Covers: R-MD-1
fn parity_export_markdown() {
    run_group("export_markdown");
}

#[test]
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
