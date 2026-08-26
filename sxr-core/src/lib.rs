//! # sxr-core
//!
//! Domain types, canonical JSON, CEP envelope construction and validation,
//! the generated record-kind registry, and threshold evaluation for the
//! Semantic Exhaust Runtime.
//!
//! **No I/O.** This crate must never depend on rusqlite, filesystem/process
//! access, or DomainForge core (AC-CORE-001, AC-SXR-001). It is pure data
//! and pure functions.

pub mod canonical_json;
pub mod chain;
pub mod clock;
pub mod embedded_schema;
pub mod envelope;
pub mod evidence_emit;
pub mod graph;
pub mod proof_ingest;
pub mod record_kinds;
pub mod threshold;
pub mod types;

pub use types::*;
