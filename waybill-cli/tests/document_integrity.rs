//! Invariant I2, enforced per-PR across ecosystems (#980).
//!
//! I2 — "every edge endpoint must resolve to a component present in the
//! document" — is named in `generate::graph_completeness` (m860, FR-001,
//! C-3.3) and was asserted there only against a hand-built three-element
//! fixture. Nothing checked it against a real emitted document, and #980 is
//! what that cost: the nixpkgs Haskell pass assigned versions, which rewrote
//! component PURLs, and the PURL is the identity the dependency graph keys
//! on. Every component the feature resolved was disconnected — 66% of the
//! SPDX relationships on one real project.
//!
//! Neither format complains. CycloneDX has no referential-integrity rule for
//! `bom-ref`, and SPDX omits the relationship rather than dangling it, so the
//! document stays schema-valid while being wrong. **No schema or conformance
//! gate in this repo can see this class of defect.** Only an explicit
//! invariant can.
//!
//! #980 also added this as layer 0 of the public-corpus harness, which covers
//! 13 real projects — but that lane is nightly. This suite is the per-PR half:
//! narrower fixtures, every ecosystem that ships one, no network, no external
//! tools, seconds not minutes.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixtures that emit a non-trivial dependency graph offline.
///
/// Each must produce at least one edge — see `emits_edges_or_the_check_is_vacuous`.
/// A fixture whose graph silently empties would make every assertion below
/// pass while checking nothing, which is the failure mode this whole suite
/// exists to prevent.
const FIXTURES: &[(&str, &str)] = &[
    ("cargo", "cargo/root_package_lifecycle"),
    ("cargo-optional", "optional_dep/cargo"),
    ("golang", "golang"),
    ("npm-bun", "bun_lock/minimal_repro"),
    ("python-uv", "uv_lock/multi_source"),
    ("ruby-gemfile", "gemfile_application"),
    ("pants-resolves", "pants_resolve_edges"),
    ("pants-go", "pants_go"),
    ("split-shared", "split_shared_deps"),
    ("produces-binaries", "produces_binaries"),
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(rel: &str) -> PathBuf {
    workspace_root().join("tests/fixtures").join(rel)
}

struct Docs {
    cdx: serde_json::Value,
    spdx2: serde_json::Value,
    spdx3: serde_json::Value,
}

fn scan(label: &str, root: &Path) -> Docs {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx = tmp.path().join("a.cdx.json");
    let s2 = tmp.path().join("a.spdx.json");
    let s3 = tmp.path().join("a.spdx3.json");

    let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
        // `--offline` keeps this hermetic. It also means the nixpkgs Haskell
        // pass degrades rather than resolving, so the specific #980 path is
        // NOT exercised here — `nix_haskell_resolution_m926.rs` covers that
        // with a seeded cache. This suite covers the general invariant.
        .arg("--offline")
        .args(["sbom", "scan", "--path"])
        .arg(root)
        .args(["--format", "cyclonedx-json,spdx-2.3-json,spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .output()
        .expect("spawn waybill");
    assert!(
        out.status.success(),
        "{label}: scan failed\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let read = |p: &Path| -> serde_json::Value {
        let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{label}: read {}: {e}", p.display()));
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{label}: parse {}: {e}", p.display()))
    };
    Docs { cdx: read(&cdx), spdx2: read(&s2), spdx3: read(&s3) }
}

/// Every `bom-ref` a component declares, plus the document's root.
fn cdx_known_refs(cdx: &serde_json::Value) -> std::collections::HashSet<String> {
    let mut refs: std::collections::HashSet<String> = cdx["components"]
        .as_array()
        .map(|a| a.iter().filter_map(|c| c["bom-ref"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    if let Some(root) = cdx["metadata"]["component"]["bom-ref"].as_str() {
        refs.insert(root.to_string());
    }
    refs
}

fn cdx_edges(cdx: &serde_json::Value) -> Vec<(String, String)> {
    cdx["dependencies"]
        .as_array()
        .map(|a| {
            a.iter()
                .flat_map(|e| {
                    let from = e["ref"].as_str().unwrap_or("?").to_string();
                    e["dependsOn"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(move |t| t.as_str().map(|t| (from.clone(), t.to_string())))
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The guard that keeps every other test in this file honest.
///
/// A fixture whose dependency graph silently empties would make the I2
/// assertions pass while examining nothing — the exact shape #980 slipped
/// through.
///
/// It keys on **SPDX 2.3 relationships**, not CycloneDX edges, and that
/// choice is load-bearing rather than arbitrary.
///
/// Measured while building this suite: clearing the resolved relationship set
/// entirely still leaves CycloneDX emitting a root edge to all 8 components of
/// a fixture. That is not a defect — it is the deliberate primary-dependency
/// fallback in `generate::cyclonedx::dependencies` (see its comment and
/// milestone 894 / #894): when the root has no declared outgoing edges, CDX
/// synthesizes root → every component nothing else depends on, so a flat scan
/// still describes something.
///
/// The consequence for testing is the point: because that fallback fires
/// exactly when the graph is empty, a vacuity guard keyed on CDX edges **can
/// never fail**, and would certify an empty graph as healthy. SPDX 2.3
/// relationships carry no such fallback and do track the resolved graph, so
/// they are what the guard has to watch. Verified by mutation — emptying the
/// relationship set fails this test and leaves the CDX edge count untouched.
#[test]
fn emits_relationships_or_the_check_is_vacuous() {
    for (label, rel) in FIXTURES {
        let d = scan(label, &fixture(rel));
        let n = d.spdx2["relationships"].as_array().map(Vec::len).unwrap_or(0);
        // Every document carries a DESCRIBES; a real graph carries more.
        assert!(
            n > 1,
            "{label}: SPDX 2.3 carries {n} relationship(s), so the resolved \
             dependency graph is empty and every integrity assertion over this \
             fixture would pass while checking nothing. Either a reader \
             regressed or the fixture changed — do not silence this by dropping \
             the fixture from FIXTURES"
        );
    }
}

/// Invariant I2 in CycloneDX: a `dependsOn` target that no component declares.
///
/// This is how #980 presented in CDX — the edge survived, pointing at the
/// pre-rewrite PURL.
#[test]
fn i2_cyclonedx_has_no_dangling_edge_endpoint() {
    for (label, rel) in FIXTURES {
        let d = scan(label, &fixture(rel));
        let refs = cdx_known_refs(&d.cdx);
        let dangling: Vec<String> = cdx_edges(&d.cdx)
            .into_iter()
            .filter(|(_, to)| !refs.contains(to))
            .map(|(f, t)| format!("{f} -> {t}"))
            .collect();
        assert!(
            dangling.is_empty(),
            "{label}: invariant I2 violated — {} edge(s) point at a bom-ref no \
             component has:\n  {}\nSomething rewrote a component identity after \
             the edges were built (#980)",
            dangling.len(),
            dangling.join("\n  ")
        );
    }
}

/// Invariant I2 in SPDX 2.3.
///
/// Note the different failure mode: SPDX usually DROPS a relationship whose
/// endpoint moved rather than dangling it, so this passing does not by itself
/// prove the edges survived — it proves none is corrupt. The CycloneDX test
/// above is the one that sees a lost endpoint.
#[test]
fn i2_spdx_23_relationship_endpoints_all_exist() {
    for (label, rel) in FIXTURES {
        let d = scan(label, &fixture(rel));
        let known: std::collections::HashSet<&str> = d.spdx2["packages"]
            .as_array()
            .map(|a| a.iter().filter_map(|p| p["SPDXID"].as_str()).collect())
            .unwrap_or_default();
        let doc_id = d.spdx2["SPDXID"].as_str();
        let mut bad: Vec<String> = Vec::new();
        for r in d.spdx2["relationships"].as_array().into_iter().flatten() {
            for key in ["spdxElementId", "relatedSpdxElement"] {
                if let Some(v) = r[key].as_str() {
                    if v == "NONE" || v == "NOASSERTION" || Some(v) == doc_id {
                        continue;
                    }
                    if !known.contains(v) && !v.starts_with("SPDXRef-File") {
                        bad.push(format!("{key}={v}"));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{label}: SPDX 2.3 endpoints naming no package: {bad:?}");
    }
}

/// Invariant I2 in SPDX 3: a relationship `to`/`from` naming no element.
#[test]
fn i2_spdx_3_relationship_endpoints_all_exist() {
    for (label, rel) in FIXTURES {
        let d = scan(label, &fixture(rel));
        let ids: std::collections::HashSet<&str> = d.spdx3["@graph"]
            .as_array()
            .map(|a| a.iter().filter_map(|e| e["spdxId"].as_str()).collect())
            .unwrap_or_default();
        let mut bad: Vec<String> = Vec::new();
        for e in d.spdx3["@graph"].as_array().into_iter().flatten() {
            if e["type"].as_str() != Some("Relationship") {
                continue;
            }
            if let Some(f) = e["from"].as_str() {
                if !ids.contains(f) {
                    bad.push(format!("from={f}"));
                }
            }
            for t in e["to"].as_array().into_iter().flatten() {
                if let Some(t) = t.as_str() {
                    if !ids.contains(t) {
                        bad.push(format!("to={t}"));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{label}: SPDX 3 endpoints naming no element: {bad:?}");
    }
}
