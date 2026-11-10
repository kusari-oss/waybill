//! #1430 — a curated CPE row's second vendor reaches a scanner.
//!
//! The curated `unzip` row is `info-zip:unzip` then `unzip_project:unzip`.
//! NVD files every unzip CVE grype knows under the second, and the second was
//! emitted only in `waybill:cpe-candidates`, which grype does not read. Its
//! CycloneDX decoder reads `component.cpe` and `syft:cpe23` properties; its
//! SPDX 2.3 decoder reads every `cpe23Type` reference
//! (`specs/1430-secondary-cpe-carrier/`).
//!
//! `unzip` and `zlib` are real names because the curated table is keyed on
//! the name; the versions are synthetic. The closure is JSON handed to a stub
//! `nix`, never evaluated by a real one.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

const INFO_ZIP: &str = "cpe:2.3:a:info-zip:unzip:9.9.9:*:*:*:*:*:*:*";
const UNZIP_PROJECT: &str = "cpe:2.3:a:unzip_project:unzip:9.9.9:*:*:*:*:*:*:*";
const ZLIB: &str = "cpe:2.3:a:zlib:zlib:9.9.9:*:*:*:*:*:*:*";

const CLOSURE: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-cpeproj-0.1.0",
      "env": {
        "pname": "waybill-fixture-cpeproj",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-unzip /nix/store/h2-zlib"
      },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-unzip.drv": {
      "name": "unzip-9.9.9",
      "env": { "pname": "unzip", "version": "9.9.9" },
      "outputs": { "out": { "path": "h1-unzip" } }
    },
    "d-zlib.drv": {
      "name": "zlib-9.9.9",
      "env": { "pname": "zlib", "version": "9.9.9" },
      "outputs": { "out": { "path": "h2-zlib" } }
    }
  },
  "version": 4
}"#;

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

/// The meta evaluation fails, so no nixpkgs-declared CPE replaces the
/// curated one: the curated table is what is under test.
fn scan() -> Scan {
    NixStub {
        project: project(),
        closure: CLOSURE,
        rows: Rows::Fail,
        attr: DEFAULT_ATTR,
    }
    .scan(&["--nix-closure", "--no-deps-dev", "--no-clearly-defined"])
}

fn component<'a>(s: &'a Scan, name: &str) -> &'a Value {
    s.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no component named {name}"))
}

fn properties(c: &Value, name: &str) -> Vec<String> {
    c["properties"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .filter(|p| p["name"].as_str() == Some(name))
                .filter_map(|p| p["value"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn spdx23_cpes(s: &Scan, name: &str) -> Vec<String> {
    s.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str() == Some(name))
        .flat_map(|p| p["externalRefs"].as_array().cloned().unwrap_or_default())
        .filter(|r| r["referenceType"].as_str() == Some("cpe23Type"))
        .filter_map(|r| r["referenceLocator"].as_str().map(str::to_string))
        .collect()
}

fn spdx3_cpes(s: &Scan, name: &str) -> Vec<String> {
    let mut out: Vec<String> = s.spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "software_Package" && e["name"].as_str() == Some(name))
        .flat_map(|e| e["externalIdentifier"].as_array().cloned().unwrap_or_default())
        .filter(|id| id["externalIdentifierType"].as_str() == Some("cpe23"))
        .filter_map(|id| id["identifier"].as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

/// CONTROL for every test below: the closure was read and both components
/// carry their curated primary CPE. Without it a degraded tier would pass
/// "zlib has no secondary carrier" over an empty document.
#[test]
fn i1430_control_the_curated_primaries_are_emitted() {
    let s = scan();
    assert!(
        s.cdx["metadata"]["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "waybill:nix-closure"),
        "C184 absent: the closure tier degraded, so nothing here is evidence\n{}",
        s.stderr
    );
    assert_eq!(component(&s, "unzip")["cpe"].as_str(), Some(INFO_ZIP));
    assert_eq!(component(&s, "zlib")["cpe"].as_str(), Some(ZLIB));
    assert_eq!(
        properties(component(&s, "unzip"), "waybill:cpe-candidates"),
        vec![serde_json::to_string(&[INFO_ZIP, UNZIP_PROJECT]).unwrap()],
        "C19 is unchanged"
    );
}

/// The second vendor rides `syft:cpe23` in CycloneDX.
#[test]
fn i1430_the_second_vendor_rides_syft_cpe23() {
    let s = scan();
    assert_eq!(properties(component(&s, "unzip"), "syft:cpe23"), vec![UNZIP_PROJECT]);
}

/// A single-vendor row has nothing to add.
#[test]
fn i1430_a_single_vendor_row_gets_no_syft_cpe23() {
    let s = scan();
    assert!(properties(component(&s, "zlib"), "syft:cpe23").is_empty());
}

/// SPDX 2.3 carries every curated vendor as a `cpe23Type` reference, the
/// primary first; SPDX 3 already did.
#[test]
fn i1430_both_spdx_formats_carry_every_vendor() {
    let s = scan();
    assert_eq!(spdx23_cpes(&s, "unzip"), vec![INFO_ZIP, UNZIP_PROJECT]);
    assert_eq!(spdx23_cpes(&s, "zlib"), vec![ZLIB]);
    assert_eq!(spdx3_cpes(&s, "unzip"), vec![INFO_ZIP, UNZIP_PROJECT]);
}

/// A12's parity extractor sees the further vendor in CycloneDX, and every
/// CycloneDX CPE is in both SPDX documents (A12 is `CdxSubsetOfSpdx`).
#[test]
fn i1430_a12_parity_covers_the_second_vendor() {
    let s = scan();
    let a12 = waybill::parity::extractors::EXTRACTORS
        .iter()
        .find(|e| e.row_id == "A12")
        .expect("A12 registered");
    let cdx = (a12.cdx)(&s.cdx);
    let spdx23 = (a12.spdx23)(&s.spdx23);
    let spdx3 = (a12.spdx3)(&s.spdx3);
    assert!(cdx.contains(UNZIP_PROJECT), "{cdx:?}");
    assert!(cdx.is_subset(&spdx23), "CDX {cdx:?}\nSPDX 2.3 {spdx23:?}");
    assert!(cdx.is_subset(&spdx3), "CDX {cdx:?}\nSPDX 3 {spdx3:?}");
}

/// The CycloneDX document stays schema-valid.
#[test]
fn i1430_the_cyclonedx_document_is_schema_valid() {
    let s = scan();
    let errors = common::cdx_schema::cdx_validation_errors(&s.cdx);
    assert!(errors.is_empty(), "{errors:#?}");
}
