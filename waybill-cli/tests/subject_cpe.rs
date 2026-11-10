//! #1402 — a document subject carries a CPE only when it has a real one.
//!
//! Before this, a subject with no CPE of its own was given
//! `cpe:2.3:a:waybill:<name>:<version>`: a CPE naming waybill as the vendor of
//! software waybill did not make, which no vulnerability database indexes.
//! Since #1407 a `--path .` self-scan made that `cpe:2.3:a:waybill:waybill:0.0.0`,
//! indistinguishable from a real identifier for waybill itself.
//!
//! Measured before removing it: sbomqs v2.1.1's overall score and NTIA 2026
//! profile do not move without it; the Interlynk profile moves by -0.005 on the
//! rust-ripgrep corpus golden.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

struct Docs {
    cdx: Value,
    spdx23: Value,
    spdx3: Value,
}

fn scan(dir: &Path, extra: &[&str]) -> Docs {
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let (c, s2, s3) = (out.join("c.json"), out.join("s2.json"), out.join("s3.json"));
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(dir.join("src"))
        .args([
            "--no-deep-hash",
            "--format",
            "cyclonedx-json",
            "--format",
            "spdx-2.3-json",
            "--format",
            "spdx-3-json",
        ])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .args(extra)
        .output()
        .unwrap();
    assert!(
        c.exists(),
        "no document: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    Docs {
        cdx: read(&c),
        spdx23: read(&s2),
        spdx3: read(&s3),
    }
}

/// The subject's CPEs, as each format states them.
fn subject_cpes(d: &Docs) -> [Vec<String>; 3] {
    let s = |v: &Value| v.as_str().map(str::to_string);
    let cdx: Vec<String> = s(&d.cdx["metadata"]["component"]["cpe"]).into_iter().collect();

    let described = d.spdx23["documentDescribes"][0].as_str().unwrap().to_string();
    let spdx23: Vec<String> = d.spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["SPDXID"].as_str() == Some(described.as_str()))
        .flat_map(|p| p["externalRefs"].as_array().cloned().unwrap_or_default())
        .filter(|r| r["referenceType"] == "cpe23Type")
        .filter_map(|r| s(&r["referenceLocator"]))
        .collect();

    let graph = d.spdx3["@graph"].as_array().unwrap();
    let root = graph
        .iter()
        .find(|e| e["type"] == "SpdxDocument")
        .and_then(|e| e["rootElement"][0].as_str())
        .unwrap()
        .to_string();
    let spdx3: Vec<String> = graph
        .iter()
        .filter(|e| e["spdxId"].as_str() == Some(root.as_str()))
        .flat_map(|e| e["externalIdentifier"].as_array().cloned().unwrap_or_default())
        .filter(|i| i["externalIdentifierType"] == "cpe23")
        .filter_map(|i| s(&i["identifier"]))
        .collect();
    [cdx, spdx23, spdx3]
}

fn no_waybill_vendor_cpe_anywhere(d: &Docs) {
    for (name, doc) in [("cdx", &d.cdx), ("spdx-2.3", &d.spdx23), ("spdx-3", &d.spdx3)] {
        assert!(
            !doc.to_string().contains("cpe:2.3:a:waybill:"),
            "{name} carries a waybill-vendor CPE"
        );
    }
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    for (name, body) in files {
        std::fs::write(src.join(name), body).unwrap();
    }
    dir
}

/// The synthetic placeholder subject (nothing identifies the target).
#[test]
fn a_placeholder_subject_carries_no_cpe() {
    let dir = project(&[("README.txt", "waybill-fixture: nothing to identify\n")]);
    let d = scan(dir.path(), &[]);
    assert_eq!(d.cdx["metadata"]["component"]["purl"], "pkg:generic/src@0.0.0");
    assert_eq!(subject_cpes(&d), [vec![], vec![], vec![]] as [Vec<String>; 3]);
    assert!(d.cdx["metadata"]["component"].get("cpe").is_none(), "absent, not null");
    no_waybill_vendor_cpe_anywhere(&d);
}

/// An operator override names the subject; it does not invent a vendor.
#[test]
fn an_overridden_subject_carries_no_cpe() {
    let dir = project(&[("README.txt", "waybill-fixture\n")]);
    let d = scan(
        dir.path(),
        &["--root-name", "waybill-fixture-svc", "--root-version", "1.2.3"],
    );
    assert_eq!(d.cdx["metadata"]["component"]["name"], "waybill-fixture-svc");
    assert_eq!(subject_cpes(&d), [vec![], vec![], vec![]] as [Vec<String>; 3]);
    no_waybill_vendor_cpe_anywhere(&d);
}

/// CONTROL: a subject with a CPE of its own keeps it, in every format, so the
/// two tests above pass because the placeholder is gone and not because
/// subject CPEs stopped being emitted at all.
#[test]
fn control_a_main_module_subject_keeps_its_own_cpe() {
    let dir = project(&[(
        "package.json",
        r#"{"name":"waybill-fixture-app","version":"1.4.0","dependencies":{}}"#,
    )]);
    let d = scan(dir.path(), &[]);
    let [cdx, spdx23, spdx3] = subject_cpes(&d);
    assert_eq!(cdx.len(), 1, "{:?}", d.cdx["metadata"]["component"]);
    assert!(cdx[0].contains("waybill-fixture-app:1.4.0"), "{cdx:?}");
    assert_eq!(spdx23.first(), cdx.first(), "SPDX 2.3 states the same CPE");
    assert!(spdx3.contains(&cdx[0]), "SPDX 3 states the same CPE: {spdx3:?}");
    no_waybill_vendor_cpe_anywhere(&d);
}
