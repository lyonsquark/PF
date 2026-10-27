//! Localhost HTTP API, byte-compatible with the Swift app (T12, R-API-1..10). `router` maps a
//! request to a reply without sockets; `server` runs it on a background thread.

pub mod router;
pub mod server;

pub use router::*;
pub use server::*;
