# T02: pf-core budget projector

**Phase:** 1 · **Deps:** T01 · **Crate(s):** `pf-core` · **Size:** small (about 150 lines of code, 300 of tests)

## Goal

Port `BudgetProjector.generateDates`: given a budget item and a date range, list the dates
the item recurs on. Reports (T08), charts (T08b) and projection generation (T10c) all use it.
Also add the two small pure helpers projection needs (signed amount and payee text).

## Read first

- Swift: `../PFApp/Sources/PFApp/Models/BudgetProjector.swift` (all of it, about 96 lines)
- Swift: `../PFApp/Sources/PFApp/Views/BudgetProjectionView.swift` → `generateProjection` (amount and payee text)
- Rules: R-PRJ-1, R-PRJ-2, R-PRJ-3, R-PRJ-4, R-PRJ-5, R-PRJ-6 (amount/payee part only)
- `docs/testing.md` → L1 property tests (projector bullet)

## Deliverables

```rust
// crates/pf-core/src/lib.rs
pub mod projector;

// crates/pf-core/src/projector.rs
use jiff::civil::Date;
use pf_model::{BudgetItem, BudgetItemType};
use rust_decimal::Decimal;

/// R-PRJ-1..5: all dates in [from, until] (inclusive) on which `item` recurs, ascending.
pub fn generate_dates(item: &BudgetItem, from: Date, until: Date) -> Vec<Date>;

/// R-PRJ-6: signed amount of one projected occurrence: −|amount| for Bill/Expense, +|amount| for Income.
pub fn projected_amount(item: &BudgetItem) -> Decimal;

/// R-PRJ-6: payee text of a projected transaction: name for Bill and Income, "Expense: {name}" for Expense.
pub fn projected_payee(item: &BudgetItem) -> String;

/// R-PRJ-6: memo put on generated transactions.
pub const PROJECTION_MEMO: &str = "Auto-generated from Budget";

/// R-PRJ-3: the date in (year, month) with day `min(preferred_day, last day of that month)`.
fn clamped_date(year: i16, month: i8, preferred_day: i8) -> Option<Date>;
```

## Algorithm (port exactly)

```
effective_end = match item.end_date { Some(e) => min(until, e), None => until }      // R-PRJ-1

if item.frequency == Weekly:                                                          // R-PRJ-2
    cur = item.start_date
    while cur <= effective_end:
        if cur >= from: push(cur)
        cur = cur + 7 days            (stop if the add overflows)
    return

inc = Monthly 1 | EveryOtherMonth 2 | Quarterly 3 | Yearly 12 | SpecificMonths 1     // R-PRJ-3
preferred_day = item.start_date.day()
offset = 0
loop:
    total = start.year*12 + (start.month-1) + offset                                  // month arithmetic in i32
    (y, m) = (total div 12, total mod 12 + 1)
    candidate = clamped_date(y, m, preferred_day)   else break
    if candidate > effective_end: break
    if candidate >= from && candidate >= item.start_date:
        if frequency == SpecificMonths:                                               // R-PRJ-4
            if item.specific_months.contains(m): push(candidate)
        else: push(candidate)
    offset += inc
    if offset > 1200: break                                                           // R-PRJ-5
```

* Every candidate is computed **from the original start month** plus an offset, never from the previous
  candidate. That is why Jan 31 gives Feb 28 and then **Mar 31** (no drift).
* With jiff: `jiff::civil::Date::new(y, m, 1)?.last_of_month().day()` gives the month length; 7 days is
  `date.checked_add(jiff::Span::new().days(7))`.
* If `from > until`, or `end_date < start_date`, the loops naturally return an empty Vec.

## Steps

1. Create `projector.rs`, add `pub mod projector;` to `lib.rs` (delete the placeholder test).
2. Write the unit tests below using `pf_model::testing::{d, m}` and a small helper
   `fn item(freq, start, end: Option<&str>, months: &[u32]) -> BudgetItem` built with `BudgetItem::new`.
