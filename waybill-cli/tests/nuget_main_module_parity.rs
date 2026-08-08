//! Integration test for milestone 230 (NuGet main-module + root→direct edges).
//!
//! Companion to the unit tests colocated with the NuGet reader. This test
//! spawns the `waybill sbom scan` binary against the shared
//! `tests/fixtures/golden_inputs/nuget/packages_lock_present` fixture and
//! asserts the end-to-end SBOM shape:
//!
//! - SC-001 + SC-002 — every NuGet package component whose lockfile
//!   entry_type is `Direct` (or `CentralTransitive`) has ≥1 incoming
//!   dependency edge from a main-module component. Under the pre-m230
//!   reader, `MikebomFixture.SampleLib` had ZERO incoming edges — the
//!   exact orphan pattern the reporter surfaced against `dotnet/eShop`.
//! - SC-003 — the pre-m230 package-component set is preserved (no PURL
//!   deleted, renamed, or version-shifted; only main-modules are new).
//! - SC-004 — the `waybill:graph-completeness-reason` document-scope
//!   annotation no longer contains `multi-ecosystem-partial-root: nuget`.
//! - SC-005 — an unlocked-fixture scan produces the same root→direct
//!   topology as the locked fixture, distinguished only by the design-
//!   tier signal on the packages.
//!
//! Fixture names use the synthetic `MikebomFixture.*` prefix per memory
//! `feedback_fixture_synthetic_package_names` (real coordinates trip
//! Kusari Inspector).

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn fixture(subdir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("nuget")
        .join(subdir)
}

fn run_scan(path: &std::path::Path) -> serde_json::Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

/// Returns the set of component `bom-ref` values that appear at least once
/// as a target in any `dependencies[].dependsOn[]` list — i.e., have ≥1
/// incoming dependency edge.
fn refs_with_incoming_edges(json: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    if let Some(deps) = json["dependencies"].as_array() {
        for d in deps {
            if let Some(targets) = d["dependsOn"].as_array() {
                for t in targets {
                    if let Some(s) = t.as_str() {
                        out.insert(s.to_string());
                    }
                }
            }
        }
    }
    out
}

fn nuget_components(json: &serde_json::Value) -> Vec<&serde_json::Value> {
    json["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:nuget/"))
                .unwrap_or(false)
        })
        .collect()
}

fn is_main_module(component: &serde_json::Value) -> bool {
    component["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| {
            p["name"].as_str() == Some("waybill:component-role")
                && p["value"].as_str() == Some("main-module")
        })
}

#[test]
fn main_module_reaches_lockfile_direct_dep() {
    // SC-001 + SC-002 — the pre-m230 reader emitted MikebomFixture.SampleLib
    // as a Direct lockfile entry with ZERO incoming edges. Post-m230, a
    // main-module component MUST edge to it.
    let json = run_scan(&fixture("packages_lock_present"));
    let has_incoming = refs_with_incoming_edges(&json);
    let sample_lib_ref = json["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["purl"].as_str() == Some("pkg:nuget/MikebomFixture.SampleLib@1.2.4")
        })
        .expect("SampleLib component present")
        ["bom-ref"]
        .as_str()
        .expect("bom-ref")
        .to_string();
    assert!(
        has_incoming.contains(&sample_lib_ref),
        "SampleLib (Direct lockfile entry) still orphaned post-m230; \
         incoming-edge set: {:?}",
        has_incoming
    );
}

#[test]
fn one_main_module_per_project_locked_fixture() {
    // SC-001 — the locked fixture has exactly one .csproj so exactly one
    // main-module component MUST be emitted. When there's a single
    // main-module for the scan, m127's root selector promotes it to
    // `metadata.component` (the document subject) rather than listing
    // it in `components[]` — matching every other single-main-module
    // ecosystem's emission shape.
    let json = run_scan(&fixture("packages_lock_present"));
    let subject = &json["metadata"]["component"];
    assert!(
        is_main_module(subject),
        "metadata.component missing main-module role; got: {}",
        subject
    );
    // App.csproj has no <Version>; falls through to pkg:generic per FR-010.
    assert_eq!(
        subject["purl"].as_str(),
        Some("pkg:generic/App@0.0.0"),
        "unversioned main-module should use pkg:generic fallback"
    );
    // In-array main-modules (none expected — single-project scan
    // promotes the main-module to metadata.component).
    let in_array: Vec<_> = json["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| is_main_module(c))
        .collect();
    assert!(
        in_array.is_empty(),
        "single-project scan should not duplicate main-module into components[]; got {:?}",
        in_array
    );
}

#[test]
fn preserves_pre_m230_package_component_purls() {
    // SC-003 — the pre-m230 NuGet package component set is a strict
    // subset of the post-m230 set (m230 only adds; never renames or
    // deletes). Check every package component the pre-m230 reader
    // would have emitted for this fixture is still present.
    let json = run_scan(&fixture("packages_lock_present"));
    let purls: std::collections::BTreeSet<_> = nuget_components(&json)
        .iter()
        .filter(|c| !is_main_module(c))
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    // Same assertions as scan_nuget.rs::packages_lock_overrides_csproj_and_emits_transitives.
    assert!(
        purls.contains("pkg:nuget/MikebomFixture.SampleLib@1.2.4"),
        "SampleLib@1.2.4 (lockfile-resolved) missing post-m230; \
         got {:?}",
        purls
    );
    assert!(
        purls.contains("pkg:nuget/MikebomFixture.SubDep@0.5.0"),
        "SubDep@0.5.0 (transitive) missing post-m230; got {:?}",
        purls
    );
}

#[test]
fn graph_completeness_no_longer_flags_nuget_partial_root() {
    // SC-004 — the pre-m230 reader emitted no main-modules, so the
    // graph-completeness classifier at bfs.rs:87 lacked a per-ecosystem
    // root for nuget and fired `multi-ecosystem-partial-root: nuget`.
    // Post-m230 the annotation MUST NOT contain that substring.
    let json = run_scan(&fixture("packages_lock_present"));
    let reason = json["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some("waybill:graph-completeness-reason"))
        .and_then(|p| p["value"].as_str())
        .unwrap_or("");
    assert!(
        !reason.contains("multi-ecosystem-partial-root: nuget"),
        "graph-completeness still flags nuget as partial-root post-m230; \
         reason value: {:?}",
        reason
    );
}

#[test]
fn unlocked_fixture_still_edges_root_to_direct() {
    // SC-005 — csproj_legacy has no packages.lock.json; the design-tier
    // fallback (US2) MUST still emit main-module edges to the declared
    // <PackageReference> targets.
    let json = run_scan(&fixture("csproj_legacy"));
    let has_incoming = refs_with_incoming_edges(&json);
    let sample_ref = json["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["purl"].as_str() == Some("pkg:nuget/MikebomFixture.SampleLib@1.2.3")
        })
        .expect("SampleLib component present")
        ["bom-ref"]
        .as_str()
        .expect("bom-ref")
        .to_string();
    assert!(
        has_incoming.contains(&sample_ref),
        "unlocked SampleLib still orphaned post-m230; \
         incoming-edge set: {:?}",
        has_incoming
    );
}
