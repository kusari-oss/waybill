//! Feature 229 US3: effective waybill version string, honoring the
//! `WAYBILL_VERSION` build-time env override.
//!
//! **Mechanism**: `waybill-cli/build.rs` reads the shell env var,
//! validates SemVer shape (fail-closed on invalid), and writes the
//! effective value to `$OUT_DIR/waybill_version.rs`. This file is
//! `include!()`ed here so the const's value comes from cargo's OUT_DIR
//! machinery, which IS properly cache-invalidated by
//! `cargo:rerun-if-env-changed=WAYBILL_VERSION` (unlike `option_env!()`
//! which cargo does NOT auto-fingerprint).
//!
//! Falls back to `env!("CARGO_PKG_VERSION")` in build.rs when the env
//! var is unset — which is every developer build and every
//! tag-triggered stable release.
//!
//! Reference: feature 229 FR-005, FR-012.

// The generated file at `$OUT_DIR/waybill_version.rs` defines exactly:
//   pub const VERSION: &str = "<effective version>";
include!(concat!(env!("OUT_DIR"), "/waybill_version.rs"));
