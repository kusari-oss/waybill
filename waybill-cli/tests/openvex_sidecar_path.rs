//! #1443 — the OpenVEX sidecar a scan writes: schema-valid, named after the
//! SPDX document it accompanies, and logged as its own artifact.
//!
//! Before the fix, every `affected` statement lacked the `action_statement`
//! OpenVEX 0.2.0 requires; the sidecar was always `waybill.openvex.json`, so
//! two scans into one directory overwrote each other's; and stderr reported
//! it under `format=spdx-2.3-json` and again under `format=spdx-3-json`.
//!
//! Driven through the stub `nix` with a closure whose library applies a
//! CVE-named patch, which is the m1035 backport path. The declaration path
//! (m1050) cannot be driven by a fixture (see `nixpkgs_declarations.rs`); its
//! schema validity is unit-tested in `generate/openvex/mod.rs`.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};
use serde_json::Value;
use sha2::{Digest, Sha256};

const CVE: &str = "CVE-2026-90001";

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nix_closure_subject")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

/// The moat shape: the evaluated package is the main module, and
/// `waybill-fixture-lib` applies a patch named for [`CVE`].
fn patched_closure() -> String {
    let mut c: Value = serde_json::from_str(&fixture("closure-mainmodule.json")).unwrap();
    c["derivations"]["aaaa-waybill-fixture-lib-1.0.drv"]["env"]["patches"] =
        Value::String(format!("/nix/store/p1-{CVE}.patch"));
    c.to_string()
}

/// One `--nix-closure` scan writing `<stem>.{cdx,spdx,spdx3}.json` into `out`.
fn scan_into(out: &Path, stem: &str) -> Scan {
    let closure = patched_closure();
    let rows = fixture("rows.json");
    NixStub {
        project: project(),
        closure: &closure,
        rows: Rows::Json(&rows),
        attr: DEFAULT_ATTR,
    }
    .scan_to(out, stem, &["--nix-closure"])
}

fn openvex_schema() -> &'static jsonschema::Validator {
    static CELL: OnceLock<jsonschema::Validator> = OnceLock::new();
    CELL.get_or_init(|| {
        let raw = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schemas/openvex-0.2.0.json"),
        )
        .unwrap();
        jsonschema::validator_for(&serde_json::from_str(&raw).unwrap()).unwrap()
    })
}

fn schema_errors(doc: &Value) -> Vec<String> {
    openvex_schema()
        .iter_errors(doc)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

fn read_json(p: &Path) -> Value {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_slice(&bytes).unwrap()
}

/// The log as a reader sees it: tracing colours its fields when stderr is
/// not a terminal too.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Every `*.openvex.json` in `dir`.
fn sidecars_in(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".openvex.json"))
        .collect()
}

/// The SPDX 2.3 document's reference to its sidecar: (file, SHA-256).
fn spdx23_reference(doc: &Value) -> (String, String) {
    let r = doc["externalDocumentRefs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["externalDocumentId"] == "DocumentRef-OpenVEX")
        .unwrap_or_else(|| panic!("no DocumentRef-OpenVEX: {:#}", doc["externalDocumentRefs"]));
    assert_eq!(r["checksum"]["algorithm"], "SHA256", "{r:#}");
    (
        r["spdxDocument"].as_str().unwrap().to_string(),
        r["checksum"]["checksumValue"].as_str().unwrap().to_string(),
    )
}

/// The SPDX 3 document's locator for its sidecar.
fn spdx3_locator(doc: &Value) -> String {
    let refs: Vec<Value> = doc["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "SpdxDocument")
        .flat_map(|e| e["externalRef"].as_array().cloned().unwrap_or_default())
        .filter(|r| r["contentType"] == "application/openvex+json")
        .collect();
    assert_eq!(refs.len(), 1, "{refs:#?}");
    refs[0]["locator"][0].as_str().unwrap().to_string()
}

#[test]
fn the_sidecar_a_scan_writes_validates_against_the_openvex_schema() {
    let out = tempfile::tempdir().unwrap();
    let s = scan_into(out.path(), "app");
    let path = out.path().join("app.openvex.json");
    let doc = read_json(&path);

    // CONTROL: the document holds the statements whose shape is at issue,
    // so a clean result is not a clean result over nothing.
    let statuses: Vec<&str> = doc["statements"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|st| st["status"].as_str())
        .collect();
    assert!(
        statuses.contains(&"affected") && statuses.contains(&"not_affected"),
        "expected the backport pair: {statuses:?}\nstderr:\n{}",
        s.stderr
    );

    let errors = schema_errors(&doc);
    assert!(errors.is_empty(), "{} schema errors:\n{}", errors.len(), errors.join("\n"));

    // CONTROL: the validator rejects the pre-fix shape, so the pass above is
    // the schema speaking and not a validator that accepts anything.
    let mut pre_fix = doc.clone();
    for st in pre_fix["statements"].as_array_mut().unwrap() {
        st.as_object_mut().unwrap().remove("action_statement");
    }
    assert!(!schema_errors(&pre_fix).is_empty(), "validator accepted affected without action");
}

#[test]
fn two_scans_into_one_directory_write_two_sidecars() {
    let out = tempfile::tempdir().unwrap();
    let first = scan_into(out.path(), "first");
    let second = scan_into(out.path(), "second");

    assert_eq!(
        sidecars_in(out.path()),
        ["first.openvex.json", "second.openvex.json"]
            .into_iter()
            .map(String::from)
            .collect::<BTreeSet<_>>(),
        "stderr of the second scan:\n{}",
        second.stderr
    );

    for (stem, s) in [("first", &first), ("second", &second)] {
        let name = format!("{stem}.openvex.json");
        let bytes = std::fs::read(out.path().join(&name)).unwrap();

        // Both SPDX documents of one run reference the one sidecar.
        let (file, hash) = spdx23_reference(&s.spdx23);
        assert_eq!(file, name, "SPDX 2.3 reference");
        assert_eq!(hash, sha256_hex(&bytes), "SPDX 2.3 checksum of {name}");
        assert_eq!(spdx3_locator(&s.spdx3), name, "SPDX 3 locator");
    }
}

#[test]
fn the_sidecar_is_logged_once_as_its_own_artifact() {
    let out = tempfile::tempdir().unwrap();
    let s = scan_into(out.path(), "app");
    let stderr = strip_ansi(&s.stderr);
    let wrote: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("wrote") && l.contains("app.openvex.json"))
        .collect();
    assert_eq!(wrote.len(), 1, "the sidecar's write lines:\n{wrote:#?}\nstderr:\n{stderr}");
    assert!(wrote[0].contains("format=openvex"), "{}", wrote[0]);
}
