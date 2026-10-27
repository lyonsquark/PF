//! R-CSV-1: line and field splitting.

use proptest::prelude::*;

use super::{fixture, strings};
use crate::csv_import::{parse_csv, split_csv_line};

#[test]
fn r_csv_1_split_respects_quotes() {
    assert_eq!(split_csv_line(r#"a,"b,c",d"#), strings(&["a", "b,c", "d"]));
    assert_eq!(
        split_csv_line(r#" a , "b,c" , d "#),
        strings(&["a", "b,c", "d"])
    );
    assert_eq!(split_csv_line(r#""""#), strings(&[""]));
    assert_eq!(split_csv_line(""), strings(&[""]));
    assert_eq!(split_csv_line("a,,b"), strings(&["a", "", "b"]));
}

#[test]
fn r_csv_1_escaped_quotes_not_supported() {
    assert_eq!(
        split_csv_line(r#""say ""hi""",x"#),
        strings(&["say hi", "x"])
    );
}

#[test]
fn r_csv_1_parse_drops_blank_lines() {
    let text = fixture("bank_basic.csv");
    let parsed = parse_csv(&text);
    assert_eq!(parsed.headers.len(), 5);
    assert_eq!(parsed.rows.len(), 6);
    assert_eq!(
        parsed.rows[0],
        strings(&[
            "03/01/2026",
            "STARBUCKS STORE 123, SEATTLE",
            "-6.75",
            "DEBIT",
            "1000.00"
        ])
    );
    assert_eq!(parse_csv(&text.replace('\n', "\r\n")), parsed);
}

proptest! {
    /// Covers: R-CSV-1
    #[test]
    fn r_csv_1_split_property(line in "\\PC*") {
        let fields = split_csv_line(&line);
        prop_assert!(!fields.is_empty());
        if !line.contains('"') {
            let expected: Vec<String> = line.split(',').map(|f| f.trim().to_string()).collect();
            prop_assert_eq!(fields, expected);
        }
    }
}
