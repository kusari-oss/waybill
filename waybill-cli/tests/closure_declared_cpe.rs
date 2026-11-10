//! Milestone 1377 (#1394) — closure components carry the CPE nixpkgs
//! declares in `meta.identifiers.cpe`, with the curated table as fallback.
//!
//! Measured before this milestone on openssh's buildtime closure: 16
//! name+version pairs carry a nixpkgs-declared CPE. The curated table matched
//! 14 exactly, lacked gnugrep, and encoded bash's update as part of the
//! version (`5.3p9`), which grype matches to nothing.
//!
//! Every assertion is on the emitted documents, through a stub `nix`. The
//! fixture table is in `fixtures/nix_closure_declared_cpe/README.md`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

const VENDOR: &str = "waybill-fixture-vendor";

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_declared_cpe")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn scan_with(rows: Rows<'_>, project: PathBuf, extra: &[&str]) -> Scan {
    let mut args = vec!["--nix-closure"];
    args.extend_from_slice(extra);
    NixStub {
        project,
        closure: &fixture("closure.json"),
        rows,
        attr: DEFAULT_ATTR,
    }
    .scan(&args)
}

fn scan() -> Scan {
    let rows = fixture("rows.json");
    scan_with(Rows::Json(&rows), project(), &[])
}

fn component<'a>(s: &'a Scan, name: &str) -> &'a Value {
    s.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&s.cdx["metadata"]["component"]))
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no component named {name}"))
}

fn cdx_cpe(s: &Scan, name: &str) -> Option<String> {
    component(s, name)["cpe"].as_str().map(str::to_string)
}

/// Every CPE the SPDX 2.3 document gives the named package.
fn spdx23_cpes(s: &Scan, name: &str) -> Vec<String> {
    s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str() == Some(name))
        .flat_map(|p| p["externalRefs"].as_array().cloned().unwrap_or_default())
        .filter(|r| r["referenceType"].as_str().is_some_and(|t| t.starts_with("cpe")))
        .filter_map(|r| r["referenceLocator"].as_str().map(str::to_string))
        .collect()
}

/// Every CPE the SPDX 3 document gives the named package.
fn spdx3_cpes(s: &Scan, name: &str) -> Vec<String> {
    let graph = s.spdx3["@graph"].as_array().unwrap();
    graph
        .iter()
        .filter(|e| e["type"] == "software_Package" && e["name"].as_str() == Some(name))
        .flat_map(|e| e["externalIdentifier"].as_array().cloned().unwrap_or_default())
        .filter_map(|id| {
            let id = match id.as_str() {
                // An IRI reference to an element elsewhere in the graph.
                Some(iri) => graph.iter().find(|e| e["spdxId"].as_str() == Some(iri))?.clone(),
                None => id,
            };
            id["externalIdentifierType"]
                .as_str()
                .is_some_and(|t| t.starts_with("cpe"))
                .then(|| id["identifier"].as_str().map(str::to_string))
                .flatten()
        })
        .collect()
}

fn c184_identifiers(s: &Scan) -> Value {
    s.cdx["metadata"]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "waybill:nix-closure")
        .and_then(|p| p["value"].as_str())
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
        .map(|v| v["identifiers"].clone())
        .expect("a C184 record")
}

fn declared(product: &str, version: &str) -> String {
    format!("cpe:2.3:a:{VENDOR}:{product}:{version}:*:*:*:*:*:*:*")
}

/// CONTROL for the replacement test below: without the meta evaluation,
/// the curated table gives zlib its CPE. Otherwise "replaced" could pass
/// against a build in which the curated table never fired.
#[test]
fn control_without_the_evaluation_the_curated_table_applies() {
    let s = scan_with(Rows::Fail, project(), &[]);
    assert_eq!(
        cdx_cpe(&s, "zlib").as_deref(),
        Some("cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:*")
    );
}

/// FR-005, FR-007 — the declared CPE replaces the curated one and is the
/// component's only CPE, in every format.
#[test]
fn a_declared_cpe_replaces_the_curated_one_and_is_the_only_cpe() {
    let s = scan();
    let want = declared("zlib", "9.9.9");
    assert_eq!(cdx_cpe(&s, "zlib").as_deref(), Some(want.as_str()));
    assert_eq!(spdx23_cpes(&s, "zlib"), vec![want.clone()]);
    assert_eq!(spdx3_cpes(&s, "zlib"), vec![want]);
    assert!(
        !component(&s, "zlib").to_string().contains("cpe:2.3:a:zlib:zlib"),
        "the overridden curated CPE must not be emitted as a second identifier: {}",
        component(&s, "zlib")
    );
}

/// The bash shape: the member's derivation differs from the attribute's, and
/// name+version reach it.
#[test]
fn a_member_with_another_drv_is_reached_by_name_and_version() {
    let s = scan();
    assert_eq!(
        cdx_cpe(&s, "waybill-fixture-shell").as_deref(),
        Some("cpe:2.3:a:waybill-fixture-vendor:waybill-fixture-shell:5.3:9:*:*:*:*:*:*")
    );
}

