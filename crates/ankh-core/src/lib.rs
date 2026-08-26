//! `ankh-core` is the one place that knows about `rslib` types.
//!
//! Everything above it (CLI, TUI, Lua) talks in the domain vocabulary from
//! `CONTEXT.md`: profiles, credentials, decks, cards, notes, sync reports.
//! Keeping the `anki` crate confined here means an upstream bump is a
//! single-crate change.

pub mod auth;
pub mod decks;
pub mod engine;
pub mod error;
pub mod paths;
pub mod sync;

pub use auth::{AuthStore, Credentials};
pub use decks::{DeckId, DeckNode, DeckTree};
pub use engine::Engine;
pub use error::{Error, Result};
pub use paths::Paths;
pub use sync::{SyncOptions, SyncOutcome, SyncProgress, SyncReport};

/// Version of the JSON output contract. Bump on any breaking change to the
/// shape of `--format json` output.
pub const SCHEMA_VERSION: u32 = 1;
