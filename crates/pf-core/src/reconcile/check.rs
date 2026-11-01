//! R-REC-3 … R-REC-6: does the statement add up, and how are its signs to be read.

use pf_model::AccountType;
use rust_decimal::Decimal;

use super::statement::{Statement, StatementLine};
use crate::format::format_currency;

/// How the printed balances count: an asset grows with credits, a debt (owed) shrinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceStyle {
    /// opening + Σ lines = closing.
    Asset,
    /// opening − Σ lines = closing.
    Owed,
}

/// Whether line amounts are used as printed or negated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Lines as in the file.
    AsPrinted,
    /// Every line amount negated.
    Flipped,
}

/// R-REC-5: the style that fits an account type (Credit or Loan ⇒ Owed, else Asset).
pub fn style_for_account(t: AccountType) -> BalanceStyle {
    match t {
        AccountType::Credit | AccountType::Loan => BalanceStyle::Owed,
        _ => BalanceStyle::Asset,
    }
}

fn other(style: BalanceStyle) -> BalanceStyle {
    match style {
        BalanceStyle::Asset => BalanceStyle::Owed,
        BalanceStyle::Owed => BalanceStyle::Asset,
    }
}

fn adds_up(opening: Decimal, closing: Decimal, sum: Decimal) -> Option<BalanceStyle> {
    if opening + sum == closing {
        Some(BalanceStyle::Asset)
    } else if opening - sum == closing {
        Some(BalanceStyle::Owed)
    } else {
        None
    }
}

/// R-REC-3 + R-REC-5 with the account type unknown (S = 0 then falls back to Asset).
pub fn self_check(st: &Statement) -> Result<[(Reading, BalanceStyle); 2], String> {
    self_check_for(st, AccountType::Checking)
}

/// R-REC-3 + R-REC-5: both readings with their styles, or the "does not add up" message.
pub fn self_check_for(
    st: &Statement,
    account_type: AccountType,
) -> Result<[(Reading, BalanceStyle); 2], String> {
    let (o, c) = (st.opening_balance, st.closing_balance);
    let sum: Decimal = st.lines.iter().map(|l| l.amount).sum();
    if sum.is_zero() && o == c {
        let style = style_for_account(account_type);
        return Ok([(Reading::AsPrinted, style), (Reading::Flipped, style)]);
    }
    if let Some(style) = adds_up(o, c, sum) {
        return Ok([
            (Reading::AsPrinted, style),
            (Reading::Flipped, other(style)),
        ]);
    }
    let off = (o + sum - c).abs().min((o - sum - c).abs());
    let mut msg = format!(
        "Statement does not add up: opening {}, lines {}, closing {}. Off by {}.",
        format_currency(o),
        format_currency(sum),
        format_currency(c),
        format_currency(off)
    );
    if let Some(section) = fixing_section(st, sum) {
        msg.push_str(&format!(
            " Flipping the signs under '{section}' would make it add up."
        ));
    }
    Err(msg)
}

/// The first non-empty section whose negation makes the statement add up.
fn fixing_section(st: &Statement, sum: Decimal) -> Option<String> {
    let mut seen: Vec<&str> = Vec::new();
    for line in &st.lines {
        let s = line.section.as_str();
        if s.is_empty() || seen.contains(&s) {
            continue;
        }
        seen.push(s);
        let in_section: Decimal = st
            .lines
            .iter()
            .filter(|l| l.section == s)
            .map(|l| l.amount)
            .sum();
        let flipped = sum - in_section - in_section;
        if adds_up(st.opening_balance, st.closing_balance, flipped).is_some() {
            return Some(s.to_string());
        }
    }
    None
}

/// R-REC-4: printed totals against the lines, in either order.
pub fn check_totals(st: &Statement) -> Result<(), String> {
    let (Some(credits), Some(debits)) = (st.total_credits, st.total_debits) else {
        return Ok(());
    };
    let pos: Decimal = st
        .lines
        .iter()
        .filter(|l| l.amount > Decimal::ZERO)
        .map(|l| l.amount)
        .sum();
    let neg: Decimal = st
        .lines
        .iter()
        .filter(|l| l.amount < Decimal::ZERO)
        .map(|l| -l.amount)
        .sum();
    if (pos == credits && neg == debits) || (pos == debits && neg == credits) {
        return Ok(());
    }
    Err(format!(
        "Statement totals do not match: credits {} and debits {} listed, lines give {} and {}.",
        format_currency(credits),
        format_currency(debits),
        format_currency(pos),
        format_currency(neg)
    ))
}

/// A line's amount under a reading.
pub fn line_amount(line: &StatementLine, reading: Reading) -> Decimal {
    match reading {
        Reading::AsPrinted => line.amount,
        Reading::Flipped => -line.amount,
    }
}

/// R-REC-6: a printed balance in PF's sign (owed balances are negative).
pub fn pf_balance(printed: Decimal, style: BalanceStyle) -> Decimal {
    match style {
        BalanceStyle::Asset => printed,
        BalanceStyle::Owed => -printed,
    }
}
