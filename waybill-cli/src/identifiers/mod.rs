//! Shared identifier-handling utilities used across the waybill-cli
//! binary. Currently exposes one submodule:
//!
//! * [`sanitize`] — milestone 105 (originally milestone 075): URL
//!   credential redaction. Strips RFC 3986 userinfo from candidate URLs
//!   before they appear in any emitted SBOM (`pkg:git+https://`,
//!   `waybill:download-url`, etc.) and emits a `tracing::warn!` event
//!   so operators can fix manifests that contain secrets.
//!
//! Milestone 105 promoted the helper from `binding/identifiers/auto_detect.rs`'s
//! module-private state to a shared utility so the six new C/C++
//! readers (`west`, `idf_component`, `git_submodule`, plus
//! cmake/conan/vcpkg extensions) can call it without duplicating
//! the logic per FR-016.

pub mod sanitize;
