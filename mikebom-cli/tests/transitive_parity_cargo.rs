//! Cargo transitive-parity regression test — milestone 083 (issue #111).
//!
//! Fixture: clap-rs/clap @ v4.5.21 (commit
//! `2920fb082c987acb72ed1d1f47991c4d157e380d`). Manifest + lockfile
//! only per spec FR-002 + Q1 clarification.
//!
//! Per FR-007 + research §1: pinned tool versions are trivy 0.69.3 +
//! syft 1.27.0. Per research §5: graceful-skip when external tools
//! are missing unless `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1` is set.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "cargo";

/// Pinned at the alpha.26 baseline (post-milestone-087 release —
/// the cargo workspace-member version-disambiguation fix landed in
/// this baseline). Bump per quickstart.md Recipe 3 only when a
/// deliberate per-ecosystem-reader change shifts the count.
///
/// Milestone 179: 317 → 316. The clap fixture declares
/// `clap_derive = { path = "./clap_derive", version = "=4.5.18",
/// optional = true }` in its `[dependencies]` table. Post-m179 the
/// cargo reader classifies `clap_derive` as `LifecycleScope::Optional`;
/// the SPDX 2.3 emitter reversed-direction-emits the `clap →
/// clap_derive` edge as `clap_derive OPTIONAL_DEPENDENCY_OF clap`.
/// The transitive-parity extractor at `transitive_parity_common/mod.rs`
/// counts only generic `DEPENDS_ON` / `DEPENDENCY_OF` verbs (the
/// cross-tool lingua franca — trivy/syft don't emit typed dep-scope
/// verbs at all), so the m179 downgrade drops this edge from
/// mikebom's audited edge set. The clap_derive component itself
/// remains present in the SBOM; its outgoing edges (to heck,
/// proc-macro2, quote, syn per m088 pin) also remain since they're
/// declared with regular runtime scope. This is the pico filter-
/// parity fix landing on a real fixture: consumers walking the SPDX
/// 2.3 typed dep-scope verbs now correctly identify clap_derive as
/// not-in-production-build.
const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 316;

/// Representative edges that mikebom **actually emits** today — pinning
/// current behavior so future milestones can't silently regress.
/// Per FR-010 the audit is observation-only: spot-check edges reflect
/// what mikebom does, not what it should ideally do. The audit's
/// `cross_tool_parity_check` test surfaces divergences from trivy /
/// syft for follow-up triage; this baseline test catches mikebom
/// regressing AGAINST ITSELF.
///
/// Closed by milestone 087:
/// - The `clap@4.5.21 → clap_builder@4.5.9` wrong-version edge is
///   gone. mikebom now correctly emits `→ clap_builder@4.5.21`,
///   pinned below as the post-087 invariant.
///
/// Closed by milestone 088 (locked-down by milestone 087's fix):
/// - `clap_derive@4.5.18` (a workspace-member proc-macro crate) emits
///   its 4 declared outgoing edges (heck, proc-macro2, quote, syn) per
///   `Cargo.lock`. Pre-087 mikebom emitted ZERO outgoing edges from
///   `clap_derive` because the workspace-member skip dropped the
///   source crate from the component set. Pinned below as the
///   post-088 invariant; future cargo-reader changes that drop
///   proc-macro outgoing edges fail this test.
const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Confirmed in mikebom output — clap workspace root depends on automod.
    ("pkg:cargo/clap", "pkg:cargo/automod"),
    // Confirmed — anstream emits style-stripper deps.
    ("pkg:cargo/anstream", "pkg:cargo/anstyle"),
    // Confirmed — terminal_size emits libc on unix.
    ("pkg:cargo/terminal_size", "pkg:cargo/rustix"),
    // Milestone 087 invariant — workspace-member version disambiguation:
    // clap@4.5.21 must depend on clap_builder@4.5.21 (NOT 4.5.9). The
    // PURL-prefix matcher strips `@<version>` so the test still
    // requires both endpoints to be present + correctly emitted.
    ("pkg:cargo/clap", "pkg:cargo/clap_builder"),
    // Milestone 088 invariant — proc-macro outgoing-edge pin (closes
    // #173): clap_derive@4.5.18 is a workspace-member proc-macro crate;
    // its 4 declared deps in Cargo.lock MUST emit as outgoing
    // DEPENDS_ON edges. Pre-087 mikebom emitted zero edges from
    // clap_derive because the workspace-member skip removed the
    // source crate from the component set.
    ("pkg:cargo/clap_derive", "pkg:cargo/heck"),
    ("pkg:cargo/clap_derive", "pkg:cargo/proc-macro2"),
    ("pkg:cargo/clap_derive", "pkg:cargo/quote"),
    ("pkg:cargo/clap_derive", "pkg:cargo/syn"),
];

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("Cargo.toml").exists(), "missing Cargo.toml at {}", f.display());
    assert!(f.join("Cargo.lock").exists(), "missing Cargo.lock at {}", f.display());
}

