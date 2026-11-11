//! #1436 — a `--nix-closure` scan's VEX names the subject its document names.
//!
//! Since milestone 1377 the subject of a closure scan is the evaluated
//! package. The VEX code found its "this build" subject only through a
//! main-module component, so on a wrapper flake (no language manifest, so no
//! main module) no backport statement was emitted at all: no OpenVEX sidecar
//! and no CycloneDX `vulnerabilities[]`. Measured on curl, openssh, httpie,
//! gh and ripgrep wrapper flakes.
//!
//! Every assertion is on the emitted documents, through the stub `nix`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;

const CVE: &str = "CVE-2026-90001";
const LIB: &str = "pkg:generic/waybill-fixture-lib@1.0";

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

/// The `not_affected` statement for [`CVE`], from the sidecar.
fn openvex_not_affected(s: &VexScan) -> Value {
    let doc = s.openvex.as_ref().unwrap_or_else(|| {
        panic!("no OpenVEX sidecar was written.\nstderr:\n{}", s.scan.stderr)
    });
    doc["statements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|st| st["status"] == "not_affected" && st["vulnerability"]["name"] == CVE)
        .cloned()
        .unwrap_or_else(|| panic!("no not_affected for {CVE}: {doc:#}"))
}

/// The CycloneDX `vulnerabilities[]` entry for [`CVE`] in `state`.
fn cdx_entry(s: &VexScan, state: &str) -> Value {
    s.scan.cdx["vulnerabilities"]
        .as_array()
        .unwrap_or_else(|| panic!("no vulnerabilities[]: {:#}", s.scan.cdx["vulnerabilities"]))
        .iter()
        .find(|v| v["id"] == CVE && v["analysis"]["state"] == state)
        .cloned()
        .unwrap_or_else(|| panic!("no {state} entry for {CVE}: {:#}", s.scan.cdx["vulnerabilities"]))
}

fn subject_purl(s: &VexScan) -> String {
    s.scan.cdx["metadata"]["component"]["purl"].as_str().unwrap_or_default().to_string()
}

fn subject_ref(s: &VexScan) -> String {
    s.scan.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or_default().to_string()
}

/// Asserts both carriers name the document subject as the build, with the
/// patched component beneath it.
fn assert_vex_names_the_subject(s: &VexScan, expected_subject: &str) {
    // CONTROL: the document's subject is the one this case is about, so the
    // VEX assertions below compare against a known value.
    assert_eq!(subject_purl(s), expected_subject, "document subject");

    let st = openvex_not_affected(s);
    assert_eq!(st["products"][0]["@id"], expected_subject, "OpenVEX build subject: {st:#}");
    assert_eq!(st["products"][0]["subcomponents"][0]["@id"], LIB, "{st:#}");

    // The version-scoped half is still emitted and still about the
    // component, not the build (the m1035 pair is unchanged).
    let doc = s.openvex.as_ref().unwrap();
    let affected: Vec<&Value> = doc["statements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|st| st["status"] == "affected" && st["vulnerability"]["name"] == CVE)
        .collect();
    assert_eq!(affected.len(), 1, "{doc:#}");
    assert_eq!(affected[0]["products"][0]["@id"], LIB);

    let not = cdx_entry(s, "not_affected");
    let refs: Vec<&str> = not["affects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["ref"].as_str())
        .collect();
    assert_eq!(refs, vec![subject_ref(s).as_str()], "CDX build subject: {not:#}");
    assert!(
        not["properties"]
            .as_array()
            .is_some_and(|p| p.iter().any(|p| p["name"] == "waybill:vex-subcomponent")),
        "the patched component rides as a subcomponent: {not:#}"
    );
    let _ = cdx_entry(s, "exploitable");
}

/// The issue's case: a wrapper flake has no main module, and its subject is
/// the evaluated package. Before #1436 neither carrier held a statement.
#[test]
fn a_wrapper_flake_names_the_evaluated_package_as_the_vex_subject() {
    let dir = wrapper_flake();
    let s = scan(dir.path(), &patched("closure.json"));
    assert_vex_names_the_subject(&s, "pkg:generic/waybill-fixture-app@2.1");
}

/// A main module that is not the evaluated package is a sibling of the
/// subject, so it is not "this build". Before #1436 the VEX named it while the
/// document named the evaluated package.
#[test]
fn a_main_module_beside_the_evaluated_package_does_not_become_the_vex_subject() {
    let s = scan(&cabal_project(), &patched("closure.json"));
    assert_vex_names_the_subject(&s, "pkg:generic/waybill-fixture-app@2.1");
}

/// Control: the moat shape, where the evaluated package IS the main module.
/// Its subject was right before #1436 and must not move.
#[test]
fn control_an_evaluated_package_that_is_the_main_module_keeps_its_subject() {
    let s = scan(&cabal_project(), &patched("closure-mainmodule.json"));
    assert_vex_names_the_subject(&s, "pkg:hackage/waybill-fixture-roleproj@0.1.0");
}
