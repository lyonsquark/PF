//! R-API-13: the reconciliation report as JSON or text.

use pf_model::{Db, TransactionId};
use rust_decimal::Decimal;

use super::{Format, JsonValue, jstr, num, obj, pad, to_apple_json, ymd};
use crate::format::format_currency;
use crate::reconcile::{
    BalanceStyle, Choice, Confidence, Pair, Reading, Report, Statement, difference,
};

fn reading_name(r: Reading) -> &'static str {
    match r {
        Reading::AsPrinted => "asPrinted",
        Reading::Flipped => "flipped",
    }
}

fn style_name(s: BalanceStyle) -> &'static str {
    match s {
        BalanceStyle::Asset => "asset",
        BalanceStyle::Owed => "owed",
    }
}

fn confidence_name(c: Confidence) -> &'static str {
    match c {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::AmountDiffers => "amountDiffers",
    }
}

fn int(n: i64) -> JsonValue {
    JsonValue::Num(n as f64)
}

/// The statement line fields of an entry.
fn line_fields(st: &Statement, report: &Report, line: usize) -> Vec<(&'static str, JsonValue)> {
    let l = &st.lines[line];
    vec![
        ("line", int(line as i64)),
        ("date", jstr(&ymd(l.date))),
        ("description", jstr(&l.description)),
        ("amount", num(report.amounts[line])),
    ]
}

/// The transaction object of an entry.
fn tx_json(db: &Db, id: TransactionId) -> JsonValue {
    match db.transaction(id) {
        Some(t) => obj(vec![
            ("id", int(id.raw())),
            ("date", jstr(&ymd(t.date))),
            ("payee", jstr(&t.payee)),
            ("amount", num(t.amount)),
        ]),
        None => obj(vec![("id", int(id.raw()))]),
    }
}

fn pair_json(db: &Db, st: &Statement, report: &Report, p: &Pair) -> JsonValue {
    let mut f = line_fields(st, report, p.line);
    f.push(("transaction", tx_json(db, p.transaction)));
    f.push(("confidence", jstr(confidence_name(p.confidence))));
    f.push(("dayDistance", int(i64::from(p.day_distance))));
    obj(f)
}

/// R-API-13: the report in `f`. Markdown is the text form.
pub fn format_reconcile_report(
    account_name: &str,
    db: &Db,
    st: &Statement,
    report: &Report,
    f: Format,
) -> String {
    let diff = difference(report, &Choice::default_for(report));
    match f {
        Format::Json => {
            let arr = |v: Vec<JsonValue>| JsonValue::Arr(v);
            let pairs =
                |list: &[Pair]| arr(list.iter().map(|p| pair_json(db, st, report, p)).collect());
            let sections = obj(vec![
                ("matched", pairs(&report.matched)),
                ("amountDiffers", pairs(&report.amount_differs)),
                (
                    "statementOnly",
                    arr(report
                        .statement_only
                        .iter()
                        .map(|&i| obj(line_fields(st, report, i)))
                        .collect()),
                ),
                (
                    "pfOnlyInPeriod",
                    arr(report
                        .pf_only_in_period
                        .iter()
                        .map(|&t| tx_json(db, t))
                        .collect()),
                ),
                (
                    "pfOnlyOutside",
                    arr(report
                        .pf_only_outside
                        .iter()
                        .map(|&t| tx_json(db, t))
                        .collect()),
                ),
                (
                    "alreadyReconciled",
                    arr(report
                        .already_reconciled
                        .iter()
                        .map(|&(i, t)| {
                            let mut fields = line_fields(st, report, i);
                            fields.push(("transaction", tx_json(db, t)));
                            obj(fields)
                        })
                        .collect()),
                ),
            ]);
            let v = obj(vec![
                ("account", jstr(account_name)),
                (
                    "period",
                    obj(vec![
                        ("start", jstr(&ymd(st.period_start))),
                        ("end", jstr(&ymd(st.period_end))),
                    ]),
                ),
                ("reading", jstr(reading_name(report.reading))),
                ("style", jstr(style_name(report.style))),
                ("opening", num(report.opening)),
                ("closing", num(report.closing)),
                ("reconciledBalance", num(report.reconciled_balance)),
                ("openingDifference", num(report.opening_difference)),
                ("firstTime", JsonValue::Bool(report.first_time)),
                ("difference", num(diff)),
                ("sections", sections),
            ]);
            to_apple_json(&v, true, true)
        }
        Format::Text | Format::Markdown => text_report(account_name, db, st, report, diff),
    }
}

fn money(d: Decimal) -> String {
    pad(&format_currency(d), 12)
}

fn text_report(account_name: &str, db: &Db, st: &Statement, r: &Report, diff: Decimal) -> String {
    let n = match r.reading {
        Reading::AsPrinted => r.matches_as_printed,
        Reading::Flipped => r.matches_flipped,
    };
    let mut out = vec![
        format!(
            "{account_name} — statement {} to {}",
            ymd(st.period_start),
            ymd(st.period_end)
        ),
        format!(
            "Read as: {} balances, signs {} ({n} of {} lines match)",
            style_name(r.style),
            if r.reading == Reading::AsPrinted {
                "as printed"
            } else {
                "flipped"
            },
            st.lines.len()
        ),
        format!("Opening balance: {}", format_currency(r.opening)),
    ];
    if r.first_time {
        out.push("This is the first reconciliation for this account.".to_string());
    }
    if !r.opening_difference.is_zero() {
        out.push(format!(
            "PF's reconciled balance is {} (off by {}).",
            format_currency(r.reconciled_balance),
            format_currency(r.opening_difference)
        ));
    }
    out.push(format!("Closing balance: {}", format_currency(r.closing)));
    out.push(format!("Difference: {}", format_currency(diff)));
    let line_text = |i: usize| {
        let l = &st.lines[i];
        format!(
            "{}  {}  {}",
            ymd(l.date),
            money(r.amounts[i]),
            l.description
        )
    };
    let tx_text = |t: TransactionId| match db.transaction(t) {
        Some(x) => format!("{}  {}  {}", ymd(x.date), money(x.amount), x.payee),
        None => format!("(missing {})", t.raw()),
    };
    let mut section = |title: &str, rows: Vec<String>| {
        if !rows.is_empty() {
            out.push(String::new());
            out.push(format!("{title} ({})", rows.len()));
            out.extend(rows.into_iter().map(|r| format!("  {r}")));
        }
    };
    let pair_rows = |list: &[Pair]| -> Vec<String> {
        list.iter()
            .map(|p| format!("{}  <->  {}", line_text(p.line), tx_text(p.transaction)))
            .collect()
    };
    section("Matched", pair_rows(&r.matched));
    section("Amount differs", pair_rows(&r.amount_differs));
    section(
        "On statement only",
        r.statement_only.iter().map(|&i| line_text(i)).collect(),
    );
    section(
        "In PF only, in this period",
        r.pf_only_in_period.iter().map(|&t| tx_text(t)).collect(),
    );
    section(
        "Already reconciled",
        r.already_reconciled
            .iter()
            .map(|&(i, t)| format!("{}  <->  {}", line_text(i), tx_text(t)))
            .collect(),
    );
    section(
        "In PF only, outside this period",
        r.pf_only_outside.iter().map(|&t| tx_text(t)).collect(),
    );
    out.join("\n")
}
