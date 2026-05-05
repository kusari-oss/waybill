//! Milestone 073 T019 — determinism + cross-format consistency tests
//! (US4 forward-looking handshake for milestone 074's
//! `--bind-to-source <identifier>` resolution path).
//!
//! Two assertions:
//!
//! 1. The same scan invocation against byte-identical inputs produces
//!    SBOMs whose identifier slots are byte-identical across runs.
//! 2. An external walker extracting identifiers from each format
//!    (CDX / SPDX 2.3 / SPDX 3) returns equal `(scheme, value)` lists.
//!    This satisfies SC-002 + the milestone-074 forward-looking
//!    handshake (SC-005).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn write_minimal_cargo_project(dir: &Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        b"[package]\nname = \"src-id-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.lock"),
        b"version = 3\n\n[[package]]\nname = \"src-id-test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
}

fn scan_all_formats(
    path: &Path,
    fake_home: &Path,
    extra_args: &[&str],
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let cdx_out = path.join("out.cdx.json");
    let spdx23_out = path.join("out.spdx.json");
    let spdx3_out = path.join("out.spdx3.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_out.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_out.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_out.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cdx: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&cdx_out).unwrap()).unwrap();
    let spdx23: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&spdx23_out).unwrap()).unwrap();
    let spdx3: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&spdx3_out).unwrap()).unwrap();
    (cdx, spdx23, spdx3)
}

/// Extract `(scheme, value)` pairs from a CDX SBOM.
fn extract_cdx_identifiers(doc: &serde_json::Value) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    // Built-in identifiers ride metadata.component.externalReferences[].
    if let Some(refs) = doc["metadata"]["component"]
        .get("externalReferences")
        .and_then(|v| v.as_array())
    {
        for r in refs {
            let ty = r.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
            // Map CDX type back to scheme. `bom` is milestone 072's
            // cross-document reference, NOT a milestone-073 identifier
            // — exclude it.
            let scheme = match ty {
                "vcs" => "repo",
                "distribution" => "image",
                "attestation" => "attestation",
                _ => continue,
            };
            out.insert((scheme.to_string(), url.to_string()));
        }
    }
    // User-defined identifiers ride metadata.properties[mikebom:identifiers].
    if let Some(props) = doc["metadata"].get("properties").and_then(|v| v.as_array()) {
        for p in props {
            if p.get("name").and_then(|v| v.as_str()) != Some("mikebom:identifiers") {
                continue;
            }
            let raw = p["value"].as_str().unwrap_or("[]");
            let parsed: Vec<serde_json::Value> = serde_json::from_str(raw).unwrap_or_default();
            for entry in parsed {
                let scheme = entry["scheme"].as_str().unwrap_or("").to_string();
                let value = entry["value"].as_str().unwrap_or("").to_string();
                out.insert((scheme, value));
            }
        }
    }
    out
}

/// Extract `(scheme, value)` pairs from a SPDX 2.3 SBOM. Walks both
/// the main-module Package's externalRefs[PERSISTENT-ID] AND the
/// document-level annotations[] envelope.
fn extract_spdx23_identifiers(doc: &serde_json::Value) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    // Built-in: main-module Package.externalRefs[PERSISTENT-ID].
    if let Some(packages) = doc.get("packages").and_then(|v| v.as_array()) {
        for pkg in packages {
            let Some(refs) = pkg.get("externalRefs").and_then(|v| v.as_array()) else {
                continue;
            };
            for r in refs {
                if r.get("referenceCategory").and_then(|v| v.as_str()) != Some("PERSISTENT-ID") {
                    continue;
                }
                let scheme = r
                    .get("referenceType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let value = r
                    .get("referenceLocator")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                out.insert((scheme, value));
            }
        }
    }
    // User-defined: document-level annotations[mikebom:identifiers].
    if let Some(annos) = doc.get("annotations").and_then(|v| v.as_array()) {
        for a in annos {
            let Some(comment) = a.get("comment").and_then(|v| v.as_str()) else {
                continue;
            };
            let parsed: serde_json::Value = match serde_json::from_str(comment) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if parsed.get("field").and_then(|v| v.as_str()) != Some("mikebom:identifiers") {
                continue;
            }
            let Some(arr) = parsed.get("value").and_then(|v| v.as_array()) else {
                continue;
            };
            for entry in arr {
                let scheme = entry["scheme"].as_str().unwrap_or("").to_string();
                let value = entry["value"].as_str().unwrap_or("").to_string();
                out.insert((scheme, value));
            }
        }
    }
    out
}