3. Implement `clamped_date`, then `generate_dates`, then the two helpers.
4. Add the proptest block (`proptest!` macro) with the properties below.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_prj_1_end_date_caps_range` | R-PRJ-1 | monthly from 2026-01-15, end_date 2026-03-20, until 2026-12-31 → Jan 15, Feb 15, Mar 15 |
| `r_prj_1_until_before_end_date` | R-PRJ-1 | end_date later than until → until wins |
| `r_prj_1_from_after_until_empty` | R-PRJ-1 | from > until → empty |
| `r_prj_2_weekly_steps_seven_days` | R-PRJ-2 | weekly from 2026-01-01, range Jan 1–Jan 31 → 1, 8, 15, 22, 29 |
| `r_prj_2_weekly_skips_before_from` | R-PRJ-2 | weekly start 2026-01-01, from 2026-01-10 → first date Jan 15 (aligned to start, not to from) |
| `r_prj_2_weekly_start_after_until_empty` | R-PRJ-2 | start after until → empty |
| `r_prj_3_jan31_clamps_to_feb28` | R-PRJ-3 | monthly 2026-01-31 → Jan 31, Feb 28, Mar 31, Apr 30 |
| `r_prj_3_leap_year_feb29` | R-PRJ-3 | monthly 2028-01-31 → Feb 29 2028 |
| `r_prj_3_every_other_month` | R-PRJ-3 | start 2026-01-10 → Jan, Mar, May… |
| `r_prj_3_quarterly_from_mid_year` | R-PRJ-3 | start 2025-11-05, from 2026-01-01 to 2026-12-31 → Feb 5, May 5, Aug 5, Nov 5 |
| `r_prj_3_yearly_across_years` | R-PRJ-3 | start 2024-02-29 yearly, range 2025-01-01..2028-12-31 → 2025-02-28, 2026-02-28, 2027-02-28, 2028-02-29 |
| `r_prj_3_from_before_start_date` | R-PRJ-3 | from earlier than start → no dates before start |
| `r_prj_3_inclusive_bounds` | R-PRJ-3 | an occurrence exactly on from and one exactly on until are both included |
| `r_prj_4_specific_months_filter` | R-PRJ-4 | months [1,4,7,10], start 2026-01-15 → Jan, Apr, Jul, Oct 15 |
| `r_prj_4_specific_months_empty_list` | R-PRJ-4 | empty months → empty |
| `r_prj_4_specific_months_clamps` | R-PRJ-4 | start 2026-01-31, months [2] → Feb 28 |
| `r_prj_5_safety_stop_1200_months` | R-PRJ-5 | monthly start 2000-01-01, until 2200-01-01 → exactly 1201 dates (offsets 0..=1200), last 2100-01-01 |
| `r_prj_6_projected_amount_signs` | R-PRJ-6 | Bill 100 → −100; Expense −50 → −50; Income −20 → +20 |
| `r_prj_6_projected_payee_text` | R-PRJ-6 | Bill "Rent" → "Rent"; Income "Salary" → "Salary"; Expense "Groceries" → "Expense: Groceries" |
| `prop_dates_sorted_and_in_range` | R-PRJ-1 | property: strictly ascending; every date within `[max(from,start), min(until,end)]` |
| `prop_month_based_day_is_clamped_start_day` | R-PRJ-3 | property: for non-weekly, every `date.day() == min(start.day(), month_len)` |
| `prop_weekly_diffs_are_seven` | R-PRJ-2 | property: consecutive weekly dates differ by exactly 7 days |
| `prop_specific_months_subset` | R-PRJ-4 | property: every month is in `specific_months` |

Property strategy: years 2000..=2040, month 1..=12, day 1..=28 for from/until, any day 1..=31 for start (clamp
to month length when constructing), ranges up to about 10 years so tests stay fast.

## Pitfalls

- **Off by one at the safety stop:** Swift increments the offset, *then* checks `> 1200`, so offset 1200 is
  still evaluated. Test `r_prj_5` pins this.
- jiff `Date::new(year: i16, month: i8, day: i8)` takes small integer types; cast carefully.
- Do not use `date + 1.month()` repeatedly (it drifts after a clamp). Always offset from the start.
- Weekly ignores `specific_months`, and Weekly does **not** check `>= start_date` (it starts there anyway).
- Swift compares dates after normalising to the start of the day; we use calendar dates, so no time handling at all.

## Done when

```bash
cargo test -p pf-core projector
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules | grep R-PRJ-[1-5]      # no output
```

## Out of scope

- Creating transactions from the dates (T10c). Reproject (R-PRJ-9, T10c). P&L budget totals (T08).
