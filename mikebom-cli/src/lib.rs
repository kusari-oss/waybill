//! Library-crate root for mikebom-cli.
//!
//! mikebom-cli is canonically a binary crate (`src/main.rs` is the
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
//!     * `src/cli/parity_cmd.rs` (US3 — the `mikebom sbom
//!       parity-check` diagnostic) via `crate::parity::*`
//!     * `mikebom-cli/tests/holistic_parity.rs` (US1 holistic
//!       parity test) via `mikebom::parity::*`
//!     * `mikebom-cli/tests/mapping_doc_bidirectional.rs` (US2
//!       auto-discovery + reverse check) via `mikebom::parity::*`
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
//! `mikebom-cli/tests/`, because exposing scan_fs here would
//! cascade-require lib-exposing every binary-internal module
//! (`trace`, `generate`, `resolve`, ...). See
//! `mikebom-cli/tests/go_transitive_edges.rs` for the pointer.

pub mod parity;

/// Milestone 072: cross-tier SBOM binding — pure-data + pure-function code
/// for computing binding hashes, verifying bindings, and serializing the
/// `mikebom:source-document-binding` annotation. Exposed at lib root so
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