/// FR-006 — no declared CPE: the curated table applies as before.
#[test]
fn without_a_declared_cpe_the_curated_one_stays() {
    let s = scan();
    assert_eq!(
        cdx_cpe(&s, "openssl").as_deref(),
        Some("cpe:2.3:a:openssl:openssl:9.9.9:*:*:*:*:*:*:*")
    );
}

/// FR-006 — a row with only `cpeParts` (so `cpe: null`) synthesises nothing.
#[test]
fn a_member_with_no_vendor_gets_no_cpe() {
    let s = scan();
    assert_eq!(cdx_cpe(&s, "waybill-fixture-plain"), None);
    assert!(spdx23_cpes(&s, "waybill-fixture-plain").is_empty());
}

/// A declared value that is not a CPE 2.3 name is not emitted.
#[test]
fn an_invalid_declared_cpe_is_not_emitted() {
    let s = scan();
    assert_eq!(cdx_cpe(&s, "waybill-fixture-badcpe"), None);
    assert!(!s.cdx.to_string().contains("cpe:2.3:a::waybill-fixture-badcpe"));
}

/// Edge case — a declared CPE whose version differs from the derivation's is
/// used as declared, never "corrected".
#[test]
fn a_declared_cpe_is_used_as_declared_even_when_its_version_differs() {
    let s = scan();
    assert_eq!(
        cdx_cpe(&s, "waybill-fixture-skew").as_deref(),
        Some(declared("waybill-fixture-skew", "2.9").as_str())
    );
}

/// US1 + US2 — the subject carries its declared CPE.
#[test]
fn the_subject_carries_its_declared_cpe() {
    let s = scan();
    let m = &s.cdx["metadata"]["component"];
    assert_eq!(m["name"], "waybill-fixture-cpeapp");
    assert_eq!(m["cpe"].as_str(), Some(declared("waybill-fixture-cpeapp", "1.0").as_str()));
}

/// US2 scenario 5 — no component that had a CPE loses it.
#[test]
fn no_component_loses_a_cpe() {
    let without = scan_with(Rows::Fail, project(), &[]);
    let with = scan();
    for c in without.cdx["components"].as_array().unwrap() {
        if c["cpe"].is_string() {
            let name = c["name"].as_str().unwrap();
            assert!(
                cdx_cpe(&with, name).is_some(),
                "{name} carried {} without the evaluation and nothing with it",
                c["cpe"]
            );
        }
    }
}

/// FR-009 — C184 `identifiers`, against the fixture table.
#[test]
fn c184_records_identifier_coverage() {
    let s = scan();
    let id = c184_identifiers(&s);
    // The root and six members.
    assert_eq!(id["wanted"], 7, "{id}");
    assert_eq!(id["received"], 7, "{id}");
    assert_eq!(id["by-drv"], 6, "{id}");
    assert_eq!(id["by-name-version"], 1, "{id}");
    // root, zlib, shell, skew.
    assert_eq!(id["declared"], 4, "{id}");
    // badcpe.
    assert_eq!(id["rejected"], 1, "{id}");
}

/// FR-009 — a failed evaluation degrades: nothing received, curated stays.
#[test]
fn a_failed_evaluation_records_nothing_received() {
    let s = scan_with(Rows::Fail, project(), &[]);
    let id = c184_identifiers(&s);
    assert_eq!(id["wanted"], 7, "{id}");
    assert_eq!(id["received"], 0, "{id}");
    assert_eq!(id["declared"], 0, "{id}");
}

/// FR-010 — identifiers come from the evaluation the licenses already use:
/// exactly one meta evaluation per scan.
#[test]
fn there_is_exactly_one_meta_evaluation() {
    let s = scan();
    // The expression spans lines and the stub logs it verbatim, so count
    // its definition of `rowIn`, which occurs once per invocation.
    let calls = s.nix_log.matches("rowIn = root: name:").count();
    assert_eq!(calls, 1, "{}", s.nix_log);
}

/// FR-013 — an operator's `--supplement-cdx` CPE is kept, even when it is
/// exactly what the curated table would give.
#[test]
fn an_operator_supplied_cpe_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let supp = dir.path().join("supplement.cdx.json");
    let operator = "cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:*";
    std::fs::write(
        &supp,
        format!(
            r#"{{"bomFormat":"CycloneDX","specVersion":"1.6","components":[
                {{"type":"library","bom-ref":"zlib","name":"zlib","version":"9.9.9",
                  "purl":"pkg:generic/zlib@9.9.9","cpe":"{operator}"}}]}}"#
        ),
    )
    .unwrap();
    let rows = fixture("rows.json");
    let s = scan_with(
        Rows::Json(&rows),
        project(),
        &["--supplement-cdx", supp.to_str().unwrap()],
    );
    assert_eq!(cdx_cpe(&s, "zlib").as_deref(), Some(operator));
}
