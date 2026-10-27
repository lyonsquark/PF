//! The engine: `Engine::apply` is the only way to change data. Also holds undo/redo, persistence,
//! UI-level state and the `World` test helper.

pub mod commands;
pub mod engine;
pub mod error;
pub mod state;
pub mod testing;
pub mod writer;

pub use engine::*;
pub use error::*;
pub use pf_model::{Change, Record, RowKey};
pub use state::AppState;
pub use writer::Writer;
