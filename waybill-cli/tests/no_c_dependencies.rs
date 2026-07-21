//! Principle I regression test (milestone 003, T001).
//!
//! waybill's constitution (Principle I) mandates zero C source files or
//! C-toolchain dependencies anywhere in the build pipeline. Historically,
//! the risk surface has been feature-flag-driven: adding a dependency
//! like `zip`, `flate2`, or `rusqlite` can silently pull in a C backend
//! via a default feature we didn't audit.
//!
//! This test shells out to `cargo tree` and asserts the dependency
//! graph contains none of the known C-backed crate names. If a future
//! `cargo update` or new-feature enablement introduces one of these,
//! CI fails here — before any reviewer has to read the full tree
//! diff in a PR.
//!
//! The blacklist is intentionally broad: any crate name containing
//! `libz`, `zlib`, `c-bindings`, or `libsqlite3` is forbidden.
//! Currently-clean state as of milestone 003:
//!   - `flate2` uses `miniz_oxide` (pure Rust) via its default
//!     `rust_backend` feature.
//!   - `zip` uses `deflate-miniz` (pinned explicitly in Cargo.toml)
//!     which routes through the same pure-Rust `flate2` backend.
//!   - `object` has no C deps in its default-features-off minimal
//!     configuration we use.
//!   - `quick-xml` is pure Rust.
//!
//! When this test fails:
//! 1. Run `cargo tree -p waybill -e normal` locally.
//! 2. Identify which new crate triggered the match.
//! 3. Find the feature flag that pulled it in; either disable the
//!    feature or find a pure-Rust alternative.
//! 4. If no alternative exists, propose a constitution amendment
//!    before proceeding.

use std::process::Command;

const BLACKLIST: &[&str] = &[
    "libz-sys",
    "zlib-sys",
    "zlib-ng-sys",
    "libsqlite3-sys",
    "openssl-sys",
    "c-bindings",
    // Milestone 031 — when adding the optional `oci-registry` feature
    // we discovered that newer oci-client versions transitively bring
    // in `aws-lc-sys` (a `*-sys` crate wrapping the AWS-LC C library)
    // via newer rustls's switched-default crypto provider. We pinned
    // `oci-client = "0.12"` to stay on rustls's older ring-based
    // default. Adding `aws-lc-sys` to the blacklist locks in that
    // decision: future oci-client version bumps that pull aws-lc
    // back fail this test at PR time. See
    // specs/031-oci-registry-image-scan/spec.md FR-005 / SC-005.
    "aws-lc-sys",
];

fn assert_no_c_deps(extra_args: &[&str], profile_label: &str) {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut cmd = Command::new("cargo");
    cmd.arg("tree").arg("-p").arg("waybill").arg("-e").arg("normal");
    for a in extra_args {
        cmd.arg(a);
    }
    let output = cmd
        .current_dir(manifest_dir)
        .output()
        .expect("cargo tree should run");
    assert!(
        output.status.success(),
        "cargo tree {profile_label} failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8_lossy(&output.stdout);
    let matches: Vec<&str> = tree
        .lines()
        .filter(|line| BLACKLIST.iter().any(|needle| line.contains(needle)))
        .collect();
    assert!(
        matches.is_empty(),
        "Principle I violation in {profile_label} tree: C-backed crate found:\n{}\n\
         Full blacklist: {:?}\n\
         See specs/003-multi-ecosystem-expansion/tasks.md T001 for the rationale.",
        matches.join("\n"),
        BLACKLIST,
    );
}

#[test]
fn no_c_dependencies_in_tree() {
    assert_no_c_deps(&[], "default-profile");
}

/// Milestone 031 — also audit the feature-on tree so future bumps of
/// oci-client (or other gated deps) that re-introduce a C-backed
/// crate fail this test at PR time. The default-profile audit above
/// would not catch them since the feature-gated deps don't appear
/// when `cargo tree` is invoked without `--features`.
#[test]
fn no_c_dependencies_in_oci_registry_feature_tree() {
    assert_no_c_deps(&["--features", "oci-registry"], "oci-registry-feature");
}
