// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-owned editor/link state. No game is discovered, connected or mutated
//! merely by constructing this library or observing a selection.
pub mod protocol;
pub mod state;
pub mod store;
pub use store::*;
pub mod transaction;
pub use state::*;
use std::fmt;
pub use transaction::*;
#[derive(Debug)]
pub enum LinkError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Sqlite(rusqlite::Error),
    Invalid(String),
    Stale(String),
    Unsupported(String),
    Budget(String),
    Authentication,
    Replay,
    Cancelled,
}
pub type Result<T> = std::result::Result<T, LinkError>;
impl From<std::io::Error> for LinkError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for LinkError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
impl From<rusqlite::Error> for LinkError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
            Self::Sqlite(e) => write!(f, "{e}"),
            Self::Invalid(e) => write!(f, "Invalid link state: {e}"),
            Self::Stale(e) => write!(f, "Link state changed: {e}"),
            Self::Unsupported(e) => write!(f, "Unsupported link capability: {e}"),
            Self::Budget(e) => write!(f, "Link budget exceeded: {e}"),
            Self::Authentication => write!(f, "Local peer authentication failed"),
            Self::Replay => write!(f, "Repeated or out-of-order local message"),
            Self::Cancelled => write!(
                f,
                "Link checkpoint operation cancelled; previous checkpoint preserved"
            ),
        }
    }
}
impl std::error::Error for LinkError {}
