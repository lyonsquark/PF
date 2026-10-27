//! CSV import core (R-CSV-1 … R-CSV-16): parsing, column detection, mapping lookup, unmapped
//! values, translation choices and the import plan. Pure functions; nothing is written here.
//! `PFApp/Sources/PFApp/Services/CSVImportService.swift` is the behavioural truth.

mod columns;
mod lookup;
mod parse;
mod plan;
mod translate;
mod unmapped;

#[cfg(test)]
mod tests;

pub use columns::{
    ColumnMapping, ImportField, detect_columns, saved_column_assignments, saved_negate,
    validate_required,
};
pub use lookup::lookup_mapping;
pub use parse::{ParsedCsv, parse_csv, split_csv_line};
pub use plan::{
    ContextRef, ImportPlan, ImportRequest, ImportResult, PlannedTransaction, parse_csv_amount,
    parse_csv_date, plan_import,
};
pub use translate::{
    CategoryChoice, MappingPlan, NewCategory, NewMapping, TranslationChoices, plan_mappings,
};
pub use unmapped::{UnmappedValues, collect_unmapped};
