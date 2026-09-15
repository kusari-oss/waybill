//! Milestone 867 (#886) — US3: a declared dependency that resolves to
//! nothing must be visible in the emitted document, not merely absent.
//!
//! Silence is what let the ecosystem-mismatch defect survive across
//! releases and two readers, so these assert the signal itself rather than
//! the edges.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn scan(dir: &Path, out: &Path) -> Value {
    let fake_home = tempfile::tempdir().expect("fake-home");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .env("HOME", fake_home.path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(dir)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out.display()))
        .arg("--root-name")
        .arg("fixture")
        .arg("--root-version")
        .arg("1")
        .status()
        .expect("waybill invokes");
    assert!(status.success(), "waybill failed for {}", dir.display());
    serde_json::from_slice(&std::fs::read(out).expect("read output")).expect("parse cdx")
}

fn doc_count(doc: &Value) -> Option<String> {
    doc["metadata"]["properties"].as_array().and_then(|props| {
        props
            .iter()
            .find(|p| p["name"] == "waybill:unresolved-declared-dep-count")
            .and_then(|p| p["value"].as_str().map(str::to_string))
    })
}

/// A `Gemfile.lock` declaring one gem that is present in the scan and one
/// that is not.
fn write_partial_fixture(dir: &Path) {
    std::fs::write(
        dir.join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'waybill-fixture-present'\ngem 'waybill-fixture-absent'\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    waybill-fixture-present (1.0.0)\n\n\
         PLATFORMS\n  ruby\n\nDEPENDENCIES\n  waybill-fixture-absent\n  waybill-fixture-present\n",
    )
    .unwrap();
}

#[test]
fn m867_unresolved_declared_dep_is_counted_and_localised() {
    // FR-004 + FR-005 + SC-005, and contract D-4: the name produces no edge
    // and no invented component, but the drop is no longer silent.
    let tmp = tempfile::tempdir().unwrap();
    write_partial_fixture(tmp.path());
    let out = tmp.path().join("out.json");
    let doc = scan(tmp.path(), &out);

    assert_eq!(
        doc_count(&doc).as_deref(),
        Some("1"),
        "exactly one declared name resolved to nothing"
    );

    // D-4: nothing was invented for the unresolvable name.
    let fabricated = doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.contains("waybill-fixture-absent"))
        })
        .count();
    assert_eq!(fabricated, 0, "an unresolvable name must not become a component");

    // C115: localised on the component that declared it.
    let localised: Vec<&str> = doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["properties"].as_array())
        .flatten()
        .filter(|p| p["name"] == "waybill:unresolved-declared-dep")
        .filter_map(|p| p["value"].as_str())
        .collect();
    assert_eq!(
        localised,
        vec!["waybill-fixture-absent"],
        "the requirer must name what it could not resolve"
    );
}

#[test]
fn m867_count_is_emitted_as_zero_when_everything_resolved() {
    // FR-005a + SC-005a. Three states must stay distinguishable:
    // "declared nothing", "all resolved", "declarations went nowhere".
    // An absent field collapses the first two, which is why zero is
    // emitted rather than omitted.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'waybill-fixture-present'\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    waybill-fixture-present (1.0.0)\n\n\
         PLATFORMS\n  ruby\n\nDEPENDENCIES\n  waybill-fixture-present\n",
    )
    .unwrap();
    let out = tmp.path().join("out.json");
    let doc = scan(tmp.path(), &out);

    assert_eq!(
        doc_count(&doc).as_deref(),
        Some("0"),
        "the count must be present and zero, not absent"
    );
    assert!(
        doc["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["properties"].as_array())
            .flatten()
            .all(|p| p["name"] != "waybill:unresolved-declared-dep"),
        "nothing should be localised when everything resolved"
    );
}
