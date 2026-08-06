//! Library-crate root for waybill-cli.
//!
//! waybill-cli is canonically a binary crate (`src/main.rs` is the
//! entry point); this library exists **only** to share a small
//! amount of code between the binary AND its integration tests
//! under `tests/`. Rust integration tests live in their own crate
//! and cannot import binary-internal modules; the lib + bin layout
//! is the standard solution.
//!
//! Today the library exposes one module:
//!
//! * [`parity`] — milestone 013: the canonical cross-format datum
//!   catalog parser (`parity::catalog`) + per-row extractor table
//!   (`parity::extractors`). Consumed by:
//!     * `src/cli/parity_cmd.rs` (US3 — the `waybill sbom
//!       parity-check` diagnostic) via `crate::parity::*`
//!     * `waybill-cli/tests/holistic_parity.rs` (US1 holistic
//!       parity test) via `waybill::parity::*`
//!     * `waybill-cli/tests/mapping_doc_bidirectional.rs` (US2
//!       auto-discovery + reverse check) via `waybill::parity::*`
//!
//! Every other module (`cli`, `generate`, `resolve`, `enrich`,
//! `scan_fs`, `trace`, `attestation`, `policy`, `sbom`, `error`,
//! `config`) is intentionally NOT exposed here — they remain
//! binary-internal per Constitution Principle VI. Adding a new
//! module to this lib root is a deliberate decision that should
//! match the same pattern as `parity`: small, pure-data + pure-
//! function code that benefits from being importable by tests.
//!
//! Note (milestone 055): the Go transitive-edges resolver lives in
//! `scan_fs::package_db::golang::graph_resolver`, which the binary
//! consumes via `mod scan_fs` in main.rs. Wiremock-backed integration
//! tests for the resolver live alongside the resolver
//! (`graph_resolver::wiremock_integration`), NOT under
//! `waybill-cli/tests/`, because exposing scan_fs here would
//! cascade-require lib-exposing every binary-internal module
//! (`trace`, `generate`, `resolve`, ...). See
//! `waybill-cli/tests/go_transitive_edges.rs` for the pointer.

pub mod parity;

// Feature 229 US3: effective waybill version string — TEST-VISIBLE
// mirror of the bin's `crate::version::VERSION`. Both bin and lib
// `include!()` the same generated file at `$OUT_DIR/waybill_version.rs`
// written by `waybill-cli/build.rs`, ensuring the const's value is
// cache-invalidated when `WAYBILL_VERSION` changes (via build.rs's
// `rerun-if-env-changed` — which does NOT work for `option_env!()`
// directly). The `pub const VERSION: &str = "..."` declaration is
// inside the generated file, so it's part of this lib crate's public
// surface. Reference: feature 229 FR-005, FR-012.
include!(concat!(env!("OUT_DIR"), "/waybill_version.rs"));

/// Milestone 072: cross-tier SBOM binding — pure-data + pure-function code
/// for computing binding hashes, verifying bindings, and serializing the
/// `waybill:source-document-binding` annotation. Exposed at lib root so
/// integration tests under `tests/` can call `compute_binding_hash` and
/// `verify_binding` directly. Per Constitution Principle VI, only pure-
/// data + pure-function code lives here; the CLI subcommand wiring
/// (`verify-binding`, `--bind-to-source`) stays binary-internal in
/// `cli/`.
pub mod binding;

/// Milestone 105 (originally milestone 075): shared identifier-handling
/// utilities. Currently exposes `sanitize::sanitize_userinfo` and
/// `sanitize::redact_userinfo_for_log` — pure-function helpers that
/// strip RFC 3986 userinfo from candidate URLs before they appear in
/// any emitted SBOM. Exposed at lib root because both `binding/identifiers/`
/// (the source-tier/build-tier identifier auto-detection from milestone 075)
/// AND the milestone-105 C/C++ readers (`scan_fs/package_db/{west,
/// git_submodule, ...}`) call into it. Per Constitution Principle VI, only
/// pure-function code lives here; no I/O, no state.
pub mod identifiers;

/// Internal testing utilities shared between src-side `#[cfg(test)]`
/// blocks and integration test binaries under `tests/`. Currently
/// exposes `testing::EnvGuard`, a save-and-restore RAII guard that
/// serializes env-var-mutating tests inside a single binary process
/// (fixes the flake class documented by memories
/// `reference_podman_test_flake` + `reference_m205_cargo_metadata_env_flake`).
/// Exposed at lib root because integration test binaries can't see
/// `#[cfg(test)]`-only items in the parent crate.
pub mod testing;
