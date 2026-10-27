//! Pure request to response routing for the localhost API (T12). No sockets: the server thread
//! calls `handle`, and the tests call it directly. Port of `APIServer.route`, `HTTPRequest.parse`
//! and `APIHandlers` (Swift `Services/`).

use std::collections::HashMap;
use std::sync::MutexGuard;

use pf_core::api_format::{
    Format, balance_entries, balance_entry, error_body, format_balances, format_import_result,
    format_scenarios, format_transactions, parse_format, resolve_scenario, transaction_lines,
};
use pf_engine::commands::backup::export_markdown;
use pf_engine::commands::import::import_csv_with_saved_mappings;
use pf_engine::{Engine, SharedEngine};
use pf_model::{Db, ScenarioId};

/// Transactions returned when `n` is missing or not an integer (R-API-7).
const DEFAULT_TX_COUNT: usize = 10;

/// Prefix of every account route (Swift `extractAccountName`).
const ACCOUNT_PREFIX: &str = "/api/accounts/";

/// One reply: status, `Content-Type` media type (the `; charset=utf-8` is added by the server) and
/// body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResponse {
    /// HTTP status code.
    pub status: u16,
    /// Media type, e.g. `application/json`.
    pub content_type: &'static str,
    /// Reply text.
    pub body: String,
}

impl ApiResponse {
    /// A reply with the given status, media type and body.
    fn new(status: u16, content_type: &'static str, body: String) -> ApiResponse {
        ApiResponse {
            status,
            content_type,
            body,
        }
    }

    /// The plain-text `404` for an account name that does not match (R-API-5).
    fn account_not_found(name: &str) -> ApiResponse {
        ApiResponse::new(404, "text/plain", format!("Account not found: {name}"))
    }
}

/// The reply to a body over the limit, or one that is not UTF-8 (R-API-10).
pub fn bad_request() -> ApiResponse {
    ApiResponse::new(400, "text/plain", "Bad Request".to_string())
}

/// Answers one request. `url` is the raw request target, e.g.
/// `/api/accounts/My%20Checking/transactions?n=5&format=text`. `changed` is set to true when the
/// request mutated data, so the server can request a UI repaint (R-API-9). The engine is locked
/// once, for the whole request.
pub fn handle(
    engine: &SharedEngine,
    method: &str,
    url: &str,
    body: &str,
    changed: &mut bool,
) -> ApiResponse {
    let (path, query) = split_target(url);
    let params = parse_query(query);
    // R-API-3: the raw format string decides the media type; the parsed one decides the body.
    let raw_format = params.get("format").map_or("json", String::as_str);
    let format = parse_format(raw_format);
    let content_type = json_or_text(raw_format);
    // Swift's error bodies test the raw string too, so `format=JSON` gets an "Error:" text body.
    let error_format = if raw_format == "json" {
        Format::Json
    } else {
        Format::Text
    };
    let scenario = params.get("scenario").map(String::as_str);
    let mut eng = lock(engine);
    let is_get = method == "GET";

    if is_get && path == "/api/db-path" {
        return ApiResponse::new(200, "text/plain", db_path_text(&eng));
    }
    if is_get && path == "/api/scenarios" {
        let body = format_scenarios(eng.db(), format);
        return ApiResponse::new(200, content_type, body);
    }
    if is_get && path == "/api/accounts/balances" {
        return balances_reply(&eng, scenario, format, error_format, content_type);
    }
    if is_get && let Some(name) = extract_account_name(path, "/balance") {
        return account_balance_reply(&eng, &name, scenario, format, error_format, content_type);
    }
    if is_get && let Some(name) = extract_account_name(path, "/transactions") {
        let n = params
            .get("n")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(DEFAULT_TX_COUNT);
        let args = TxArgs {
            name: &name,
            n,
            scenario,
            format,
            error_format,
            content_type,
        };
        return transactions_reply(&eng, &args);
    }
    if method == "POST"
        && let Some(name) = extract_account_name(path, "/import")
    {
        return import_reply(
            &mut eng,
            &name,
            body,
            format,
            error_format,
            content_type,
            changed,
        );
    }
    if is_get && path == "/api/export/markdown" {
        return ApiResponse::new(200, "text/markdown", export_markdown(&eng));
    }
    ApiResponse::new(404, "text/plain", format!("Not Found: {method} {path}"))
}

/// R-API-3: `application/json` only when the raw `format` is exactly `json` (case-sensitive).
fn json_or_text(raw_format: &str) -> &'static str {
    if raw_format == "json" {
        "application/json"
    } else {
        "text/plain"
    }
}

/// Locks the engine. A poisoned lock still holds consistent data, so its contents are used.
fn lock(engine: &SharedEngine) -> MutexGuard<'_, Engine> {
    match engine.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("engine mutex was poisoned by a panic; continuing with its data");
            poisoned.into_inner()
        }
    }
}

