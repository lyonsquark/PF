//! Engine errors. The `Validation` message is shown to the user verbatim.

/// Errors returned by the engine and its commands.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineError {
    /// The row is locked in the current scenario (R-TXN-1).
    #[error("This transaction is locked in the current scenario")]
    Locked,
    /// A row id or name does not exist.
    #[error("Not found: {0}")]
    NotFound(String),
    /// A business-rule violation; the message is shown to the user.
    #[error("{0}")]
    Validation(String),
    /// The SQLite store failed; the in-memory data was rolled back.
    #[error("Storage error: {0}")]
    Store(String),
}

/// Result alias used across the engine.
pub type Result<T> = std::result::Result<T, EngineError>;

impl From<pf_store::StoreError> for EngineError {
    fn from(e: pf_store::StoreError) -> Self {
        EngineError::Store(e.to_string())
    }
}
