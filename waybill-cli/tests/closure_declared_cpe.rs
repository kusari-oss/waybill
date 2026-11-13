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
        ..Default::default()
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
    // Not a second identifier anywhere. (C212's `overrode` does name it,
    // as a record of what was overridden, which is the point of that row.)
    let props = component(&s, "zlib")["properties"].as_array().cloned().unwrap_or_default();
    assert!(
        !props.iter().any(|p| p["name"] == "waybill:cpe-candidates"),
        "the overridden curated CPE must not be emitted as a candidate: {props:?}"
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

// ---------------------------------------------------------------- US3

const C212: &str = "waybill:cpe-provenance";
const DECLARED_COMMENT: &str = "CPE declared by nixpkgs meta.identifiers.cpe";
const CURATED_COMMENT: &str = "CPE from waybill's curated CPE table";

/// The `evidence.identity[]` entry with `field: "cpe"`, if any.
fn cdx_cpe_identity(s: &Scan, name: &str) -> Option<Value> {
    component(s, name)["evidence"]["identity"]
        .as_array()?
        .iter()
        .find(|i| i["field"] == "cpe")
        .cloned()
}

fn spdx23_cpe_comment(s: &Scan, name: &str) -> Option<String> {
    s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str() == Some(name))
        .flat_map(|p| p["externalRefs"].as_array().cloned().unwrap_or_default())
        .find(|r| r["referenceType"] == "cpe23Type")
        .and_then(|r| r["comment"].as_str().map(str::to_string))
}

fn spdx3_cpe_comment(s: &Scan, name: &str) -> Option<String> {
    let graph = s.spdx3["@graph"].as_array().unwrap();
    graph
        .iter()
        .filter(|e| e["type"] == "software_Package" && e["name"].as_str() == Some(name))
        .flat_map(|e| e["externalIdentifier"].as_array().cloned().unwrap_or_default())
        .find(|id| id["externalIdentifierType"] == "cpe23")
        .and_then(|id| id["comment"].as_str().map(str::to_string))
}

fn envelope_value(comment: &str, name: &str) -> Option<String> {
    let v: Value = serde_json::from_str(comment).ok()?;
    if v.get("field")?.as_str()? != name {
        return None;
    }
    Some(match v.get("value")? {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// Every C212 value in each format, sorted, so the three can be compared.
fn c212_values(s: &Scan) -> [Vec<String>; 3] {
    let mut cdx: Vec<String> = s.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["properties"].as_array().cloned().unwrap_or_default())
        .filter(|p| p["name"] == C212)
        .filter_map(|p| p["value"].as_str().map(str::to_string))
        .collect();
    let mut spdx23: Vec<String> = s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["annotations"].as_array().cloned().unwrap_or_default())
        .filter_map(|a| envelope_value(a["comment"].as_str()?, C212))
        .collect();
    let mut spdx3: Vec<String> = s.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "Annotation")
        .filter_map(|a| envelope_value(a["statement"].as_str()?, C212))
        .collect();
    cdx.sort();
    spdx23.sort();
    spdx3.sort();
    [cdx, spdx23, spdx3]
}

/// US3 scenario 1, SC-005 — a declared CPE's source, natively, in every
/// format.
#[test]
fn a_declared_cpe_records_its_source_natively_in_every_format() {
    let s = scan();
    let id = cdx_cpe_identity(&s, "zlib").expect("an evidence.identity cpe entry");
    assert_eq!(id["concludedValue"], declared("zlib", "9.9.9"));
    assert_eq!(id["confidence"], 0.9);
    assert_eq!(id["methods"][0]["technique"], "manifest-analysis");
    assert_eq!(id["methods"][0]["value"], "nixpkgs meta.identifiers.cpe");
    assert_eq!(spdx23_cpe_comment(&s, "zlib").as_deref(), Some(DECLARED_COMMENT));
    assert_eq!(spdx3_cpe_comment(&s, "zlib").as_deref(), Some(DECLARED_COMMENT));
}

/// US3 scenario 2 — a curated CPE says so, and carries no C212.
#[test]
fn a_curated_cpe_records_its_source_natively_and_no_c212() {
    let s = scan();
    let id = cdx_cpe_identity(&s, "openssl").expect("an evidence.identity cpe entry");
    assert_eq!(id["methods"][0]["technique"], "other");
    assert_eq!(id["methods"][0]["value"], "waybill curated CPE table");
    assert_eq!(id["confidence"], 0.7);
    assert_eq!(spdx23_cpe_comment(&s, "openssl").as_deref(), Some(CURATED_COMMENT));
    assert_eq!(spdx3_cpe_comment(&s, "openssl").as_deref(), Some(CURATED_COMMENT));
    let props = component(&s, "openssl")["properties"].as_array().cloned().unwrap_or_default();
    assert!(!props.iter().any(|p| p["name"] == C212), "{props:?}");
}

