//! Budget projector: the dates a budget item recurs on, and the text of projected transactions.

use jiff::Span;
use jiff::civil::Date;
use pf_model::{BudgetFrequency, BudgetItem, BudgetItemType};
use rust_decimal::Decimal;

/// R-PRJ-6: memo put on generated transactions.
pub const PROJECTION_MEMO: &str = "Auto-generated from Budget";

/// R-PRJ-1..5: all dates in [from, until] (inclusive) on which `item` recurs, ascending.
pub fn generate_dates(item: &BudgetItem, from: Date, until: Date) -> Vec<Date> {
    // R-PRJ-1: effective end is the earlier of until and the item's end date
    let effective_end = match item.end_date {
        Some(end) => until.min(end),
        None => until,
    };
    // R-PRJ-2: weekly steps of seven days from the start date
    if item.frequency == BudgetFrequency::Weekly {
        return weekly_dates(item.start_date, from, effective_end);
    }
    let Some(inc) = month_increment(item.frequency) else {
        return Vec::new();
    };

    let preferred_day = item.start_date.day();
    let start_year = i32::from(item.start_date.year());
    let start_month0 = i32::from(item.start_date.month()) - 1;
    let mut dates = Vec::new();
    let mut offset = 0;
    loop {
        // R-PRJ-3: candidate is computed from the start month plus offset, never from the previous one
        let total = start_year * 12 + start_month0 + offset;
        let (Ok(year), Ok(month)) = (
            i16::try_from(total.div_euclid(12)),
            i8::try_from(total.rem_euclid(12) + 1),
        ) else {
            break;
        };
        let Some(candidate) = clamped_date(year, month, preferred_day) else {
            break;
        };
        if candidate > effective_end {
            break;
        }
        if candidate >= from && candidate >= item.start_date && month_allowed(item, month) {
            dates.push(candidate);
        }
        offset += inc;
        // R-PRJ-5: safety stop after 1200 months of offset
        if offset > 1200 {
            break;
        }
    }
    dates
}

/// R-PRJ-6: signed amount of one projected occurrence: −|amount| for Bill/Expense, +|amount| for Income.
pub fn projected_amount(item: &BudgetItem) -> Decimal {
    let magnitude = item.amount.abs();
    match item.item_type {
        BudgetItemType::Bill | BudgetItemType::Expense => -magnitude,
        BudgetItemType::Income => magnitude,
    }
}

/// R-PRJ-6: payee text of a projected transaction: name for Bill and Income, "Expense: {name}" for Expense.
pub fn projected_payee(item: &BudgetItem) -> String {
    match item.item_type {
        BudgetItemType::Expense => format!("Expense: {}", item.name),
        BudgetItemType::Bill | BudgetItemType::Income => item.name.clone(),
    }
}

/// R-PRJ-3: the date in (year, month) with day `min(preferred_day, last day of that month)`.
fn clamped_date(year: i16, month: i8, preferred_day: i8) -> Option<Date> {
    let first = Date::new(year, month, 1).ok()?;
    let last = first.last_of_month().day();
    Date::new(year, month, preferred_day.min(last)).ok()
}

/// R-PRJ-2: dates from `start` stepping seven days while `<= end`, keeping those `>= from`.
fn weekly_dates(start: Date, from: Date, end: Date) -> Vec<Date> {
    let mut out = Vec::new();
    let mut cur = start;
    while cur <= end {
        if cur >= from {
            out.push(cur);
        }
        match cur.checked_add(Span::new().days(7)) {
            Ok(next) => cur = next,
            Err(_) => break,
        }
    }
    out
}

/// R-PRJ-3: months between candidates; None for weekly, which is not month-based.
fn month_increment(freq: BudgetFrequency) -> Option<i32> {
    match freq {
        BudgetFrequency::Weekly => None,
        BudgetFrequency::Monthly | BudgetFrequency::SpecificMonths => Some(1),
        BudgetFrequency::EveryOtherMonth => Some(2),
        BudgetFrequency::Quarterly => Some(3),
        BudgetFrequency::Yearly => Some(12),
    }
}

