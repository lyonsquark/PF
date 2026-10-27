//! Newtype ids for every entity. Ids are shared `i64` values from one sequence.

/// Raw id value as stored in SQLite (`INTEGER PRIMARY KEY`).
pub type Id = i64;

/// Generates a `Copy` newtype id wrapping [`Id`].
macro_rules! id_type {
    ($name:ident) => {
        /// Identifier newtype for one entity kind.
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            serde::Serialize,
            serde::Deserialize,
        )]
        pub struct $name(pub Id);

        impl $name {
            /// The raw numeric id.
            pub fn raw(self) -> Id {
                self.0
            }
        }

        impl From<Id> for $name {
            fn from(value: Id) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(AccountId);
id_type!(TransactionId);
id_type!(CategoryId);
id_type!(ContextId);
id_type!(ScenarioId);
id_type!(ObligationId);
id_type!(BudgetItemId);
id_type!(ImportMappingId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_newtypes_round_trip_raw_value() {
        let id = AccountId::from(7);
        assert_eq!(id, AccountId(7));
        assert_eq!(id.raw(), 7);
        assert_eq!(id.to_string(), "7");
    }
}