/// Splits a request target into path and query at the first `?`.
fn split_target(url: &str) -> (&str, &str) {
    url.split_once('?').unwrap_or((url, ""))
}

/// The SQLite path, or `Unknown` for an in-memory engine (T12 spec; Swift's text names SwiftData).
fn db_path_text(eng: &Engine) -> String {
    eng.db_path().map_or_else(
        || "Unknown".to_string(),
        |p| p.to_string_lossy().into_owned(),
    )
}

/// R-API-4: an unknown scenario is a 200 reply carrying the error body (Swift quirk).
fn scenario_or_error(
    db: &Db,
    name: Option<&str>,
    error_format: Format,
    content_type: &'static str,
) -> Result<Option<ScenarioId>, ApiResponse> {
    resolve_scenario(db, name)
        .map_err(|msg| ApiResponse::new(200, content_type, error_body(&msg, error_format)))
}

/// R-API-6: all balances, under the requested scenario.
fn balances_reply(
    eng: &Engine,
    scenario: Option<&str>,
    format: Format,
    error_format: Format,
    content_type: &'static str,
) -> ApiResponse {
    let sc = match scenario_or_error(eng.db(), scenario, error_format, content_type) {
        Ok(sc) => sc,
        Err(reply) => return reply,
    };
    let entries = balance_entries(eng.db(), sc);
    ApiResponse::new(200, content_type, format_balances(&entries, format))
}

/// R-API-6: one account's balance; the account is looked up before the scenario (Swift order).
fn account_balance_reply(
    eng: &Engine,
    name: &str,
    scenario: Option<&str>,
    format: Format,
    error_format: Format,
    content_type: &'static str,
) -> ApiResponse {
    let Some(id) = eng.db().account_by_name_ci(name).map(|a| a.id) else {
        return ApiResponse::account_not_found(name);
    };
    let sc = match scenario_or_error(eng.db(), scenario, error_format, content_type) {
        Ok(sc) => sc,
        Err(reply) => return reply,
    };
    let entries: Vec<_> = balance_entry(eng.db(), id, sc).into_iter().collect();
    ApiResponse::new(200, content_type, format_balances(&entries, format))
}

/// The query-derived arguments of the transactions route.
struct TxArgs<'a> {
    /// Account name as requested (matched case-insensitively).
    name: &'a str,
    /// Maximum number of rows.
    n: usize,
    /// Scenario name from the query, if any.
    scenario: Option<&'a str>,
    /// Body format.
    format: Format,
    /// Format used for error bodies (raw-string rule).
    error_format: Format,
    /// Media type of the reply.
    content_type: &'static str,
}

/// R-API-7: the newest `n` visible transactions of one account.
fn transactions_reply(eng: &Engine, args: &TxArgs<'_>) -> ApiResponse {
    let Some((id, stored_name)) = eng
        .db()
        .account_by_name_ci(args.name)
        .map(|a| (a.id, a.name.clone()))
    else {
        return ApiResponse::account_not_found(args.name);
    };
    let sc = match scenario_or_error(
        eng.db(),
        args.scenario,
        args.error_format,
        args.content_type,
    ) {
        Ok(sc) => sc,
        Err(reply) => return reply,
    };
    let lines = transaction_lines(eng.db(), id, sc, args.n);
    ApiResponse::new(
        200,
        args.content_type,
        format_transactions(&stored_name, &lines, args.format),
    )
}

/// R-API-8: imports the posted CSV with the account's saved mappings. Status 400 when the reply
/// contains "error" or "Error:" (Swift's string sniffing, kept on purpose).
fn import_reply(
    eng: &mut Engine,
    name: &str,
    csv: &str,
    format: Format,
    error_format: Format,
    content_type: &'static str,
    changed: &mut bool,
) -> ApiResponse {
    let body = match eng.db().account_by_name_ci(name).map(|a| a.id) {
        None => error_body(&format!("Account not found: {name}"), error_format),
        Some(id) => match import_csv_with_saved_mappings(eng, id, csv) {
            Ok(r) => {
                // R-API-9: only a real import is a change that needs a repaint.
                *changed = r.imported > 0;
                format_import_result(
                    name,
                    r.imported,
                    r.skipped_duplicates,
                    r.skipped_unmapped,
                    format,
                )
            }
            Err(e) => error_body(&e.to_string(), error_format),
        },
    };
    let status = if body.contains("error") || body.contains("Error:") {
        400
    } else {
        200
    };
    ApiResponse::new(status, content_type, body)
}

