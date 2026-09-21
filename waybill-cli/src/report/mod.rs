//! Repo observation report — milestone 924 / issue #932.
//!
//! A versioned, machine-readable account of what waybill saw, claimed,
//! ignored, and **could not determine** while traversing a repository.
//!
//! # The organising principle
//!
//! **This module records observations and typed uncertainty, not
//! conclusions.** "I could not determine what this is" is a valid answer when
//! it carries what *was* observable: a directory of 47 binary files and a
//! directory of 47 UTF-8 text files are both unclassified and mean entirely
//! different things.
//!
//! That is a deliberate departure from how the rest of waybill behaves.
//! Constitution Principle IX requires the SBOM to assert nothing it cannot
//! support; here the uncertainty *is* the payload (spec FR-014).
//!
//! # Why this is nearly free
//!
//! `walk_registry::dispatch::dispatch_file` already returns the set of readers
//! that claimed each file, and the walker already binds that value one line
//! before handing it to a metrics sink. The census is that value, retained —
//! not a second traversal (research R1).
//!
//! # What this module must never do
//!
//! - Change emitted SBOM content in any format (FR-024).
//! - Issue a network request (FR-022).
//! - Transmit a report anywhere (FR-023).
//! - Resolve genuine ambiguity by preference (FR-014).

pub(crate) mod census;
pub(crate) mod content_kind;
pub(crate) mod ecosystems;
pub(crate) mod schema;
pub(crate) mod significance;
