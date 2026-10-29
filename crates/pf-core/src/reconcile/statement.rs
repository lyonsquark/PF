//! R-REC-1, R-REC-2: the statement file (JSON or CSV) and its parsers.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use rust_decimal::Decimal;
use serde_json::Value;

use crate::csv_import::{parse_csv, parse_csv_amount, parse_csv_date};

/// One statement, as read from a file. Amounts on lines are the holder's view (spent < 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    /// Bank name, "" when absent.
    pub institution: String,
    /// Account hint (e.g. last digits), "" when absent.
    pub account_hint: String,
    /// First day of the statement period.
    pub period_start: Date,
    /// Last day of the statement period.
    pub period_end: Date,
    /// Opening balance as printed (sign convention unknown).
    pub opening_balance: Decimal,
    /// Closing balance as printed.
    pub closing_balance: Decimal,
    /// Printed credits total, if any.
    pub total_credits: Option<Decimal>,
    /// Printed debits total, if any.
    pub total_debits: Option<Decimal>,
    /// The transaction lines.
    pub lines: Vec<StatementLine>,
}

/// One line of a statement.
#[derive(Debug, Clone, PartialEq)]
pub struct StatementLine {
    /// Transaction date.
    pub date: Date,
    /// Posting date, if shown.
    pub post_date: Option<Date>,
    /// Description as printed.
    pub description: String,
    /// Holder's view: spent/owed more < 0, received/owed less > 0.
    pub amount: Decimal,
    /// Heading on the statement, "" when absent.
    pub section: String,
    /// Reference number, "" when absent.
    pub reference: String,
}

/// A statement file that cannot be used.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StatementError {
    /// The message is shown to the owner word for word.
    #[error("{0}")]
    Invalid(String),
}

fn invalid<T>(msg: String) -> Result<T, StatementError> {
    Err(StatementError::Invalid(msg))
}

/// The text of a JSON string or number; `None` for other kinds.
fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn opt_text(obj: &serde_json::Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(scalar_text)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn json_date(field: &str, text: &str) -> Result<Date, StatementError> {
    match text.trim().parse::<Date>() {
        Ok(d) => Ok(d),
        Err(_) => invalid(format!("Invalid date in {field}: {text}")),
    }
}

fn json_amount(field: &str, text: &str) -> Result<Decimal, StatementError> {
    match parse_csv_amount(text) {
        Some(a) => Ok(a),
        None => invalid(format!("Invalid amount in {field}: {text}")),
    }
}

/// The required scalar `key` of `obj`, or the "missing" message built by `missing`.
fn required(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    missing: impl Fn(&str) -> String,
) -> Result<String, StatementError> {
    match obj.get(key).and_then(scalar_text) {
        Some(t) => Ok(t),
        None => invalid(missing(key)),
    }
}

fn optional_amount(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Decimal>, StatementError> {
    match obj.get(key).and_then(scalar_text) {
        Some(t) if !t.trim().is_empty() => json_amount(key, &t).map(Some),
        _ => Ok(None),
    }
}

fn check_period(start: Date, end: Date) -> Result<(), StatementError> {
    if end < start {
        return invalid("Statement period ends before it starts.".to_string());
    }
    Ok(())
}

/// R-REC-1: parses a statement JSON file.
pub fn parse_statement_json(text: &str) -> Result<Statement, StatementError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| StatementError::Invalid(format!("Statement is not valid JSON: {e}")))?;
    let Some(obj) = value.as_object() else {
        return invalid("Statement is not valid JSON: expected an object".to_string());
    };
    let missing = |name: &str| format!("Missing field: {name}");
    let start_text = required(obj, "period_start", missing)?;
    let end_text = required(obj, "period_end", missing)?;
    let opening_text = required(obj, "opening_balance", missing)?;
    let closing_text = required(obj, "closing_balance", missing)?;
    let Some(lines_json) = obj.get("lines").and_then(Value::as_array) else {
        return invalid(missing("lines"));
    };
    let period_start = json_date("period_start", &start_text)?;
    let period_end = json_date("period_end", &end_text)?;
    let opening_balance = json_amount("opening_balance", &opening_text)?;
    let closing_balance = json_amount("closing_balance", &closing_text)?;
    let total_credits = optional_amount(obj, "total_credits")?;
    let total_debits = optional_amount(obj, "total_debits")?;
    check_period(period_start, period_end)?;

    let mut lines = Vec::with_capacity(lines_json.len());
    for (i, item) in lines_json.iter().enumerate() {
        let n = i + 1;
        let empty = serde_json::Map::new();
        let line = item.as_object().unwrap_or(&empty);
        let line_missing = |name: &str| format!("Line {n}: missing field: {name}");
        let date_text = required(line, "date", line_missing)?;
        let amount_text = required(line, "amount", line_missing)?;
        let post_text = opt_text(line, "post_date");
        lines.push(StatementLine {
            date: json_date(&format!("line {n} date"), &date_text)?,
            post_date: if post_text.is_empty() {
                None
            } else {
                Some(json_date(&format!("line {n} post_date"), &post_text)?)
            },
            description: opt_text(line, "description"),
            amount: json_amount(&format!("line {n} amount"), &amount_text)?,
            section: opt_text(line, "section"),
            reference: opt_text(line, "reference"),
        });
    }
    Ok(Statement {
        institution: opt_text(obj, "institution"),
        account_hint: opt_text(obj, "account_hint"),
        period_start,
        period_end,
        opening_balance,
        closing_balance,
        total_credits,
        total_debits,
        lines,
    })
}

