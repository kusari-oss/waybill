//! Internal testing utilities shared across the waybill workspace.
//!
//! Only meaningful when consumed from `#[cfg(test)]` code — either
//! src-side `mod tests { ... }` blocks OR integration test binaries
//! under `waybill-cli/tests/`. The module is `pub` (not
//! `#[cfg(test)]`-gated) because integration test binaries import
//! from `waybill` as a normal library dependency and cannot see
//! `#[cfg(test)]`-only items in the parent crate.
//!
//! Non-test callers importing anything from this module get exactly
//! the semantics documented — no side effects beyond what the docs
//! state. There is no reason for production code to reach into these
//! helpers.

pub mod env_guard;

pub use env_guard::EnvGuard;
