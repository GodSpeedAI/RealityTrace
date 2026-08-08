//! sxr-ledger
//!
//! SQLite adapter for the durable ledger: frozen v1 schema (§10.3), hash
//! chains (§10.6), generation history (§10.3, §11.3a), transactional outbox
//! (§11.2), writer flock (§11.1), capability probes (§10.2a), the CAS store
//! (§10.8), and the durable commit queue (§11.1a).
//!
//! The pure chain math lives in `sxr_core::chain`; this crate owns the I/O.

pub mod capability;
pub mod cas;
pub mod error;
pub mod generation;
pub mod history;
pub mod ledger;
pub mod outbox;
pub mod quarantine;
pub mod queue;
pub mod schema;
pub mod writer_lock;

pub use error::{Error, Result};
pub use ledger::{AppendOutcome, Ledger, LedgerConfig, VerifyReport};
