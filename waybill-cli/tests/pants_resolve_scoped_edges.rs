//! Issue #910 — Pex `requires_dists` edges must resolve inside their own
//! Pants resolve.
//!
//! **The defect.** `scan_fs/mod.rs` keys the edge-resolution index on
//! `(ecosystem, normalized_name)`. A `<name> <version>` disambiguation key
//! exists only for cargo, npm and golang, and no ecosystem carries a resolve
//! dimension at all. Pex gives dependency *names* with no version and no
//! resolve, so when two resolves pin one package at different versions the
//! index holds whichever `db_entries` yielded last and every edge naming that
//! package — in either resolve — points at that one component.
//!
//! **The fixture.** Two declared resolves, one shared package name at two
//! versions, one consumer in each resolve depending on it by bare name:
//!
//! ```text
//! app.lock    consumer-a 1.0.0 -> shared      shared 1.0.0
//! tools.lock  consumer-b 1.0.0 -> shared      shared 2.0.0
//! ```
//!
//! Pre-fix output, captured on `d57235e9`-descended `main`:
//!
//! ```text
//! consumer-a@1.0.0 -> shared@2.0.0   WRONG — app pins 1.0.0
//! consumer-b@1.0.0 -> shared@2.0.0   right, by luck
//! <root>           -> shared@1.0.0   fallback edge masking the orphan
//! ```
//!
//! The third line is why this went unnoticed: the mis-resolved edge orphans
//! `shared@1.0.0`, the root-fallback adopts it, and graph completeness then
//! reports `complete 6/6, orphans 0` over a graph that is wrong. A
//! completeness signal cannot be the thing that catches this.

use std::path::PathBuf;
use std::process::Command;

/// Crate-local at `waybill-cli/tests/fixtures/pants_resolve_edges/`.
/// `common::local_fixture_path` resolves against the *workspace* root
/// (`<repo>/tests/fixtures/`), which is the m090 stay-set for synthetic
/// OS-image data; this fixture is issue-specific and belongs beside the
/// other reader fixtures, so it is computed against `CARGO_MANIFEST_DIR`
/// the way the m667 bun_lock tests do.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pants_resolve_edges")
}