/// The first header position matching any of `names` (headers already lower-cased).
fn column(headers: &[String], names: &[&str]) -> Option<usize> {
    names
        .iter()
        .find_map(|name| headers.iter().position(|h| h == name))
}

/// R-REC-2: parses a statement CSV; the period and balances come from the caller.
pub fn parse_statement_csv(
    text: &str,
    period_start: Date,
    period_end: Date,
    opening: Decimal,
    closing: Decimal,
    tz: &TimeZone,
) -> Result<Statement, StatementError> {
    let parsed = parse_csv(text);
    let headers: Vec<String> = parsed.headers.iter().map(|h| h.to_lowercase()).collect();
    let (Some(date_col), Some(amount_col)) =
        (column(&headers, &["date"]), column(&headers, &["amount"]))
    else {
        return invalid("The CSV needs Date and Amount columns.".to_string());
    };
    let desc_col = column(&headers, &["description", "payee", "merchant", "name"]);
    let post_col = column(&headers, &["post date"]);
    let section_col = column(&headers, &["section"]);
    let ref_col = column(&headers, &["reference"]);
    check_period(period_start, period_end)?;

    let cell = |row: &[String], col: Option<usize>| -> String {
        col.and_then(|c| row.get(c)).cloned().unwrap_or_default()
    };
    let mut lines = Vec::with_capacity(parsed.rows.len());
    for (i, row) in parsed.rows.iter().enumerate() {
        let date = parse_csv_date(&cell(row, Some(date_col)), tz);
        let amount = parse_csv_amount(&cell(row, Some(amount_col)));
        let (Some(date), Some(amount)) = (date, amount) else {
            return invalid(format!("Line {}: invalid date or amount.", i + 1));
        };
        lines.push(StatementLine {
            date,
            post_date: post_col
                .and_then(|c| row.get(c))
                .and_then(|s| parse_csv_date(s, tz)),
            description: cell(row, desc_col),
            amount,
            section: cell(row, section_col),
            reference: cell(row, ref_col),
        });
    }
    Ok(Statement {
        institution: String::new(),
        account_hint: String::new(),
        period_start,
        period_end,
        opening_balance: opening,
        closing_balance: closing,
        total_credits: None,
        total_debits: None,
        lines,
    })
}