/// R-PRJ-4: only SpecificMonths items are filtered, and only to their listed months.
fn month_allowed(item: &BudgetItem, month: i8) -> bool {
    if item.frequency != BudgetFrequency::SpecificMonths {
        return true;
    }
    u32::try_from(month).is_ok_and(|mo| item.specific_months.contains(&mo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{d, m};
    use pf_model::{BudgetFrequency, BudgetItemId};
    use proptest::prelude::*;

    /// Builds a live item of the given type and frequency, with an optional end date and months.
    fn item(freq: BudgetFrequency, start: Date, end: Option<&str>, months: &[u32]) -> BudgetItem {
        let mut it = BudgetItem::new(
            BudgetItemId::from(1),
            BudgetItemType::Expense,
            "Test",
            m("50"),
            start,
            freq,
        );
        it.end_date = end.map(d);
        it.specific_months = months.to_vec();
        it
    }

    /// Dates as ISO strings, for compact assertions.
    fn strs(dates: &[Date]) -> Vec<String> {
        dates.iter().map(ToString::to_string).collect()
    }

    /// R-PRJ-1: end date caps the range.
    #[test]
    fn r_prj_1_end_date_caps_range() {
        let it = item(
            BudgetFrequency::Monthly,
            d("2026-01-15"),
            Some("2026-03-20"),
            &[],
        );
        let got = generate_dates(&it, d("2026-01-01"), d("2026-12-31"));
        assert_eq!(strs(&got), ["2026-01-15", "2026-02-15", "2026-03-15"]);
    }

    /// R-PRJ-1: when until is earlier than end date, until wins.
    #[test]
    fn r_prj_1_until_before_end_date() {
        let it = item(
            BudgetFrequency::Monthly,
            d("2026-01-15"),
            Some("2026-12-31"),
            &[],
        );
        let got = generate_dates(&it, d("2026-01-01"), d("2026-02-10"));
        assert_eq!(strs(&got), ["2026-01-15"]);
    }

    /// R-PRJ-1: from after until gives nothing.
    #[test]
    fn r_prj_1_from_after_until_empty() {
        let it = item(BudgetFrequency::Monthly, d("2026-01-15"), None, &[]);
        let got = generate_dates(&it, d("2026-06-01"), d("2026-05-01"));
        assert!(got.is_empty());
    }

    /// R-PRJ-2: weekly dates step seven days.
    #[test]
    fn r_prj_2_weekly_steps_seven_days() {
        let it = item(BudgetFrequency::Weekly, d("2026-01-01"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-01-31"));
        assert_eq!(
            strs(&got),
            [
                "2026-01-01",
                "2026-01-08",
                "2026-01-15",
                "2026-01-22",
                "2026-01-29"
            ]
        );
    }

    /// R-PRJ-2: weekly alignment comes from the start date, not from `from`.
    #[test]
    fn r_prj_2_weekly_skips_before_from() {
        let it = item(BudgetFrequency::Weekly, d("2026-01-01"), None, &[]);
        let got = generate_dates(&it, d("2026-01-10"), d("2026-01-31"));
        assert_eq!(strs(&got), ["2026-01-15", "2026-01-22", "2026-01-29"]);
    }

    /// R-PRJ-2: weekly with a start after until gives nothing.
    #[test]
    fn r_prj_2_weekly_start_after_until_empty() {
        let it = item(BudgetFrequency::Weekly, d("2026-02-01"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-01-31"));
        assert!(got.is_empty());
    }

    /// R-PRJ-3: Jan 31 clamps to Feb 28, then returns to Mar 31 (no drift).
    #[test]
    fn r_prj_3_jan31_clamps_to_feb28() {
        let it = item(BudgetFrequency::Monthly, d("2026-01-31"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-04-30"));
        assert_eq!(
            strs(&got),
            ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]
        );
    }

    /// R-PRJ-3: leap year February clamps to the 29th.
    #[test]
    fn r_prj_3_leap_year_feb29() {
        let it = item(BudgetFrequency::Monthly, d("2028-01-31"), None, &[]);
        let got = generate_dates(&it, d("2028-01-01"), d("2028-02-29"));
        assert_eq!(strs(&got), ["2028-01-31", "2028-02-29"]);
    }

    /// R-PRJ-3: every other month steps two months.
    #[test]
    fn r_prj_3_every_other_month() {
        let it = item(BudgetFrequency::EveryOtherMonth, d("2026-01-10"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-12-31"));
        assert_eq!(
            strs(&got),
            [
                "2026-01-10",
                "2026-03-10",
                "2026-05-10",
                "2026-07-10",
                "2026-09-10",
                "2026-11-10"
            ]
        );
    }

    /// R-PRJ-3: quarterly from a mid-year start, with the range starting after the first occurrence.
    #[test]
    fn r_prj_3_quarterly_from_mid_year() {
        let it = item(BudgetFrequency::Quarterly, d("2025-11-05"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-12-31"));
        assert_eq!(
            strs(&got),
            ["2026-02-05", "2026-05-05", "2026-08-05", "2026-11-05"]
        );
    }

    /// R-PRJ-3: yearly from Feb 29 clamps to Feb 28 in non-leap years.
    #[test]
    fn r_prj_3_yearly_across_years() {
        let it = item(BudgetFrequency::Yearly, d("2024-02-29"), None, &[]);
        let got = generate_dates(&it, d("2025-01-01"), d("2028-12-31"));
        assert_eq!(
            strs(&got),
            ["2025-02-28", "2026-02-28", "2027-02-28", "2028-02-29"]
        );
    }

    /// R-PRJ-3: nothing is emitted before the start date, even when from is earlier.
    #[test]
    fn r_prj_3_from_before_start_date() {
        let it = item(BudgetFrequency::Monthly, d("2026-03-10"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-06-30"));
        assert_eq!(
            strs(&got),
            ["2026-03-10", "2026-04-10", "2026-05-10", "2026-06-10"]
        );
    }

    /// R-PRJ-3: occurrences exactly on from and exactly on until are both included.
    #[test]
    fn r_prj_3_inclusive_bounds() {
        let it = item(BudgetFrequency::Monthly, d("2026-01-15"), None, &[]);
        let got = generate_dates(&it, d("2026-02-15"), d("2026-03-15"));
        assert_eq!(strs(&got), ["2026-02-15", "2026-03-15"]);
    }

    /// R-PRJ-4: specific months keep only the listed months.
    #[test]
    fn r_prj_4_specific_months_filter() {
        let it = item(
            BudgetFrequency::SpecificMonths,
            d("2026-01-15"),
            None,
            &[1, 4, 7, 10],
        );
        let got = generate_dates(&it, d("2026-01-01"), d("2026-12-31"));
        assert_eq!(
            strs(&got),
            ["2026-01-15", "2026-04-15", "2026-07-15", "2026-10-15"]
        );
    }

    /// R-PRJ-4: an empty month list emits nothing.
    #[test]
    fn r_prj_4_specific_months_empty_list() {
        let it = item(BudgetFrequency::SpecificMonths, d("2026-01-15"), None, &[]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-12-31"));
        assert!(got.is_empty());
    }

    /// R-PRJ-4: a specific month still clamps the day to that month's length.
    #[test]
    fn r_prj_4_specific_months_clamps() {
        let it = item(BudgetFrequency::SpecificMonths, d("2026-01-31"), None, &[2]);
        let got = generate_dates(&it, d("2026-01-01"), d("2026-12-31"));
        assert_eq!(strs(&got), ["2026-02-28"]);
    }

    /// R-PRJ-5: offsets 0..=1200 are evaluated (1201 dates), the safety stop then ends the loop.
    #[test]
    fn r_prj_5_safety_stop_1200_months() {
        let it = item(BudgetFrequency::Monthly, d("2000-01-01"), None, &[]);
        let got = generate_dates(&it, d("2000-01-01"), d("2200-01-01"));
        assert_eq!(got.len(), 1201);
        assert_eq!(
            got.last().map(ToString::to_string),
            Some("2100-01-01".to_string())
        );
    }

    /// R-PRJ-6: signed amounts: bills and expenses negative, income positive.
    #[test]
    fn r_prj_6_projected_amount_signs() {
        let mut bill = item(BudgetFrequency::Monthly, d("2026-01-01"), None, &[]);
        bill.item_type = BudgetItemType::Bill;
        bill.amount = m("100");
        assert_eq!(projected_amount(&bill), m("-100"));

        let mut expense = item(BudgetFrequency::Monthly, d("2026-01-01"), None, &[]);
        expense.item_type = BudgetItemType::Expense;
        expense.amount = m("-50");
        assert_eq!(projected_amount(&expense), m("-50"));

        let mut income = item(BudgetFrequency::Monthly, d("2026-01-01"), None, &[]);
        income.item_type = BudgetItemType::Income;
        income.amount = m("-20");
        assert_eq!(projected_amount(&income), m("20"));
    }

    /// R-PRJ-6: payee text: bill and income use the name, expenses get an "Expense: " prefix.
    #[test]
    fn r_prj_6_projected_payee_text() {
        let mut bill = item(BudgetFrequency::Monthly, d("2026-01-01"), None, &[]);
        bill.item_type = BudgetItemType::Bill;
        bill.name = "Rent".to_string();
        assert_eq!(projected_payee(&bill), "Rent");

        let mut income = item(BudgetFrequency::Monthly, d("2026-01-01"), None, &[]);
        income.item_type = BudgetItemType::Income;
        income.name = "Salary".to_string();
        assert_eq!(projected_payee(&income), "Salary");

        let mut expense = item(BudgetFrequency::Monthly, d("2026-01-01"), None, &[]);
        expense.item_type = BudgetItemType::Expense;
        expense.name = "Groceries".to_string();
        assert_eq!(projected_payee(&expense), "Expense: Groceries");
    }

    /// Frequencies in the order they are tested by the property strategies.
    const FREQS: [BudgetFrequency; 6] = [
        BudgetFrequency::Weekly,
        BudgetFrequency::Monthly,
        BudgetFrequency::EveryOtherMonth,
        BudgetFrequency::Quarterly,
        BudgetFrequency::Yearly,
        BudgetFrequency::SpecificMonths,
    ];

    /// Calendar date from the property ranges (day 1..=28 always valid).
    fn ymd(y: i16, mo: i8, dd: i8) -> Date {
        Date::new(y, mo, dd).expect("valid by construction")
    }

    /// Any date for from/until: years 2000..=2040, day 1..=28.
    fn any_date() -> impl Strategy<Value = Date> {
        (2000i16..=2040, 1i8..=12, 1i8..=28).prop_map(|(y, mo, dd)| ymd(y, mo, dd))
    }

    /// Start date: any day 1..=31, clamped to the month length.
    fn any_start() -> impl Strategy<Value = Date> {
        (2000i16..=2040, 1i8..=12, 1i8..=31)
            .prop_map(|(y, mo, dd)| clamped_date(y, mo, dd).expect("clamped day is always valid"))
    }

    /// Any non-weekly-or-weekly item with random frequency, optional end date and month list.
    fn any_item() -> impl Strategy<Value = BudgetItem> {
        (
            prop::sample::select(FREQS.to_vec()),
            any_start(),
            prop::option::of(any_date()),
            prop::collection::vec(1u32..=12, 0..=12),
        )
            .prop_map(|(freq, start, end, months)| {
                let mut it = item(freq, start, None, &[]);
                it.end_date = end;
                it.specific_months = months;
                it
            })
    }

    /// Month increment per frequency (R-PRJ-3), or None for weekly.
    fn month_increment(freq: BudgetFrequency) -> Option<i64> {
        match freq {
            BudgetFrequency::Weekly => None,
            BudgetFrequency::Monthly | BudgetFrequency::SpecificMonths => Some(1),
            BudgetFrequency::EveryOtherMonth => Some(2),
            BudgetFrequency::Quarterly => Some(3),
            BudgetFrequency::Yearly => Some(12),
        }
    }

    proptest! {
        /// Covers: R-PRJ-1
        #[test]
        fn prop_dates_sorted_and_in_range(item in any_item(), from in any_date(), until in any_date()) {
            let dates = generate_dates(&item, from, until);
            let lo = from.max(item.start_date);
            let hi = match item.end_date {
                Some(end) => until.min(end),
                None => until,
            };
            for pair in dates.windows(2) {
                prop_assert!(pair[0] < pair[1]);
            }
            for date in &dates {
                prop_assert!(*date >= lo && *date <= hi);
            }
        }

        /// Covers: R-PRJ-3
        #[test]
        fn prop_month_based_day_is_clamped_start_day(item in any_item(), from in any_date(), until in any_date()) {
            prop_assume!(item.frequency != BudgetFrequency::Weekly);
            for date in generate_dates(&item, from, until) {
                let last = date.last_of_month().day();
                prop_assert_eq!(date.day(), item.start_date.day().min(last));
            }
        }

        /// Covers: R-PRJ-2
        #[test]
        fn prop_weekly_diffs_are_seven(item in any_item(), from in any_date(), until in any_date()) {
            let mut item = item;
            item.frequency = BudgetFrequency::Weekly;
            let dates = generate_dates(&item, from, until);
            for pair in dates.windows(2) {
                prop_assert_eq!(pair[0].checked_add(jiff::Span::new().days(7)).ok(), Some(pair[1]));
            }
        }

        /// Covers: R-PRJ-4
        #[test]
        fn prop_specific_months_subset(item in any_item(), from in any_date(), until in any_date()) {
            let mut item = item;
            item.frequency = BudgetFrequency::SpecificMonths;
            for date in generate_dates(&item, from, until) {
                let month = u32::try_from(date.month()).expect("months are positive");
                prop_assert!(item.specific_months.contains(&month));
            }
        }

        /// Covers: R-PRJ-3, R-PRJ-5
        #[test]
        fn prop_count_bounded_by_month_span(item in any_item(), from in any_date(), until in any_date()) {
            let Some(inc) = month_increment(item.frequency) else {
                return Ok(());
            };
            let end = match item.end_date {
                Some(end) => until.min(end),
                None => until,
            };
            let index = |date: Date| i64::from(date.year()) * 12 + i64::from(date.month());
            let span = (index(end) - index(item.start_date)).max(0);
            let count = i64::try_from(generate_dates(&item, from, until).len()).unwrap_or(i64::MAX);
            prop_assert!(count <= span / inc + 1);
        }
    }
}