/// SC-005 — every CPE-bearing closure component records a source in each
/// format. The project's own cabal package is not a closure component.
#[test]
fn every_cpe_bearing_closure_component_records_a_source() {
    let s = scan();
    let mut checked = 0;
    for c in s.cdx["components"].as_array().unwrap() {
        let closure_member = c["properties"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["name"] == "waybill:closure-role"));
        if !closure_member || !c["cpe"].is_string() {
            continue;
        }
        let name = c["name"].as_str().unwrap();
        assert!(cdx_cpe_identity(&s, name).is_some(), "{name}: no CDX cpe identity");
        assert!(spdx23_cpe_comment(&s, name).is_some(), "{name}: no SPDX 2.3 comment");
        assert!(spdx3_cpe_comment(&s, name).is_some(), "{name}: no SPDX 3 comment");
        checked += 1;
    }
    // zlib, shell, openssl, skew and the subject's components[] entry.
    assert!(checked >= 4, "the population checked was {checked}");
}

/// US3 scenario 3 — the bash shape on zlib: one CPE identifier, the declared
/// one, and C212 names the curated CPE it overrode.
#[test]
fn c212_records_the_basis_and_the_overridden_curated_cpe() {
    let s = scan();
    let [cdx, _, _] = c212_values(&s);
    let parsed: Vec<Value> = cdx.iter().map(|v| serde_json::from_str(v).unwrap()).collect();
    assert!(
        parsed.contains(&serde_json::json!({
            "basis": "drv",
            "overrode": "cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:*"
        })),
        "{cdx:?}"
    );
    assert!(parsed.contains(&serde_json::json!({ "basis": "name-version" })), "{cdx:?}");
    // root, zlib, shell, skew.
    assert_eq!(cdx.len(), 4, "{cdx:?}");
}

/// T025 — C212 carries identical values in all three formats.
#[test]
fn c212_is_identical_in_all_three_formats() {
    let s = scan();
    let [cdx, spdx23, spdx3] = c212_values(&s);
    assert!(!cdx.is_empty());
    assert_eq!(cdx, spdx23);
    assert_eq!(cdx, spdx3);
}

/// T025 — the project's own parity check reports C212 present, with the
/// same count, in every format.
#[test]
fn parity_check_reports_c212_in_every_format() {
    let s = scan();
    let dir = tempfile::tempdir().unwrap();
    for (f, v) in [
        ("waybill.cdx.json", &s.cdx),
        ("waybill.spdx.json", &s.spdx23),
        ("waybill.spdx3.json", &s.spdx3),
    ] {
        std::fs::write(dir.path().join(f), v.to_string()).unwrap();
    }
    let out = {
            // #1046: never reach Hackage from a test (Principle VII).
            let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_waybill"));
            c.env("WAYBILL_HACKAGE_URL", "http://127.0.0.1:9");
            c
        }
        .args(["sbom", "parity-check", "--format", "json", "--scan-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}", String::from_utf8_lossy(&out.stderr))
    });
    let row = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["row_id"] == "C212")
        .expect("a C212 row");
    // Counted as a set of values: the root and skew share `{"basis":"drv"}`,
    // so four components give three distinct values.
    for f in ["cdx", "spdx23", "spdx3"] {
        assert_eq!(row[f]["kind"], "present", "{row}");
        assert_eq!(row[f]["value_count"], 3, "{row}");
    }
    assert_eq!(row["universal_parity"], true, "{row}");
}

/// FR-013 — an operator's CPE carries no closure provenance of any kind.
#[test]
fn an_operator_supplied_cpe_carries_no_closure_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let supp = dir.path().join("supplement.cdx.json");
    std::fs::write(
        &supp,
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6","components":[
            {"type":"library","bom-ref":"zlib","name":"zlib","version":"9.9.9",
             "purl":"pkg:generic/zlib@9.9.9","cpe":"cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:*"}]}"#,
    )
    .unwrap();
    let rows = fixture("rows.json");
    let s = scan_with(
        Rows::Json(&rows),
        project(),
        &["--supplement-cdx", supp.to_str().unwrap()],
    );
    assert!(cdx_cpe_identity(&s, "zlib").is_none());
    assert_eq!(spdx23_cpe_comment(&s, "zlib"), None);
    assert_eq!(spdx3_cpe_comment(&s, "zlib"), None);
    let props = component(&s, "zlib")["properties"].as_array().cloned().unwrap_or_default();
    assert!(!props.iter().any(|p| p["name"] == C212), "{props:?}");
}
