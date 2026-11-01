//! SQLite persistence for the in-memory `Db`: schema, load, atomic apply, purge (R-PRG-1).

mod codec;
mod rows;
mod schema;
mod store;

pub use store::{
    PurgeReport, Store, StoreError, default_db_path, legacy_db_to_move, move_legacy_db,
};
