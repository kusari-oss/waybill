//! Issue #954 — a project's own declared license must reach its SBOM.
//!
//! Licenses otherwise arrive only through enrichment (deps.dev /
//! ClearlyDefined), and the root component is the LOCAL project — usually not a
//! published package, so there is no registry record to enrich from. The
//! reference case: a library declaring `license: MIT` whose own name 404s on
//! Hackage. Read at scan time or absent from the document permanently.
//!
//! A dependency's license can be recovered later by any consumer holding its
//! PURL. The root's cannot.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str { env!("CARGO_BIN_EXE_waybill") }

fn tree(license_line: Option<&str>) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let lic = license_line.map(|l| format!("license:        {l}\n")).unwrap_or_default();
    std::fs::write(
        d.path().join("app.cabal"),
        format!("name: waybill-fixture-app\nversion: 1.0\n{lic}\n\
                 library\n  build-depends: waybill-fixture-alpha\n"),
    ).unwrap();
    d
}

fn scan(root: &Path, fmt: &str, ext: &str) -> serde_json::Value {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("m954-{}-{seq}.{ext}", std::process::id()));
    let st = Command::new(binary_path())
        .args(["sbom", "scan", "--path", root.to_str().unwrap(), "--offline",
               "--format", fmt, "--output", out.to_str().unwrap()])
        .status().unwrap();
    assert!(st.success(), "scan failed: {st:?}");
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

fn root_licenses(v: &serde_json::Value) -> Vec<String> {
    v["metadata"]["component"]["licenses"].as_array().map(|a| {
        a.iter().filter_map(|l| l["license"]["id"].as_str()
            .or_else(|| l["license"]["name"].as_str())
            .or_else(|| l["expression"].as_str())
            .map(str::to_string)).collect()
    }).unwrap_or_default()
}

/// The `license.id` values only — the slot CycloneDX reserves for *listed* SPDX
/// identifiers. A value that is not on the SPDX list must never appear here.
fn root_license_ids(v: &serde_json::Value) -> Vec<String> {
    v["metadata"]["component"]["licenses"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l["license"]["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The `license.name` values only — the slot for a license that is NOT a listed
/// SPDX identifier.
fn root_license_names(v: &serde_json::Value) -> Vec<String> {
    v["metadata"]["component"]["licenses"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l["license"]["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The reported case: a declared license reaches the root component.
#[test]
fn the_root_component_carries_the_declared_license_m954() {
    let d = tree(Some("MIT"));
    assert_eq!(
        root_licenses(&scan(d.path(), "cyclonedx-json", "json")),
        vec!["MIT".to_string()],
        "the manifest declares MIT and the root is the local project — enrichment \
         has no registry record to supply it from, so absence here is permanent"
    );
}

/// SPDX 2.3 puts it in `licenseDeclared`, not `licenseConcluded`. The project
/// declared it; waybill did not conclude it, and saying otherwise would
/// overstate what the scan established.
#[test]
fn spdx_records_it_as_declared_not_concluded_m954() {
    let d = tree(Some("MIT"));
    let v = scan(d.path(), "spdx-2.3-json", "spdx.json");
    let pkg = v["packages"].as_array().unwrap().iter()
        .find(|p| p["name"].as_str() == Some("waybill-fixture-app"))
        .expect("root package present");
    assert_eq!(pkg["licenseDeclared"].as_str(), Some("MIT"));
    assert_eq!(
        pkg["licenseConcluded"].as_str(), Some("NOASSERTION"),
        "reading a declaration is not concluding one"
    );
}

/// A multi-operand expression must not be truncated at the first space.
///
/// The obvious regex (`^license:\s*(\S+)`, matching how `name:` and `version:`
/// are read) would capture `BSD-3-Clause` and silently drop `OR Apache-2.0` —
/// narrowing the project's own licensing claim to something it did not say.
#[test]
fn a_multi_operand_expression_is_not_truncated_m954() {
    let d = tree(Some("BSD-3-Clause OR Apache-2.0"));
    let got = root_licenses(&scan(d.path(), "cyclonedx-json", "json"));
    assert_eq!(got.len(), 1, "one expression, not one operand: {got:?}");
    assert!(
        got[0].contains("Apache-2.0") && got[0].contains("BSD-3-Clause"),
        "both operands must survive; got {got:?}"
    );
}

/// A string that is not a valid SPDX expression is **preserved**, in the slot for
/// non-listed licenses — never in the identifier slot.
///
/// This test previously asserted the value was *dropped*. That was milestone 957's
/// policy, superseded by #954 FR-004/FR-008a: discarding it left a consumer unable
/// to distinguish "no license declared" from "a license we could not parse", and
/// `AllRightsReserved` is often the most legally significant line in the file.
/// Sibling ecosystems make the case louder — sbt names are free-form (`"Apache 2"`),
/// so under drop-on-failure scala would emit nothing for its common shape.
///
/// The invariant milestone 957 got right is kept and is now asserted *precisely*:
/// an unrecognised string must not be presented as a **listed** identifier.
/// CycloneDX distinguishes the slots — `license.id` is the SPDX-listed enum,
/// `license.name` is for everything else — so preservation and that invariant are
/// not in tension, which the original framing assumed they were.
#[test]
fn a_non_spdx_license_string_is_preserved_but_never_as_an_identifier_m954() {
    let d = tree(Some("AllRightsReserved"));
    let doc = scan(d.path(), "cyclonedx-json", "json");

    assert!(
        root_license_ids(&doc).iter().all(|g| g != "AllRightsReserved"),
        "an unverified string must never occupy the SPDX-listed `license.id` slot: {:?}",
        root_license_ids(&doc)
    );
    assert_eq!(
        root_license_names(&doc),
        vec!["AllRightsReserved".to_string()],
        "...but it must survive in the non-listed `license.name` slot rather than \
         being discarded (FR-004)"
    );
}

/// No declaration means no license. Nothing is invented for a manifest that
/// says nothing.
#[test]
fn a_manifest_with_no_license_emits_none_m954() {
    let d = tree(None);
    assert!(root_licenses(&scan(d.path(), "cyclonedx-json", "json")).is_empty());
}
