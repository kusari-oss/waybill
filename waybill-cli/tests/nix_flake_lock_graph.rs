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

/// FR-008 — an input declared by another input keeps its edge from that
/// declarer and is NOT re-parented to the project.
#[test]
fn a_transitively_declared_input_is_not_re_parented_to_the_root() {
    let d = project_with_flake("nested-inputs");
    let v = scan(d.path(), "cyclonedx-json", "json");
    let root = v["metadata"]["component"]["bom-ref"].as_str().unwrap();

    let root_targets: BTreeSet<&str> = v["dependencies"].as_array().unwrap().iter()
        .find(|x| x["ref"].as_str() == Some(root))
        .and_then(|x| x["dependsOn"].as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str()).collect())
        .unwrap_or_default();

    let nix_from_root: Vec<&str> = root_targets.iter().copied()
        .filter(|t| t.starts_with("pkg:github/")).collect();
    assert_eq!(
        nix_from_root.len(), 1,
        "the fixture's root declares ONE input (flake-parts); nixpkgs is reached \
         through it via a follows alias. Two root edges would mean the nested \
         input was flattened onto the project. got {nix_from_root:?}"
    );
}
