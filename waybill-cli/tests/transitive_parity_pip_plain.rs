//! pip-plain transitive-parity regression test — milestone 083.
//!
//! Fixture: synthetic small `requirements.txt` with 13 common Python
//! packages, no transitive structure encoded (per FR-008 — plain pip
//! `requirements.txt` has no native way to express dep edges; this
//! test confirms ALL 3 SBOM tools agree on emitting zero transitive
//! edges from this fixture).
//!
//! This is the milestone's only "matches expected" classification:
//! the upstream limitation IS the expected behavior. Future tools
//! that synthesize edges from `requirements.txt` heuristically
//! (e.g., querying PyPI for each package's runtime deps) would
//! deviate from this baseline — that's exactly the kind of drift
//! the test is designed to catch.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "pip_plain";

/// Per FR-008: plain `requirements.txt` has no *transitive* structure
/// — Trivy/Syft emit zero edges from this fixture and that
/// upstream-limitation invariant is what `cross_tool_parity_check`
/// warns on below.
///
/// Issue #236 bumped the mikebom baseline from 0 → 13. mikebom's CDX
/// has emitted 13 `metadata.component → <each requirements.txt
/// package>` edges via the primary-dependency fallback since
/// milestone 084 (CDX's `cyclonedx/dependencies.rs:74-99`). Pre-fix
/// SPDX 2.3 had no equivalent and emitted 0, so the baseline reflected
/// SPDX-only behavior. Post-fix the SPDX 2.3 emitter mirrors the CDX
/// primary-dep fallback when `synthesize_root` fires (image scans,
/// OS-package scans, AND Gemfile-only / requirements.txt-only
/// projects that don't carry a main-module annotation), producing 13
/// synth-root → graph-root `DEPENDS_ON` edges in this fixture. These
/// are direct-dep edges (the project depends on each listed package),
/// not synthesized transitive structure, so FR-008's spirit is
/// preserved — what changed is that SPDX now agrees with CDX inside
/// mikebom. mikebom continues to emit MORE edges than Trivy/Syft here
/// by design (they emit zero); the `cross_tool_parity_check` below
/// surfaces that as a WARN.
const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 13;

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("requirements.txt").exists(), "missing requirements.txt at {}", f.display());
}

#[test]
fn transitive_edges_match_baseline() {
    let mikebom_edges = run_mikebom(&fixture());
    assert_eq!(
        mikebom_edges.len(),
        EXPECTED_MIKEBOM_EDGE_COUNT,
        "mikebom emitted edges from a plain requirements.txt fixture — \
         FR-008 expects zero. Investigate before bumping."
    );
}

#[test]
fn cross_tool_parity_check() {
    if let Some(reason) = maybe_skip(&["trivy", "syft"]) {
        eprintln!("transitive_parity_pip_plain::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);
    eprintln!("\n=== pip-plain audit (synthetic 13-pkg requirements.txt) ===");
    eprintln!(
        "edge counts: mikebom={} trivy={} syft={}",
        mikebom.len(),
        trivy.len(),
        syft.len()
    );
    eprintln!("diff:\n{}", format_edge_diff(&diff));
    // Per FR-008: matches expected = unanimous zero. Log if any tool
    // deviates so research.md can record an upstream change.
    if !diff.is_unanimous() {
        eprintln!(
            "WARN: pip-plain expected unanimous zero per FR-008 but \
             one or more tools emitted edges; verify the upstream \
             tools haven't started heuristically synthesizing edges \
             from requirements.txt."
        );
    }
}
