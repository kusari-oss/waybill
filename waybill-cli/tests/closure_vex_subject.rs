//! #1436 — a `--nix-closure` scan's backport claim, whatever its subject.
//!
//! Part 1 of #1436: since milestone 1377 the subject of a closure scan is the
//! evaluated package, and the VEX code found its "this build" subject only
//! through a main-module component, so on a wrapper flake (no language
//! manifest, so no main module) no backport statement was emitted at all.
//! Measured on curl, openssh, httpie, gh and ripgrep wrapper flakes.
//!
//! Milestone 1436 removed the dependence: a backport claim is one `fixed`
//! statement whose product is the patched component's own build-qualified
//! PURL, so it needs no subject. These three shapes — a wrapper flake, a main
//! module beside the evaluated package, and the evaluated package as the
//! main module — now pin that the claim is the same in each, and that the
//! document's subject is still the one milestone 1377 elects.
//!
//! Every assertion is on the emitted documents, through the stub `nix`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

const CVE: &str = "CVE-2026-90001";
/// The fixture's `.drv` keys carry no store hash, so `nix_drv` falls back to
/// the key's basename without `.drv` (`patches::drv_hashes`).
const LIB: &str = "pkg:generic/waybill-fixture-lib@1.0?nix_drv=aaaa-waybill-fixture-lib-1.0";

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_subject")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The 1377 project: a cabal package, so a main module.
fn cabal_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

/// The wrapper-flake shape: the same flake, with no language manifest beside
/// it, so the scan finds no main module.
fn wrapper_flake() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for f in ["flake.nix", "flake.lock"] {
        std::fs::copy(cabal_project().join(f), dir.path().join(f)).unwrap();
    }
    dir
}

/// A closure fixture with `waybill-fixture-lib` applying a CVE-named patch.
fn patched(closure_fixture: &str) -> String {
    let mut c: Value = serde_json::from_str(&fixture(closure_fixture)).unwrap();
    c["derivations"]["aaaa-waybill-fixture-lib-1.0.drv"]["env"]["patches"] =
        Value::String(format!("/nix/store/p1-{CVE}.patch"));
    c.to_string()
}

struct VexScan {
    scan: Scan,
    openvex: Option<Value>,
    _out: tempfile::TempDir,
}

fn scan(project: &Path, closure: &str) -> VexScan {
    let out = tempfile::tempdir().unwrap();
    let sidecar = out.path().join("waybill.openvex.json");
    let sidecar_arg = format!("openvex={}", sidecar.display());
    let rows = fixture("rows.json");
    let scan = NixStub {
        project: project.to_path_buf(),
        closure,
        rows: Rows::Json(&rows),
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&["--nix-closure", "--output", &sidecar_arg]);
    let openvex = std::fs::read(&sidecar)
        .ok()
        .map(|b| serde_json::from_slice(&b).unwrap());
    VexScan { scan, openvex, _out: out }
}

/// Every sidecar statement about [`CVE`].
fn openvex_statements(s: &VexScan) -> Vec<Value> {
    let doc = s.openvex.as_ref().unwrap_or_else(|| {
        panic!("no OpenVEX sidecar was written.\nstderr:\n{}", s.scan.stderr)
    });
    doc["statements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|st| st["vulnerability"]["name"] == CVE)
        .cloned()
        .collect()
}

/// Every CycloneDX `vulnerabilities[]` entry for [`CVE`].
fn cdx_entries(s: &VexScan) -> Vec<Value> {
    s.scan.cdx["vulnerabilities"]
        .as_array()
        .unwrap_or_else(|| panic!("no vulnerabilities[]: {:#}", s.scan.cdx["vulnerabilities"]))
        .iter()
        .filter(|v| v["id"] == CVE)
        .cloned()
        .collect()
}

fn subject_purl(s: &VexScan) -> String {
    s.scan.cdx["metadata"]["component"]["purl"].as_str().unwrap_or_default().to_string()
}

/// Asserts both carriers make the one backport claim, on the patched
/// component's qualified PURL, whatever the document's subject is.
fn assert_the_claim_names_the_component(s: &VexScan, expected_subject: &str) {
    // CONTROL: the document's subject is the one this case is about, so the
    // claim below is shown not to depend on it.
    assert_eq!(subject_purl(s), expected_subject, "document subject");

    let st = openvex_statements(s);
    assert_eq!(st.len(), 1, "one statement for the pair: {st:#?}");
    assert_eq!(st[0]["status"], "fixed", "{st:#?}");
    assert_eq!(st[0]["products"][0]["@id"], LIB, "{st:#?}");
    assert!(st[0]["products"][0]["subcomponents"].is_null(), "{st:#?}");

    let v = cdx_entries(s);
    assert_eq!(v.len(), 1, "one entry for the pair: {v:#?}");
    assert_eq!(v[0]["analysis"]["state"], "resolved", "{v:#?}");
    assert_eq!(v[0]["affects"], serde_json::json!([{ "ref": LIB }]), "{v:#?}");
    assert!(
        !v[0]["properties"].to_string().contains("waybill:vex-subcomponent"),
        "{v:#?}"
    );
}

/// The issue's case: a wrapper flake has no main module, and its subject is
/// the evaluated package. Before #1436 neither carrier held a statement.
#[test]
fn a_wrapper_flake_gets_its_backport_claim() {
    let dir = wrapper_flake();
    let s = scan(dir.path(), &patched("closure.json"));
    assert_the_claim_names_the_component(&s, "pkg:generic/waybill-fixture-app@2.1");
}

/// A main module that is not the evaluated package is a sibling of the
/// subject. Before #1436 the VEX named it as "this build" while the document
/// named the evaluated package; now no backport claim names a build at all.
#[test]
fn a_main_module_beside_the_evaluated_package_does_not_change_the_claim() {
    let s = scan(&cabal_project(), &patched("closure.json"));
    assert_the_claim_names_the_component(&s, "pkg:generic/waybill-fixture-app@2.1");
}

/// Control: the moat shape, where the evaluated package IS the main module.
/// Its subject was right before #1436 and must not move.
#[test]
fn control_an_evaluated_package_that_is_the_main_module_keeps_its_subject() {
    let s = scan(&cabal_project(), &patched("closure-mainmodule.json"));
    assert_the_claim_names_the_component(&s, "pkg:hackage/waybill-fixture-roleproj@0.1.0");
}
