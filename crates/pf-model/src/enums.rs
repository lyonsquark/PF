//! String-backed enums. The `as_str` values are written to the Markdown backup, so they must
//! match Swift's raw values exactly.

use std::fmt;
use std::str::FromStr;

/// Returned when a string is not an exact raw value of an enum.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind}: {value:?}")]
pub struct ParseEnumError {
    /// Name of the enum that was being parsed.
    pub kind: &'static str,
    /// The rejected input.
    pub value: String,
}

/// Generates an enum with raw strings, label, `ALL`, `FromStr`, `Default` and `Display`.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $variant:ident = $raw:literal => $label:literal, )+
        }
        default = $default:ident;
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize,
        )]
        pub enum $name {
            $(
                #[doc = concat!("Stored as `", $raw, "`.")]
                #[serde(rename = $raw)]
                $variant,
            )+
        }

        impl $name {
            const COUNT: usize = [$(stringify!($variant)),+].len();

            /// Every variant, in Swift's `allCases` order.
            pub const ALL: [Self; Self::COUNT] = [$(Self::$variant),+];

            /// The stored or exported raw string.
            pub fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $raw, )+
                }
            }

            /// The user-facing label.
            pub fn label(self) -> &'static str {
                match self {
                    $( Self::$variant => $label, )+
                }
            }

            /// Parses the raw string, or returns the Swift default when it is unknown.
            pub fn parse_or_default(s: &str) -> Self {
                s.parse().unwrap_or_default()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl FromStr for $name {
            type Err = ParseEnumError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .into_iter()
                    .find(|v| v.as_str() == s)
                    .ok_or_else(|| ParseEnumError {
                        kind: stringify!($name),
                        value: s.to_string(),
                    })
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.label())
            }
        }
    };
}

string_enum! {
    /// Kind of account.
    pub enum AccountType {
        Checking = "checking" => "Checking",
        Savings = "savings" => "Savings",
        Credit = "credit" => "Credit",
        Loan = "loan" => "Loan",
        Investment = "investment" => "Investment",
        Cash = "cash" => "Cash",
    }
    default = Checking;
}

string_enum! {
    /// Income, expense or transfer category.
    pub enum CategoryType {
        Income = "income" => "Income",
        Expense = "expense" => "Expense",
        Transfer = "transfer" => "Transfer",
    }
    default = Expense;
}

string_enum! {
    /// Status of a transaction.
    pub enum TransactionStatus {
        Pending = "pending" => "Pending",
        Cleared = "cleared" => "Cleared",
        Reconciled = "reconciled" => "Reconciled",
        Future = "future" => "Future",
    }
    default = Cleared;
}

string_enum! {
    /// Funding state of an obligation.
    pub enum ObligationStatus {
        Funding = "Funding" => "Funding",
        Spending = "Spending" => "Spending",
        Future = "Future" => "Future",
        Deferred = "Deferred" => "Deferred",
        Completed = "Completed" => "Completed",
    }
    default = Funding;
}

string_enum! {
    /// Kind of budget item.
    pub enum BudgetItemType {
        Bill = "Bill" => "Bill",
        Expense = "Expense" => "Expense",
        Income = "Income" => "Income",
    }
    default = Expense;
}

string_enum! {
    /// How often a budget item recurs.
    pub enum BudgetFrequency {
        Weekly = "Weekly" => "Weekly",
        Monthly = "Monthly" => "Monthly",
        EveryOtherMonth = "Every Other Month" => "Every Other Month",
        Quarterly = "Quarterly" => "Quarterly",
        Yearly = "Yearly" => "Yearly",
        SpecificMonths = "Specific Months" => "Specific Months",
    }
    default = Monthly;
}

string_enum! {
    /// Reporting window selected in the UI (persisted as a setting).
    pub enum TimePeriod {
        PastMonth = "Past Month" => "Past Month",
        Past60Days = "Past 60 Days" => "Past 60 Days",
        YearToDate = "Year to Date" => "Year to Date",
        PastYear = "Past Year" => "Past Year",
        Archival = "Archival" => "Archival",
        All = "All" => "All",
    }
    default = PastMonth;
}

/// Swift's lenient match (`MarkdownDataService.importBudgetItem`): the input is lowercased,
/// and matches a raw value if it equals the raw value (lowercased) or equals it with spaces removed.
/// Spaces are removed from the input only, so `"everyothermonth"` does not match `"Every Other Month"`.
fn lenient_find<T: Copy>(all: &[T], s: &str, raw: fn(T) -> &'static str) -> Option<T> {
    let input = s.to_lowercase();
    let squeezed = input.replace(' ', "");
    all.iter().copied().find(|v| {
        let r = raw(*v).to_lowercase();
        r == input || r == squeezed
    })
}