/// Extract `(scheme, value)` pairs from a SPDX 3 SBOM. Walks the
/// SpdxDocument element's externalIdentifier[] (which carries BOTH
/// built-in and user-defined identifiers per SPDX 3's open-typed
/// model).
fn extract_spdx3_identifiers(doc: &serde_json::Value) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let Some(graph) = doc["@graph"].as_array() else {
        return out;
    };
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("SpdxDocument") {
            continue;
        }
        let Some(idents) = el.get("externalIdentifier").and_then(|v| v.as_array()) else {
            continue;
        };
        for i in idents {
            let scheme = i
                .get("externalIdentifierType")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = i
                .get("identifier")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.insert((scheme, value));
        }
    }
    out
}

#[test]
fn deterministic_emission_byte_identical_across_runs() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let extra = [
        "--repo",
        "git@github.com:acme/foo.git",
        "--id",
        "acme_corp_id=abc123",
    ];
    let (cdx_a, spdx23_a, spdx3_a) = scan_all_formats(td.path(), fake_home.path(), &extra);
    let (cdx_b, spdx23_b, spdx3_b) = scan_all_formats(td.path(), fake_home.path(), &extra);

    let ids_a_cdx = extract_cdx_identifiers(&cdx_a);
    let ids_b_cdx = extract_cdx_identifiers(&cdx_b);
    assert_eq!(
        ids_a_cdx, ids_b_cdx,
        "CDX identifier extraction must be byte-identical across two runs of the same scan"
    );

    let ids_a_spdx23 = extract_spdx23_identifiers(&spdx23_a);
    let ids_b_spdx23 = extract_spdx23_identifiers(&spdx23_b);
    assert_eq!(
        ids_a_spdx23, ids_b_spdx23,
        "SPDX 2.3 identifier extraction must be byte-identical across two runs"
    );

    let ids_a_spdx3 = extract_spdx3_identifiers(&spdx3_a);
    let ids_b_spdx3 = extract_spdx3_identifiers(&spdx3_b);
    assert_eq!(
        ids_a_spdx3, ids_b_spdx3,
        "SPDX 3 identifier extraction must be byte-identical across two runs"
    );
}

#[test]
fn cross_format_consistency_same_identifier_set() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let extra = [
        "--repo",
        "git@github.com:acme/foo.git",
        "--id",
        "acme_corp_id=abc123",
        "--id",
        "internal_ticket=PROJ-456",
    ];
    let (cdx, spdx23, spdx3) = scan_all_formats(td.path(), fake_home.path(), &extra);

    let ids_cdx = extract_cdx_identifiers(&cdx);
    let ids_spdx23 = extract_spdx23_identifiers(&spdx23);
    let ids_spdx3 = extract_spdx3_identifiers(&spdx3);

    // The expected identifier set: 1 built-in (repo:) + 2 user-defined
    // (acme_corp_id, internal_ticket) — three pairs total.
    let expected: BTreeSet<(String, String)> = [
        ("repo".to_string(), "git@github.com:acme/foo.git".to_string()),
        ("acme_corp_id".to_string(), "abc123".to_string()),
        ("internal_ticket".to_string(), "PROJ-456".to_string()),
    ]
    .into_iter()
    .collect();

    assert_eq!(ids_cdx, expected, "CDX identifier set mismatch");
    assert_eq!(
        ids_spdx23, expected,
        "SPDX 2.3 identifier set mismatch (note SPDX 2.3 only carries built-ins on main-module Package; if scan has no main-module the built-in `repo:` may not appear here, in which case the assertion fails — this fixture has a Cargo.toml so a cargo main-module IS produced and the repo: rides on it)"
    );
    assert_eq!(ids_spdx3, expected, "SPDX 3 identifier set mismatch");
}