/// Scan the fixture and return the emitted CycloneDX document.
fn scan_cdx() -> serde_json::Value {
    let out = tempfile::tempdir().expect("tempdir");
    let path = out.path().join("actual.cdx.json");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture().to_str().expect("fixture path"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            path.to_str().expect("out path"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");
    serde_json::from_slice(&std::fs::read(&path).expect("read sbom")).expect("parse sbom")
}

/// Every `from -> to` pair in the CDX `dependencies` array.
fn edges(doc: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dep in doc["dependencies"].as_array().into_iter().flatten() {
        let from = dep["ref"].as_str().unwrap_or_default().to_string();
        for to in dep["dependsOn"].as_array().into_iter().flatten() {
            out.push((from.clone(), to.as_str().unwrap_or_default().to_string()));
        }
    }
    out.sort();
    out
}

/// The resolve a component claims, read from the emitted document rather
/// than from anything the reader reports about itself.
fn resolve_of(doc: &serde_json::Value, purl: &str) -> Option<String> {
    doc["components"].as_array()?.iter().find_map(|c| {
        if c["purl"].as_str()? != purl {
            return None;
        }
        c["properties"].as_array()?.iter().find_map(|p| {
            (p["name"].as_str()? == "waybill:pants-resolve")
                .then(|| p["value"].as_str().unwrap_or_default().to_string())
        })
    })
}

const CONSUMER_A: &str = "pkg:pypi/waybill-fixture-consumer-a@1.0.0";
const CONSUMER_B: &str = "pkg:pypi/waybill-fixture-consumer-b@1.0.0";
const SHARED_1: &str = "pkg:pypi/waybill-fixture-shared@1.0.0";
const SHARED_2: &str = "pkg:pypi/waybill-fixture-shared@2.0.0";

/// Guard on the fixture itself. If both `shared` versions stop being emitted
/// as distinct components, the edge assertions below become vacuous — they
/// would pass because there is nothing left to confuse.
#[test]
fn fixture_emits_both_shared_versions_in_distinct_resolves() {
    let doc = scan_cdx();
    assert_eq!(
        resolve_of(&doc, SHARED_1).as_deref(),
        Some("app"),
        "shared@1.0.0 must exist and belong to the `app` resolve"
    );
    assert_eq!(
        resolve_of(&doc, SHARED_2).as_deref(),
        Some("tools"),
        "shared@2.0.0 must exist and belong to the `tools` resolve"
    );
    assert_eq!(resolve_of(&doc, CONSUMER_A).as_deref(), Some("app"));
    assert_eq!(resolve_of(&doc, CONSUMER_B).as_deref(), Some("tools"));
}

/// #910 — the edge must land inside the declaring component's own resolve.
#[test]
fn requires_dists_edges_stay_inside_their_resolve() {
    let doc = scan_cdx();
    let edges = edges(&doc);

    assert!(
        edges.contains(&(CONSUMER_A.to_string(), SHARED_1.to_string())),
        "consumer-a is in resolve `app`, which pins shared 1.0.0, so its edge \
         must point there. Edges emitted:\n{edges:#?}"
    );
    assert!(
        !edges.contains(&(CONSUMER_A.to_string(), SHARED_2.to_string())),
        "consumer-a must NOT reach shared@2.0.0 — that version belongs to the \
         `tools` resolve and this edge crosses a resolve boundary. This is the \
         pre-fix behaviour. Edges emitted:\n{edges:#?}"
    );
    assert!(
        edges.contains(&(CONSUMER_B.to_string(), SHARED_2.to_string())),
        "consumer-b is in resolve `tools`, which pins shared 2.0.0. \
         Edges emitted:\n{edges:#?}"
    );
    assert!(
        !edges.contains(&(CONSUMER_B.to_string(), SHARED_1.to_string())),
        "consumer-b must NOT reach shared@1.0.0. Edges emitted:\n{edges:#?}"
    );
}

/// The masking half. With both `shared` versions reachable from their own
/// consumer, neither needs adopting by the root — so a root edge to either is
/// evidence that a real edge went missing and the fallback covered for it.
#[test]
fn root_does_not_adopt_a_package_its_consumer_should_reach() {
    let doc = scan_cdx();
    let edges = edges(&doc);
    let root = doc["metadata"]["component"]["bom-ref"]
        .as_str()
        .or_else(|| doc["metadata"]["component"]["purl"].as_str())
        .unwrap_or_default()
        .to_string();

    for shared in [SHARED_1, SHARED_2] {
        assert!(
            !edges.contains(&(root.clone(), shared.to_string())),
            "root has a direct edge to {shared}, which means it was orphaned by \
             a mis-resolved dependency edge and the primary-dependency fallback \
             adopted it. That fallback is what let graph completeness report \
             `complete` over a wrong graph. Edges emitted:\n{edges:#?}"
        );
    }
}

/// Cross-format parity. The same resolve-scoping must hold in SPDX 2.3, and
/// comparing the two formats is the cheapest way to catch a fabricated or
/// dropped edge in either.
///
/// SPDX does not spell these the way CycloneDX does, and reading only
/// `DEPENDS_ON` gives a false negative: an edge into a dev-scoped resolve is
/// emitted as `DEV_DEPENDENCY_OF` with the direction reversed
/// (`X DEV_DEPENDENCY_OF Y` means Y depends on X). The `tools` resolve here
/// classifies as dev, so two of this fixture's four package-level edges land
/// under that type. Normalising both before comparing is the point of the
/// test.
#[test]
fn spdx_23_agrees_with_cyclonedx_on_which_shared_version_each_consumer_reaches() {
    let out = tempfile::tempdir().expect("tempdir");
    let path = out.path().join("actual.spdx.json");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture().to_str().expect("fixture path"),
            "--offline",
            "--format",
            "spdx-2.3-json",
            "--output",
            path.to_str().expect("out path"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "spdx scan failed: {status}");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");

    // SPDXID -> "name@version"
    let names: std::collections::HashMap<String, String> = doc["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| {
            (
                p["SPDXID"].as_str().unwrap_or_default().to_string(),
                format!(
                    "{}@{}",
                    p["name"].as_str().unwrap_or_default(),
                    p["versionInfo"].as_str().unwrap_or_default()
                ),
            )
        })
        .collect();

    // Normalise every dependency-bearing relationship to (dependent, dependency).
    let mut edges: Vec<(String, String)> = Vec::new();
    for r in doc["relationships"].as_array().into_iter().flatten() {
        let from = names
            .get(r["spdxElementId"].as_str().unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        let to = names
            .get(r["relatedSpdxElement"].as_str().unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        match r["relationshipType"].as_str().unwrap_or_default() {
            "DEPENDS_ON" => edges.push((from, to)),
            // Reversed by definition.
            "DEV_DEPENDENCY_OF" | "OPTIONAL_DEPENDENCY_OF" | "BUILD_DEPENDENCY_OF"
            | "TEST_DEPENDENCY_OF" => edges.push((to, from)),
            _ => {}
        }
    }

    let a = (
        "waybill-fixture-consumer-a@1.0.0".to_string(),
        "waybill-fixture-shared@1.0.0".to_string(),
    );
    let b = (
        "waybill-fixture-consumer-b@1.0.0".to_string(),
        "waybill-fixture-shared@2.0.0".to_string(),
    );
    assert!(
        edges.contains(&a),
        "SPDX 2.3 must agree that consumer-a reaches shared@1.0.0. Edges:\n{edges:#?}"
    );
    assert!(
        edges.contains(&b),
        "SPDX 2.3 must agree that consumer-b reaches shared@2.0.0. Edges:\n{edges:#?}"
    );
    for wrong in [
        (
            "waybill-fixture-consumer-a@1.0.0".to_string(),
            "waybill-fixture-shared@2.0.0".to_string(),
        ),
        (
            "waybill-fixture-consumer-b@1.0.0".to_string(),
            "waybill-fixture-shared@1.0.0".to_string(),
        ),
    ] {
        assert!(
            !edges.contains(&wrong),
            "SPDX 2.3 has a cross-resolve edge {wrong:?}. Edges:\n{edges:#?}"
        );
    }
}