#[test]
fn transitive_edges_match_baseline() {
    // mikebom is always available (it's the test binary itself).
    let mikebom_edges = run_mikebom(&fixture());
    assert_eq!(
        mikebom_edges.len(),
        EXPECTED_MIKEBOM_EDGE_COUNT,
        "mikebom edge count drifted from the alpha.24 baseline. \
         Investigate per quickstart.md Recipe 3 + bump EXPECTED_MIKEBOM_EDGE_COUNT \
         only after confirming the change is intended."
    );

    // Spot-check at least one representative edge per the FR-007
    // baseline pin. PURL-prefix match (without versions) since
    // alpha.24 versions inside the fixture are stable but bumping
    // them in a future fixture refresh shouldn't break this test.
    let edge_set: std::collections::HashSet<(String, String)> = mikebom_edges
        .iter()
        .map(|e| {
            (
                strip_version(&e.from).to_string(),
                strip_version(&e.to).to_string(),
            )
        })
        .collect();
    for (from_prefix, to_prefix) in EXPECTED_REPRESENTATIVE_EDGES {
        assert!(
            edge_set.contains(&(from_prefix.to_string(), to_prefix.to_string())),
            "expected representative edge missing: {from_prefix} → {to_prefix}"
        );
    }
}

#[test]
fn cross_tool_parity_check() {
    if let Some(reason) = maybe_skip(&["trivy", "syft"]) {
        eprintln!("transitive_parity_cargo::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);

    eprintln!("\n=== cargo audit (clap-rs/clap @ v4.5.21) ===");
    eprintln!(
        "edge counts: mikebom={} trivy={} syft={}",
        mikebom.len(),
        trivy.len(),
        syft.len()
    );
    eprintln!("diff:\n{}", format_edge_diff(&diff));

    // The audit is observation-only per FR-010; we don't fail when
    // tools disagree, we just log the divergence so research.md and
    // any follow-up issue have evidence. The MUST-pass invariant is
    // the per-tool count + representative-edge sample (covered by
    // `transitive_edges_match_baseline`). Future milestones may
    // promote this to a hard parity assertion once each ecosystem's
    // findings are triaged.
}

#[test]
fn graceful_skip_when_tools_absent() {
    // Sanity: running maybe_skip with a tool that doesn't exist
    // returns Some(reason) when strict mode is off. Already covered
    // by transitive_parity_common_sanity but repeated here for
    // per-ecosystem documentation.
    std::env::remove_var("MIKEBOM_REQUIRE_TRANSITIVE_PARITY");
    assert!(maybe_skip(&["this-tool-definitely-does-not-exist"]).is_some());
}

/// Strip the `@<version>` suffix from a PURL for representative-edge
/// matching that survives version bumps in the fixture.
fn strip_version(purl: &str) -> &str {
    match purl.rfind('@') {
        Some(i) => &purl[..i],
        None => purl,
    }
}
