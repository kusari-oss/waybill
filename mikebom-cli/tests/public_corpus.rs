//! Milestone 195 — Public SBOM Regression Corpus.
//!
//! Opt-in cargo integration test suite that scans a set of public
//! upstream repositories and container images with the released
//! `mikebom` binary and asserts a hybrid two-layer invariant model
//! per target: (1) coarse Rust-defined assertions with class-of-bug-
//! oriented diagnostics; (2) full-SBOM byte-identity golden diff.
//!
//! Gated behind `MIKEBOM_RUN_PUBLIC_CORPUS=1` — the default
//! `cargo test` / `./scripts/pre-pr.sh` invocation MUST NOT clone
//! corpus repos or pull corpus images. Nightly CI and manual
//! `workflow_dispatch` are the primary invocation paths per Q2 spec
//! clarification.
//!
//! See `specs/195-public-corpus-fixtures/` for the full spec /
//! plan / research / data-model / contracts / quickstart.

#![cfg_attr(test, allow(clippy::unwrap_used))]

#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use corpus_harness_195::harness::{env_gate, scan_target, skip_oci_gate, FailureFormat};
use corpus_harness_195::layer2_golden::compare_golden;
use corpus_harness_195::manifest::{Ecosystem, SourceKind, TARGETS};

/// Skeleton test — proves the `public_corpus` test binary compiles
/// end-to-end from the very first commit and that the env-gate
/// helper is wired.
#[test]
fn env_gate_skips_when_unset() {
    // Paranoia test — if `MIKEBOM_RUN_PUBLIC_CORPUS` is not exactly
    // "1", the harness MUST NOT invoke `scan_target`. This test runs
    // even in the default cargo lane; it MUST NOT do any network I/O.
    let expected_gated = std::env::var("MIKEBOM_RUN_PUBLIC_CORPUS").as_deref() == Ok("1");
    assert_eq!(env_gate(), expected_gated);
}

// -----------------------------------------------------------------------
// Per-target tests
// -----------------------------------------------------------------------

fn find_target(name: &str) -> &'static corpus_harness_195::manifest::CorpusTarget {
    TARGETS
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("m195: no target named {name} in TARGETS manifest"))
}

fn run_target(name: &str) {
    if !env_gate() {
        println!("skipping {name}: MIKEBOM_RUN_PUBLIC_CORPUS not set");
        return;
    }
    let target = find_target(name);
    // MIKEBOM_CORPUS_SKIP_OCI gate for image targets.
    if matches!(&target.source, SourceKind::OciImage { .. }) && skip_oci_gate() {
        println!("skipping {name}: MIKEBOM_CORPUS_SKIP_OCI set");
        return;
    }
    let sboms = match scan_target(target) {
        Ok(s) => s,
        Err(infra_err) => panic!("{infra_err}"),
    };
    // Layer 1 — fast-fail with class-of-bug diagnostic.
    if let Err(fail) = (target.layer1)(&sboms) {
        panic!("{fail}");
    }
    // Layer 2 — full-SBOM byte-identity golden diff.
    for fmt in [FailureFormat::Cdx, FailureFormat::Spdx23, FailureFormat::Spdx3] {
        if let Err(fail) = compare_golden(target.name, fmt, &sboms) {
            panic!("{fail}");
        }
    }
}

#[test]
fn corpus_go_cobra() {
    run_target("go-cobra");
}

#[test]
fn corpus_rust_ripgrep() {
    run_target("rust-ripgrep");
}

#[test]
fn corpus_npm_express() {
    run_target("npm-express");
}

#[test]
fn corpus_python_flask() {
    run_target("python-flask");
}

#[test]
fn corpus_maven_guice() {
    run_target("maven-guice");
}

#[test]
fn corpus_image_postgres16() {
    run_target("image-postgres16");
}

// -----------------------------------------------------------------------
// US4 — byte-identity across two consecutive runs (opt-in-within-opt-in)
// -----------------------------------------------------------------------

#[test]
fn byte_identity_across_two_runs() {
    if !env_gate() {
        println!("skipping: MIKEBOM_RUN_PUBLIC_CORPUS not set");
        return;
    }
    if std::env::var("MIKEBOM_RUN_BYTE_IDENTITY_SUITE").as_deref() != Ok("1") {
        println!("skipping: MIKEBOM_RUN_BYTE_IDENTITY_SUITE not set (this test doubles corpus wall-clock)");
        return;
    }
    // Pick one lightweight target (go-cobra) — running all 6 twice
    // exceeds SC-005's 30-min budget.
    let target = find_target("go-cobra");
    let first = scan_target(target).expect("first scan");
    let second = scan_target(target).expect("second scan");
    let a = std::fs::read(&first.paths.cdx).expect("read first cdx");
    let b = std::fs::read(&second.paths.cdx).expect("read second cdx");
    assert_eq!(
        a, b,
        "m195 SC-006 violation — two consecutive scans of {} produced non-byte-identical CDX output",
        target.name
    );
}

// Silence unused-import warnings when the manifest is not fully populated.
#[allow(dead_code)]
fn _unused_hint_for_ecosystem() -> Ecosystem {
    Ecosystem::Go
}
