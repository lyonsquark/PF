//! R-REC-1, R-REC-2: statement parsing.

use jiff::tz::TimeZone;
use pf_model::testing::{d, m};

use super::fixture;
use crate::reconcile::{parse_statement_csv, parse_statement_json};

fn err(text: &str) -> String {
    parse_statement_json(text)
        .expect_err("an error")
        .to_string()
}

const HEAD: &str = r#""period_start":"2026-02-01","period_end":"2026-02-28","opening_balance":"1","closing_balance":"2""#;

/// Covers: R-REC-1
#[test]
fn r_rec_1_parses_full_json() {
    let st = parse_statement_json(&fixture("checking.json")).expect("a value");
    assert_eq!(st.institution, "First Test Bank");
    assert_eq!(st.account_hint, "Checking ...1234");
    assert_eq!(st.opening_balance, m("1000.00"));
    assert_eq!(st.closing_balance, m("2873.31"));
    assert_eq!(st.total_credits, Some(m("2000.00")));
    assert_eq!(st.lines.len(), 5);
    assert_eq!(st.lines[2].amount, m("-84.20"));
    assert_eq!(st.lines[0].section, "Deposits");
    let card = parse_statement_json(&fixture("card-separate.json")).expect("a value");
    assert_eq!(card.lines[0].post_date, Some(d("2026-02-05")));
    assert_eq!(card.opening_balance, m("1764.30"));
    let extra = format!("{{{HEAD},\"lines\":[],\"unknown\":1}}");
    assert!(
        parse_statement_json(&extra)
            .expect("a value")
            .lines
            .is_empty()
    );
}

/// Covers: R-REC-1
#[test]
fn r_rec_1_error_messages() {
    assert!(err("{nope").starts_with("Statement is not valid JSON: "));
    assert_eq!(err("{}"), "Missing field: period_start");
    assert_eq!(
        err(
            r#"{"period_start":"2026-02-01","period_end":"2026-02-28","opening_balance":"1","lines":[]}"#
        ),
        "Missing field: closing_balance"
    );
    assert_eq!(err(&format!("{{{HEAD}}}")), "Missing field: lines");
    assert_eq!(
        err(&format!("{{{HEAD},\"lines\":[{{\"amount\":\"1\"}}]}}")),
        "Line 1: missing field: date"
    );
    assert_eq!(
        err(&format!(
            "{{{HEAD},\"lines\":[{{\"date\":\"2026-02-02\",\"amount\":\"1\"}},{{\"date\":\"2026-02-03\"}}]}}"
        )),
        "Line 2: missing field: amount"
    );
    assert_eq!(
        err(&format!(
            "{{{HEAD},\"lines\":[{{\"date\":\"02/02\",\"amount\":\"1\"}}]}}"
        )),
        "Invalid date in line 1 date: 02/02"
    );
    assert_eq!(
        err(&format!(
            "{{{HEAD},\"lines\":[{{\"date\":\"2026-02-02\",\"amount\":\"abc\"}}]}}"
        )),
        "Invalid amount in line 1 amount: abc"
    );
    assert_eq!(
        err(
            r#"{"period_start":"2026-02-01","period_end":"x","opening_balance":"1","closing_balance":"2","lines":[]}"#
        ),
        "Invalid date in period_end: x"
    );
    assert_eq!(
        err(
            r#"{"period_start":"2026-02-01","period_end":"2026-02-28","opening_balance":"z","closing_balance":"2","lines":[]}"#
        ),
        "Invalid amount in opening_balance: z"
    );
    assert_eq!(
        err(
            r#"{"period_start":"2026-03-01","period_end":"2026-02-28","opening_balance":"1","closing_balance":"2","lines":[]}"#
        ),
        "Statement period ends before it starts."
    );
}

fn csv(text: &str) -> Result<crate::reconcile::Statement, String> {
    let tz = TimeZone::get("America/Chicago").expect("a value");
    parse_statement_csv(
        text,
        d("2026-02-01"),
        d("2026-02-28"),
        m("100"),
        m("90"),
        &tz,
    )
    .map_err(|e| e.to_string())
}

/// Covers: R-REC-2
#[test]
fn r_rec_2_parses_csv() {
    let st = csv("Post Date,DATE,Merchant,Amount,Section,Reference\n02/03/2026,02/02/2026,COFFEE,\"-$1,010.50\",Purchases,R1\n").expect("a value");
    assert_eq!(st.lines.len(), 1);
    let l = &st.lines[0];
    assert_eq!(l.date, d("2026-02-02"));
    assert_eq!(l.post_date, Some(d("2026-02-03")));
    assert_eq!(l.description, "COFFEE");
    assert_eq!(l.amount, m("-1010.50"));
    assert_eq!(l.section, "Purchases");
    assert_eq!(l.reference, "R1");
    assert_eq!(st.opening_balance, m("100"));
    assert_eq!(st.closing_balance, m("90"));
}

/// Covers: R-REC-2
#[test]
fn r_rec_2_bad_row_is_error() {
    assert_eq!(
        csv("Date,Amount\n2026-02-02,-5\nnot a date,-5\n").expect_err("an error"),
        "Line 2: invalid date or amount."
    );
    assert_eq!(
        csv("Date,Amount\n2026-02-02,abc\n").expect_err("an error"),
        "Line 1: invalid date or amount."
    );
    assert_eq!(
        csv("Date,Memo\n2026-02-02,x\n").expect_err("an error"),
        "The CSV needs Date and Amount columns."
    );
}
