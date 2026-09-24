//! Milestone 925 — the input graph, end to end (FR-007, FR-007a, FR-008).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nix").join(name)
}

/// A Haskell project plus a flake, so there is a main module for the inputs to
/// attach to — the shape both measured repositories have.
fn project_with_flake(fixture_name: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("app.cabal"),
        "name: waybill-fixture-app\nversion: 1.0\nlicense: BSD-3-Clause\n\n\
         library\n  build-depends: waybill-fixture-alpha\n",
    ).unwrap();
    std::fs::copy(fixture(fixture_name).join("flake.lock"), d.path().join("flake.lock")).unwrap();
    d
}

fn scan(root: &Path, fmt: &str, ext: &str) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("nix-g-{}-{seq}.{ext}", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", fmt, "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// C-5 / SC-003 — every emitted input is reachable from the document root.
#[test]
fn every_flake_input_is_reachable_from_the_root() {
    let d = project_with_flake("single-github");
    let v = scan(d.path(), "cyclonedx-json", "json");

    let refs: BTreeSet<&str> = v["components"].as_array().unwrap().iter()
        .filter_map(|c| c["bom-ref"].as_str()).collect();
    let root = v["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let reached: BTreeSet<&str> = v["dependencies"].as_array().map(|a| a.as_slice()).unwrap_or(&[])
        .iter()
        .flat_map(|d| d["dependsOn"].as_array().map(|x| x.as_slice()).unwrap_or(&[]))
        .filter_map(|t| t.as_str())
        .collect();

    let orphaned: Vec<&str> = refs.iter().copied()
        .filter(|r| *r != root && !reached.contains(r))
        .filter(|r| r.starts_with("pkg:github/"))
        .collect();
    assert!(
        orphaned.is_empty(),
        "a flake input recorded but unreachable is one most consumers will \
         discard, losing the answer to 'what was this built against'. \
         orphaned={orphaned:?}"
    );
}

/// FR-007a — the project→input edge is BUILD-scoped, not an ordinary
/// dependency.
///
/// Asserting only that an edge exists would pass under the plain `DependsOn`
/// that FR-007a forbids, which is the whole reason this assertion is separate
/// from the reachability one above.
#[test]
fn the_project_to_input_edge_is_build_scoped_not_runtime() {
    let d = project_with_flake("single-github");

    // CycloneDX: a non-runtime scope a consumer can filter on.
    let cdx = scan(d.path(), "cyclonedx-json", "json");
    let scopes: Vec<&str> = cdx["components"].as_array().unwrap().iter()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:github/")))
        .filter_map(|c| c["scope"].as_str())
        .collect();
    assert_eq!(
        scopes, vec!["excluded"],
        "nixpkgs is the build environment, not part of the library's closure; a \
         consumer filtering to runtime must drop it on this signal alone"
    );

    // SPDX 2.3: the native build-dependency semantic.
    let spdx = scan(d.path(), "spdx-2.3-json", "spdx.json");
    let build_edges = spdx["relationships"].as_array().unwrap().iter()
        .filter(|r| r["relationshipType"].as_str() == Some("BUILD_DEPENDENCY_OF"))
        .count();
    assert!(
        build_edges > 0,
        "expected BUILD_DEPENDENCY_OF; a plain DEPENDS_ON would assert the \
         project depends on its flake inputs the way it depends on its libraries"
    );
}

/// FR-007 + FR-008 — the root is attached to exactly the inputs the lockfile's
/// root node declares, and an input reached through another input keeps that
/// edge too.
///
/// The fixture's root declares BOTH `flake-parts` and `nixpkgs`, and
/// `flake-parts` follows the root's `nixpkgs`. An earlier version of this test
/// read that as "the root declares one input" and asserted a single root edge,
/// which locked in a reader that dropped the project's direct nixpkgs edge on
/// every flake whose inputs follow it.
#[test]
fn the_root_is_attached_to_exactly_its_declared_inputs() {
    let d = project_with_flake("nested-inputs");
    let v = scan(d.path(), "cyclonedx-json", "json");
    let root = v["metadata"]["component"]["bom-ref"].as_str().unwrap();

    let targets_of = |r: &str| -> BTreeSet<String> {
        v["dependencies"].as_array().unwrap().iter()
            .find(|x| x["ref"].as_str() == Some(r))
            .and_then(|x| x["dependsOn"].as_array())
            .map(|a| a.iter().filter_map(|t| t.as_str()).map(String::from).collect())
            .unwrap_or_default()
    };

    let nix_from_root: BTreeSet<String> = targets_of(root).into_iter()
        .filter(|t| t.starts_with("pkg:github/")).collect();
    let names: BTreeSet<&str> = nix_from_root.iter()
        .filter_map(|p| p.rsplit('/').next())
        .filter_map(|n| n.split('@').next())
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["flake-parts", "nixpkgs"]),
        "the root node declares flake-parts and nixpkgs; the project gets an edge \
         to each and to nothing else. got {nix_from_root:?}"
    );

    let flake_parts = nix_from_root.iter()
        .find(|p| p.contains("/flake-parts@")).unwrap();
    let nixpkgs = nix_from_root.iter()
        .find(|p| p.contains("/nixpkgs@")).unwrap();
    assert!(
        targets_of(flake_parts).contains(nixpkgs),
        "flake-parts reaches nixpkgs through a follows alias; that edge stays \
         (FR-008) alongside the root's own"
    );
}