impl BudgetItemType {
    /// Case-insensitive; also matches with spaces removed. Used by the Markdown importer.
    pub fn parse_lenient(s: &str) -> Option<Self> {
        lenient_find(&Self::ALL, s, Self::as_str)
    }
}

impl BudgetFrequency {
    /// Case-insensitive; also matches with spaces removed from the input. Used by the Markdown importer.
    pub fn parse_lenient(s: &str) -> Option<Self> {
        lenient_find(&Self::ALL, s, Self::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks `parse(as_str()) == variant` for every variant of one enum.
    fn assert_round_trip<T>(all: &[T], raw: fn(T) -> &'static str)
    where
        T: FromStr<Err = ParseEnumError> + fmt::Debug + PartialEq + Copy,
    {
        for v in all {
            let parsed: T = raw(*v).parse().expect("own raw value must parse");
            assert_eq!(parsed, *v);
        }
    }

    #[test]
    fn enum_strings_round_trip() {
        assert_round_trip(&AccountType::ALL, AccountType::as_str);
        assert_round_trip(&CategoryType::ALL, CategoryType::as_str);
        assert_round_trip(&TransactionStatus::ALL, TransactionStatus::as_str);
        assert_round_trip(&ObligationStatus::ALL, ObligationStatus::as_str);
        assert_round_trip(&BudgetItemType::ALL, BudgetItemType::as_str);
        assert_round_trip(&BudgetFrequency::ALL, BudgetFrequency::as_str);
        assert_round_trip(&TimePeriod::ALL, TimePeriod::as_str);
        assert_eq!(AccountType::ALL.len(), 6);
        assert_eq!(TimePeriod::ALL.len(), 6);
    }

    #[test]
    fn enum_defaults_match_swift() {
        assert_eq!(AccountType::default(), AccountType::Checking);
        assert_eq!(CategoryType::default(), CategoryType::Expense);
        assert_eq!(TransactionStatus::default(), TransactionStatus::Cleared);
        assert_eq!(ObligationStatus::default(), ObligationStatus::Funding);
        assert_eq!(BudgetItemType::default(), BudgetItemType::Expense);
        assert_eq!(BudgetFrequency::default(), BudgetFrequency::Monthly);
        assert_eq!(TimePeriod::default(), TimePeriod::PastMonth);
    }

    #[test]
    fn enum_parse_is_exact() {
        assert!("Checking".parse::<AccountType>().is_err());
        assert!("funding".parse::<ObligationStatus>().is_err());
        let err = "bogus"
            .parse::<CategoryType>()
            .expect_err("bogus is not a category type");
        assert_eq!(err.kind, "CategoryType");
        assert_eq!(err.value, "bogus");
    }

    #[test]
    fn parse_or_default_falls_back() {
        assert_eq!(
            AccountType::parse_or_default("bogus"),
            AccountType::Checking
        );
        assert_eq!(
            TransactionStatus::parse_or_default(""),
            TransactionStatus::Cleared
        );
        assert_eq!(
            ObligationStatus::parse_or_default("funding"),
            ObligationStatus::Funding
        );
    }

    #[test]
    fn every_other_month_lenient_quirk() {
        use BudgetFrequency as F;
        assert_eq!(
            F::parse_lenient("every other month"),
            Some(F::EveryOtherMonth)
        );
        assert_eq!(
            F::parse_lenient("EVERY OTHER MONTH"),
            Some(F::EveryOtherMonth)
        );
        // Spaces are removed from the input only, so this does NOT match (Swift behaviour).
        assert_eq!(F::parse_lenient("everyothermonth"), None);
        assert_eq!(F::parse_lenient("Mon thly"), Some(F::Monthly));
        assert_eq!(F::parse_lenient("MONTHLY"), Some(F::Monthly));
    }

    #[test]
    fn budget_item_type_lenient() {
        assert_eq!(
            BudgetItemType::parse_lenient("bill"),
            Some(BudgetItemType::Bill)
        );
        assert_eq!(
            BudgetItemType::parse_lenient("BILL"),
            Some(BudgetItemType::Bill)
        );
        assert_eq!(
            BudgetItemType::parse_lenient("Income"),
            Some(BudgetItemType::Income)
        );
        assert_eq!(BudgetItemType::parse_lenient("bills"), None);
    }

    #[test]
    fn labels_match_swift() {
        assert_eq!(TransactionStatus::Reconciled.label(), "Reconciled");
        assert_eq!(TransactionStatus::Reconciled.to_string(), "Reconciled");
        assert_eq!(
            BudgetFrequency::EveryOtherMonth.as_str(),
            "Every Other Month"
        );
        assert_eq!(TimePeriod::YearToDate.as_str(), "Year to Date");
        assert_eq!(AccountType::Checking.label(), "Checking");
    }
}
