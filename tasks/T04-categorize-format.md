# T04: pf-core categorize, pattern validation, colours, money formatting, context names

**Phase:** 1 · **Deps:** T01 · **Crate(s):** `pf-core` · **Size:** medium (about 350 lines of code, 400 of tests)

## Goal

The small pure helpers used everywhere: auto-categorization by match patterns, pattern
validation, the category picker order and tree, emoji detection, colour hex parsing and
formatting, US-dollar money formatting (identical to Swift's `NumberFormatter` `.currency`),
amount-field parsing, and the obligation context-name generator.

## Read first

- Rules: R-CAT-1, R-CAT-2, R-CAT-3 (matching part), R-CAT-4 (sorting part), R-CAT-5, R-CAT-6, R-CAT-8, R-CTX-4, R-OBL-4
- Swift: `Views/TransactionListView.swift` → `autoCategorize(payee:)`
- Swift: `Views/AddTransactionView.swift` → `autoCategorize(from:)`, `isRecDisabled`, `addPayeeToMatchPatterns`
- Swift: `Views/CategoriesView.swift` → `saveCategory`, `applyRulesToUncategorized`, the `body` tree, `extension String { var isEmoji }`
- Swift: `Views/ContextsView.swift` → `extension Color { init?(hex:) ; toHex() }`
- Swift: `Views/ObligationsView.swift` → `generateContextName`
- Swift: `Views/AmountEditField.swift` → `commitEdit`, `decimalToString`
- Swift: `Services/APIHandlers.swift` → the `NumberFormatter` usage (`.currency`, `"USD"`)

## Deliverables

```rust
// crates/pf-core/src/lib.rs
pub mod categorize;
pub mod color;
pub mod format;
pub mod names;
```

### `categorize.rs`
```rust
/// R-CAT-1: first matching live category for `payee`. Categories in name order (Db::categories_by_name),
/// patterns in stored order. Match = payee contains the pattern case-insensitively, OR the pattern
/// compiles as a `regex::Regex` and `is_match(payee)` (case-SENSITIVE, like Swift's Regex).
/// Blank (whitespace-only) payee → None.
pub fn auto_categorize(db: &Db, payee: &str) -> Option<CategoryId>;

/// R-CAT-6: add-transaction dialog variant. Substring-only (no regex), case-insensitive, same order.
/// Empty text → None.
pub fn auto_categorize_substring(db: &Db, text: &str) -> Option<CategoryId>;

/// Shared matcher: true if any pattern matches `payee` per R-CAT-1.
pub fn matches_any(patterns: &[String], payee: &str) -> bool;

/// R-CAT-2: contains any of the characters  ^ $ . * + ? [ ] { } ( ) | \
pub fn looks_like_regex(pattern: &str) -> bool;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatternValidation { pub valid: Vec<String>, pub invalid: Vec<String> }
impl PatternValidation {
    /// The alert body Swift shows: each invalid pattern as "• {p}", joined by "\n". Empty if none.
    pub fn alert_text(&self) -> String;
}
/// R-CAT-2: split `input` on ',', trim, drop empties; a pattern is invalid iff it looks like a regex
/// and `regex::Regex::new` fails. Order preserved. Duplicates kept (Swift keeps them).
pub fn validate_patterns(input: &str) -> PatternValidation;

/// Patterns as the editor shows them: joined with ", " (Swift `joined(separator: ", ")`).
pub fn patterns_to_input(patterns: &[String]) -> String;

/// R-CAT-3 (pure part): ids of live transactions with no category and non-empty payee that match
/// any of `patterns` (use only VALID patterns, per R-CAT-2). The write happens in T10b.
pub fn uncategorized_matches(db: &Db, patterns: &[String]) -> Vec<TransactionId>;
/// R-CAT-3 message: "Applied to {n} uncategorized transaction(s)."
pub fn apply_rules_message(n: usize) -> String;

/// R-CAT-5: the "Rec" button is disabled when no category, the trimmed payee is empty, or the
/// category already has a pattern equal to the trimmed payee case-insensitively.
pub fn rec_disabled(db: &Db, category: Option<CategoryId>, payee: &str) -> bool;
/// R-CAT-5 confirmation: `Added "{payee}" to {fullName} auto-match`.
pub fn rec_message(db: &Db, category: CategoryId, payee: &str) -> String;

/// R-CAT-4: live categories for pickers, sorted by full name, case-insensitive (to_lowercase), ties by id.
pub fn categories_for_picker(db: &Db) -> Vec<(CategoryId, String)>;   // (id, full_name)

/// R-CAT-8: the Categories screen. Sections in CategoryType::ALL order (Income, Expense, Transfer),
/// each with top-level live categories of that type sorted by name, each followed by its live
/// subcategories (depth 1) sorted by name. Empty sections omitted. Title = "Income" / "Expense" / "Transfer".
#[derive(Debug, Clone, PartialEq)]
pub struct CategorySection { pub title: &'static str, pub rows: Vec<(CategoryId, u8)> }  // (id, depth 0|1)
pub fn category_tree(db: &Db) -> Vec<CategorySection>;

/// Swift `String.isEmoji`: true if ANY scalar is in the listed ranges (see below).
pub fn is_emoji(s: &str) -> bool;
```
Emoji ranges (copy from Swift exactly): `0x1F600..=0x1F64F, 0x1F300..=0x1F5FF, 0x1F680..=0x1F6FF,
0x1F1E6..=0x1F1FF, 0x2600..=0x26FF, 0x2700..=0x27BF, 0xE0020..=0xE007F, 0xFE00..=0xFE0F,
0x1F900..=0x1F9FF, 0x1FA70..=0x1FAFF`.

### `color.rs` (no egui types: pf-core must not depend on egui)
```rust
/// R-CTX-4: trim, strip every '#', then 6 hex digits → [r,g,b,255]; 8 → [r,g,b,a]; otherwise None.
pub fn parse_hex(s: &str) -> Option<[u8; 4]>;
/// Swift `toHex()`: "#%06x", i.e. lowercase, alpha dropped. [255,0,170,_] → "#ff00aa".
pub fn to_hex(rgb: [u8; 3]) -> String;
/// Default context colour when parsing fails (SwiftUI `.blue`, approximately): [0, 122, 255, 255].
pub const DEFAULT_BLUE: [u8; 4] = [0, 122, 255, 255];
```

### `format.rs`
```rust
/// US currency like Swift NumberFormatter(.currency, "USD", en_US):
/// round to 2 dp with banker's rounding (MidpointNearestEven, NSNumberFormatter's default),
/// group thousands with ',', "$" prefix, negative = "-$…". Examples:
/// 1234.5 → "$1,234.50"; -1234.567 → "-$1,234.57"; 0 → "$0.00"; 0.125 → "$0.12"; 0.135 → "$0.14";
/// 1000000 → "$1,000,000.00"; -0.001 → "-$0.00" (Swift keeps the sign of the unrounded value; see pitfalls).
pub fn format_currency(amount: Decimal) -> String;

/// Amount shown while editing (Swift AmountEditField.decimalToString): no grouping,
/// 0–2 fraction digits (trailing zeros dropped), banker's rounding: 12.50 → "12.5", 3 → "3", -0.125 → "-0.12".
pub fn format_amount_for_edit(amount: Decimal) -> String;

/// Swift commitEdit: strip "$" and ",", trim, parse Decimal. Invalid → None (caller keeps the old value).
pub fn parse_amount_input(s: &str) -> Option<Decimal>;
```

### `names.rs`
```rust
/// R-OBL-4: lowercase; split on Unicode whitespace; drop empties; join with '-'; keep only chars that are
/// alphabetic, numeric or '-'. Empty → "". If `existing` contains it (case-insensitive), try "{base}-2",
/// "{base}-3", … until unique.
pub fn generate_context_name<'a>(name: &str, existing: impl IntoIterator<Item = &'a str> + Clone) -> String;
```
(Take `existing: &[&str]` instead if the iterator bound is awkward; that is the simpler choice.)

## Steps

1. Create the four modules; write the tests first.
2. `categorize.rs`: build regexes on the fly (performance does not matter here; T12b may add caching).
   For `matches_any`, check substring first, then regex, per pattern, in order.
3. `format.rs`: use `amount.round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven)`, then format the
   absolute value's integer part with commas and always 2 decimals; prefix "-" if `amount.is_sign_negative()` **and**
   `amount != 0` (see pitfall about `-0.001`).
4. `color.rs`, `names.rs`.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_cat_1_substring_case_insensitive` | R-CAT-1 | pattern "STARBUCKS" matches "Starbucks #123" |
| `r_cat_1_regex_pattern` | R-CAT-1 | pattern `^AMZN.*Mktp` matches "AMZN Mktp US" |
| `r_cat_1_regex_is_case_sensitive` | R-CAT-1 | `^amzn\d` does not match "AMZN1" by regex; also not by substring → None |
| `r_cat_1_first_category_by_name_wins` | R-CAT-1 | "Auto" and "Zoo" both match → Auto |
| `r_cat_1_blank_payee_none` | R-CAT-1 | "   " → None |
| `r_cat_1_deleted_category_ignored` | R-CAT-1 | |
| `r_cat_1_invalid_regex_falls_back_to_substring` | R-CAT-1 | pattern "(" matches payee "a ( b" by substring |
| `r_cat_2_looks_like_regex` | R-CAT-2 | "abc" false; "a.b", "x|y", "\\d" true |
| `r_cat_2_invalid_regex_removed` | R-CAT-2 | input "coffee, [bad, ^ok$" → valid ["coffee","^ok$"], invalid ["[bad"]; `alert_text() == "• [bad"` |
| `r_cat_2_trims_and_drops_empty` | R-CAT-2 | " a , ,b ," → ["a","b"] |
| `r_cat_3_uncategorized_matches_only_uncategorized` | R-CAT-3 | categorized and empty-payee rows excluded; deleted excluded |
| `r_cat_3_message` | R-CAT-3 | `apply_rules_message(2) == "Applied to 2 uncategorized transaction(s)."` |
| `r_cat_4_picker_sorted_by_full_name_ci` | R-CAT-4 | "b", "A/z", "a" → "a", "A/z", "b" |
| `r_cat_5_rec_disabled_cases` | R-CAT-5 | no category; empty payee; payee already a pattern ("coffee" vs pattern "Coffee") → disabled; otherwise enabled |
| `r_cat_5_rec_message` | R-CAT-5 | `Added "Joe's" to Food/Dining auto-match` |
| `r_cat_6_substring_only_no_regex` | R-CAT-6 | pattern `^Shell` does not match "Shell Oil" via regex in this variant (it is not a substring because of `^`) → None |
| `r_cat_8_tree_sections_and_depth` | R-CAT-8 | Income section before Expense; child at depth 1 right after its parent; deleted child hidden |
| `is_emoji_matches_swift_ranges` | R-CAT-8 | "🍔" true, "cart" false, "☕" true (0x2615), "A" false |
| `r_ctx_4_parse_hex_six_and_eight` | R-CTX-4 | "#FFD700" → [255,215,0,255]; "8B451380" → [139,69,19,128] |
| `r_ctx_4_parse_hex_invalid` | R-CTX-4 | "", "#FFF", "#GGGGGG", "#1234567" → None |
| `to_hex_lowercase_six_digits` | R-CTX-4 | [255,0,170] → "#ff00aa"; [0,0,0] → "#000000" |
| `format_currency_examples` | – | every example in the doc comment |
| `format_currency_bankers_rounding` | – | 2.345 → "$2.34", 2.355 → "$2.36" |
| `format_amount_for_edit_examples` | – | the examples in the doc comment |
| `parse_amount_input_strips_symbols` | – | "$1,234.50" → 1234.50; " -3 " → -3; "abc" → None |
| `r_obl_4_basic` | R-OBL-4 | "Property Tax 2026" → "property-tax-2026" |
| `r_obl_4_strips_punctuation` | R-OBL-4 | "Kid's  Braces!" → "kids-braces" |
| `r_obl_4_dedupes` | R-OBL-4 | existing ["property-tax", "Property-Tax-2"] → "property-tax-3" |
| `r_obl_4_empty` | R-OBL-4 | "!!!" → "" |
| `r_obl_4_unicode_letters_kept` | R-OBL-4 | "Café Fund" → "café-fund" |

## Pitfalls

- **Regex syntax:** Swift `Regex` and the Rust `regex` crate differ slightly (no look-around in Rust).
  Patterns that compile in Swift but not in Rust are treated as "not a regex" (substring only) at match time,
  and as invalid when the user saves (R-CAT-2). This is acceptable; note it in a code comment.
- `-0.001` formatting: NSNumberFormatter prints `-$0.00`. Verify against the Swift parity capture in T26; if it
  differs, T26 fixes this function. Keep the test as written for now.
- `format_currency` must not use `f64` at any point.
- `to_lowercase()` on both sides for case-insensitive comparisons; never `eq_ignore_ascii_case` (non-ASCII payees exist).
- The pf-core crate must not depend on egui. Colours are `[u8; 4]`; pf-ui converts them to `Color32`.

## Done when

```bash
cargo test -p pf-core categorize color format names
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules | grep -E "R-(CAT-[1-68]|CTX-4|OBL-4)"   # no output
```

## Out of scope

- Writing categories to transactions (T10b), category deletion (R-CAT-7, T10b), UI widgets (T20).