/// R-API-5: the account name in `/api/accounts/{name}{suffix}`, percent-decoded (raw when the
/// encoding is invalid). `None` when the path does not have that prefix and suffix.
pub fn extract_account_name(path: &str, suffix: &str) -> Option<String> {
    if !(path.starts_with(ACCOUNT_PREFIX) && path.ends_with(suffix)) {
        return None;
    }
    let rest = &path[ACCOUNT_PREFIX.len()..];
    // When the suffix overlaps the prefix, Swift's dropFirst/dropLast leaves an empty name.
    let middle = &rest[..rest.len().saturating_sub(suffix.len())];
    Some(percent_decode(middle).unwrap_or_else(|| middle.to_string()))
}

/// R-API-5: percent-decodes `s` as UTF-8. `None` for a bad `%` escape or non-UTF-8 bytes.
pub fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex_digit(*bytes.get(i + 1)?)?;
            let lo = hex_digit(*bytes.get(i + 2)?)?;
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The value of one ASCII hex digit.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// R-API-5: parses a query string like Swift's `HTTPRequest.parse`. Pairs are split on `&`, and
/// each on the first `=`. Pairs without `=`, and pairs with an empty key or value, are ignored
/// (Swift drops empty subsequences). `+` becomes a space before percent-decoding, and an invalid
/// escape keeps the raw text. A later duplicate key wins.
pub fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key.is_empty() || value.is_empty() {
            continue;
        }
        map.insert(decode_component(key), decode_component(value));
    }
    map
}

/// `+` to space, then percent-decode; raw text when the escape is invalid.
fn decode_component(raw: &str) -> String {
    let spaced = raw.replace('+', " ");
    percent_decode(&spaced).unwrap_or(spaced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    /// Covers: R-API-5
    fn r_api_5_parse_query_basics() {
        let q = parse_query("n=5&format=text");
        assert_eq!(q.get("n").map(String::as_str), Some("5"));
        assert_eq!(q.get("format").map(String::as_str), Some("text"));
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_parse_query_plus_and_percent() {
        let q = parse_query("scenario=Plan+A&name=My%20Checking&enc=%2B");
        assert_eq!(q["scenario"], "Plan A");
        assert_eq!(q["name"], "My Checking");
        assert_eq!(q["enc"], "+");
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_parse_query_ignores_pairs_without_equals() {
        let q = parse_query("flag&n=3&=x&empty=");
        assert_eq!(q.len(), 1);
        assert_eq!(q["n"], "3");
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_parse_query_later_duplicate_wins() {
        let q = parse_query("n=1&n=2");
        assert_eq!(q["n"], "2");
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_parse_query_keeps_invalid_escape_raw() {
        let q = parse_query("a=%zz&b=x%");
        assert_eq!(q["a"], "%zz");
        assert_eq!(q["b"], "x%");
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_parse_query_value_may_contain_equals() {
        let q = parse_query("a=b=c");
        assert_eq!(q["a"], "b=c");
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_extract_account_name_matches_suffix() {
        assert_eq!(
            extract_account_name("/api/accounts/Checking/balance", "/balance").as_deref(),
            Some("Checking")
        );
        assert_eq!(
            extract_account_name("/api/accounts/Checking/balance", "/transactions"),
            None
        );
        assert_eq!(
            extract_account_name("/other/Checking/balance", "/balance"),
            None
        );
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_extract_account_name_decodes_utf8() {
        assert_eq!(
            extract_account_name("/api/accounts/%E2%80%94/balance", "/balance").as_deref(),
            Some("\u{2014}")
        );
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_extract_account_name_bad_escape_is_raw() {
        assert_eq!(
            extract_account_name("/api/accounts/%zz/balance", "/balance").as_deref(),
            Some("%zz")
        );
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_extract_account_name_overlap_is_empty() {
        // Swift: "/api/accounts/balance" has the prefix and the suffix, sharing the slash.
        assert_eq!(
            extract_account_name("/api/accounts/balance", "/balance").as_deref(),
            Some("")
        );
    }

    #[test]
    /// Covers: R-API-5
    fn r_api_5_percent_decode_rules() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("%E2%80%94").as_deref(), Some("\u{2014}"));
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("100%"), None);
        assert_eq!(percent_decode("%FF"), None, "not UTF-8");
        assert_eq!(percent_decode("plain").as_deref(), Some("plain"));
    }

    #[test]
    /// Covers: R-API-3
    fn r_api_3_json_or_text_is_case_sensitive() {
        assert_eq!(json_or_text("json"), "application/json");
        assert_eq!(json_or_text("JSON"), "text/plain");
        assert_eq!(json_or_text(""), "text/plain");
    }

    #[test]
    /// Covers: R-API-3
    fn r_api_3_split_target_at_first_question_mark() {
        assert_eq!(split_target("/a?b=1?c"), ("/a", "b=1?c"));
        assert_eq!(split_target("/a"), ("/a", ""));
    }
}
