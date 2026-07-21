//! Maven transitive-parity regression test — milestone 083.
//!
//! Fixture: apache/commons-lang @ rel/commons-lang-3.14.0 (commit
//! `c8774fa`). The fixture pom.xml declares a `<parent>` block
//! (commons-parent@64) and inherits its `<groupId>` from the parent;
//! the project's own `<version>` is `3.14.0`.
//!
//! ## Closed by milestone 092 (populated-cache version-extraction)
//!
//! Pre-092, mikebom's pom.xml parser captured the project-level
//! `<version>` only when project-level `<groupId>` was ALSO present.
//! For commons-lang3 (which omits project-level `<groupId>` and
//! inherits it from `<parent>`), `self_coord` was None and the
//! project's own version was discarded — `build_maven_main_module_entry`
//! fell back to the parent's version, emitting
//! `pkg:maven/org.apache.commons/commons-lang3@64` (parent's
//! `commons-parent@64`) instead of `@3.14.0`. Milestone 092 added a
//! `self_version: Option<String>` field to `PomXmlDocument`,
//! populated independently of `self_coord`, and threaded it through
//! `build_maven_main_module_entry` + the property-substitution arms.
//! See specs/092-fix-maven-version-extract/.
//!
//! ## Remaining gap (track 1 of #175 — out of scope for milestone 092)
//!
//! Even post-092, with `$M2_REPO` empty (the CI baseline), mikebom
//! emits ZERO transitive dep edges from this fixture. Maven's parent
//! POM inheritance + property substitution model means a single
//! isolated `pom.xml` can't self-resolve transitive deps without
//! cached or fetched parent POMs. trivy 0.69.3 also emits 0 in the
//! same configuration; syft 1.27.0 emits 8 via DEPENDENCY_OF
//! reverse-direction. A future milestone analogous to milestone-055's
//! Go proxy fetch would close this gap.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "maven";

/// **Cache-empty baseline** — pinned at the CI-reproducible state where
/// `$M2_REPO` is empty. Mikebom's maven reader needs cached parent
/// POMs in `$M2_REPO` to resolve transitive dep declarations; with
/// the cache empty, the reader extracts ZERO transitive edges.
/// Milestone 092 fixed the **version-extraction** bug for the
/// populated-cache case (the main-module emission path now correctly
/// reports `commons-lang3@3.14.0` instead of the parent's `@64`),
/// but the cache-empty zero-edge gap (track 1 of #175) is unchanged
/// here and will require a future milestone analogous to
/// milestone-055's Go proxy fetch.
const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 0;

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("pom.xml").exists(), "missing pom.xml at {}", f.display());
}

#[test]
fn transitive_edges_match_baseline() {
    let mikebom_edges = run_mikebom(&fixture());
    assert_eq!(
        mikebom_edges.len(),
        EXPECTED_MIKEBOM_EDGE_COUNT,
        "mikebom edge count drifted from the alpha.24 baseline. \
         maven currently under-emits — see research.md §8."
    );
}

#[test]
fn cross_tool_parity_check() {
    if let Some(reason) = maybe_skip(&["trivy", "syft"]) {
        eprintln!("transitive_parity_maven::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);
    eprintln!("\n=== maven audit (apache/commons-lang @ 3.14.0) ===");
    eprintln!(
        "edge counts: mikebom={} trivy={} syft={}",
        mikebom.len(),
        trivy.len(),
        syft.len()
    );
    eprintln!("diff:\n{}", format_edge_diff(&diff));
}
