//! `sxr-cli` library facade. Exposes the CLI's modules so integration tests
//! can exercise projection and query logic directly (Slice 3 G_declared/FTS).
//! The binary (`main.rs`) is the primary entry point; this lib mirrors the
//! same module set.

pub mod commands;
pub mod envelopes;
pub mod ledger_io;
pub mod output;
pub mod projections;
pub mod state;
