//! Maven transitive-parity regression test — milestone 083.
//!
//! Fixture: apache/commons-lang @ rel/commons-lang-3.14.0 (commit
//! `c8774fa`). Manifest only — commons-lang has no parent POM
//! resolution needed at this version (it's the top of its own
//! release lineage).
//!
//! Audit finding (mikebom-side gap): mikebom emits only 1 dep edge
//! from this fixture, with a malformed version string
//! (`commons-lang3@64`). This appears to be a pom.xml reader
//! extracting the wrong version field — possibly a property
//! reference that resolves to a build-system value rather than the
//! library's GAV. Pinning current behavior pending a follow-up
//! cycle to fix the Maven reader's version resolution. See
//! research.md §8 for the full per-tool comparison.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "maven";

/// **Cache-empty baseline** — pinned at the CI-reproducible state where
/// `$M2_REPO` is empty. Mikebom's maven reader needs cached parent
/// POMs in `$M2_REPO` to resolve transitive dep declarations; with
/// the cache empty, the reader extracts ZERO transitive edges.
/// Real-world output on a developer's box with `~/.m2/` populated
/// will be a small number of edges (1 was observed in the audit run
/// from a populated cache; the actual count depends on which POMs
/// happen to be cached). The 0-edge cache-empty count is what CI
/// sees. Future Maven-reader work that adds parent-POM resolution
/// without cache hits (or via deps.dev fallback per research §2) will
/// bump this.
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
