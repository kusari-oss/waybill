//! Milestone 149 SC-004 — cross-format byte-equality of the
//! `waybill:demoted-from-main-module` annotation when the operator
//! passes `--root-name` + `--root-version` + `--preserve-manifest-main-module`
//! against a Cargo / npm / Go fixture.
//!
//! Per US1 acceptance scenarios 1+2 of `specs/149-demote-manifest-mainmod/spec.md`:
//! the demoted manifest-derived main-module entry MUST carry the
//! `waybill:demoted-from-main-module = "true"` annotation in `components[]`
//! across CDX 1.6 / SPDX 2.3 / SPDX 3 outputs, with byte-equivalent
//! value. Issue #151.
//!
//! Three tests (one per ecosystem in the FR-012 representative trio:
//! Cargo + npm + Go). The pip / gem / Maven coverage is operator-cadence
//! per spec Assumption 8.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;

const ROOT_NAME: &str = "widget-svc";
const ROOT_VERSION: &str = "1.2.3";

/// Run `waybill sbom scan` against `fixture` with the milestone-149
/// preserve flag set and milestone-077 root-override flags; return the
/// emitted document as a parsed serde_json::Value.
fn scan_with_demote(fixture: &Path, format: &str, output_file: &str) -> Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join(output_file);
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let mut cmd = Command::new(bin);
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--format")
        .arg(format)
        .arg("--output")
        .arg(&out_path)
        .arg("--root-name")
        .arg(ROOT_NAME)
        .arg("--root-version")
        .arg(ROOT_VERSION)
        .arg("--preserve-manifest-main-module")
        .arg("--no-deep-hash");
    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed for fixture {}: stderr={}",
        fixture.display(),
        String::from_utf8_lossy(&output.stderr),
    );
    let text = std::fs::read_to_string(&out_path).expect("read produced sbom");
    serde_json::from_str(&text).expect("emitted SBOM is valid JSON")
}

/// Extract the `waybill:demoted-from-main-module` annotation value
/// from a CDX 1.6 document for the component whose PURL prefix matches.
/// Returns `Some("true")` when the annotation is present, `None`
/// otherwise.
fn cdx_demote_value(doc: &Value, purl_prefix: &str) -> Option<String> {
    let components = doc.get("components")?.as_array()?;
    for c in components {
        let purl = c.get("purl").and_then(|v| v.as_str())?;
        if !purl.starts_with(purl_prefix) {
            continue;
        }
        let props = c.get("properties")?.as_array()?;
        for p in props {
            let name = p.get("name").and_then(|v| v.as_str())?;
            if name == "waybill:demoted-from-main-module" {
                return p.get("value").and_then(|v| v.as_str()).map(String::from);
            }
        }
    }
    None
}

/// Extract the `waybill:demoted-from-main-module` annotation value
/// from an SPDX 2.3 document via the envelope annotation pattern.
fn spdx23_demote_value(doc: &Value, purl_prefix: &str) -> Option<String> {
    let packages = doc.get("packages")?.as_array()?;
    for p in packages {
        let external_refs = p.get("externalRefs").and_then(|v| v.as_array());
        let purl_match = external_refs.is_some_and(|refs| {
            refs.iter().any(|r| {
                r.get("referenceType").and_then(|v| v.as_str()) == Some("purl")
                    && r.get("referenceLocator")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.starts_with(purl_prefix))
            })
        });
        if !purl_match {
            continue;
        }
        let annotations = p.get("annotations").and_then(|v| v.as_array());
        if let Some(anns) = annotations {
            for a in anns {
                let comment = a.get("comment").and_then(|v| v.as_str())?;
                let env: Value = match serde_json::from_str(comment) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if env.get("field").and_then(|v| v.as_str())
                    == Some("waybill:demoted-from-main-module")
                {
                    return env.get("value").and_then(|v| v.as_str()).map(String::from);
                }
            }
        }
    }
    None
}

/// Extract the `waybill:demoted-from-main-module` annotation value
/// from an SPDX 3 document.
///
/// **SPDX 3 subject-routing note**: per the C102 docs row in
/// `docs/reference/sbom-format-mapping.md`, the SPDX 3 emitter's
/// `package_iri_by_purl` aliasing at `v3_document.rs:318-324`
/// rewrites the demoted entry's PURL → synth-root IRI to serve
/// milestone-084 relationship re-anchoring (US1 Option A — demoted
/// entry has no outbound dependsOn in wire output). The demote
/// annotation rides with the alias and ends up emitted with
/// `subject = synth_root_iri` rather than `subject = demoted_entry_iri`.
/// This differs from CDX (subject = demoted entry's bom-ref) and
/// SPDX 2.3 (subject = demoted Package's SPDXID). Per FR-009 the
/// annotation VALUE is byte-identical across all three formats;
/// only the SPDX 3 SUBJECT differs.
///
/// This extractor walks ALL Annotation elements regardless of
/// subject — finding the annotation by its `statement` envelope's
/// `field` key, then returning its value.
fn spdx3_demote_value(doc: &Value, _purl_prefix: &str) -> Option<String> {
    let graph = doc.get("@graph")?.as_array()?;
    for el in graph {
        let ty = el.get("type").and_then(|v| v.as_str())?;
        if ty != "Annotation" {
            continue;
        }
        let statement = el.get("statement").and_then(|v| v.as_str())?;
        let env: Value = match serde_json::from_str(statement) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if env.get("field").and_then(|v| v.as_str())
            == Some("waybill:demoted-from-main-module")
        {
            return env.get("value").and_then(|v| v.as_str()).map(String::from);
        }
    }
    None
}

/// Assert all three formats agree on the demote annotation value.
fn assert_cross_format_demote_annotation(fixture: &Path, purl_prefix: &str, ecosystem: &str) {
    let cdx = scan_with_demote(fixture, "cyclonedx-json", "out.cdx.json");
    let spdx23 = scan_with_demote(fixture, "spdx-2.3-json", "out.spdx.json");
    let spdx3 = scan_with_demote(fixture, "spdx-3-json", "out.spdx3.json");

    let cdx_v = cdx_demote_value(&cdx, purl_prefix).unwrap_or_else(|| {
        panic!(
            "{ecosystem}: CDX MUST carry waybill:demoted-from-main-module on component starting \
             with `{purl_prefix}`; emitted doc had no matching annotation. Components: {}",
            serde_json::to_string_pretty(cdx.get("components").unwrap_or(&Value::Null))
                .unwrap_or_default()
        )
    });
    let spdx23_v = spdx23_demote_value(&spdx23, purl_prefix).unwrap_or_else(|| {
        panic!(
            "{ecosystem}: SPDX 2.3 MUST carry waybill:demoted-from-main-module on package with \
             PURL starting `{purl_prefix}`; emitted doc had no matching annotation",
        )
    });
    let spdx3_v = spdx3_demote_value(&spdx3, purl_prefix).unwrap_or_else(|| {
        panic!(
            "{ecosystem}: SPDX 3 MUST carry waybill:demoted-from-main-module on software_Package \
             with PURL starting `{purl_prefix}`; emitted doc had no matching annotation",
        )
    });

    // Cross-format byte-equality of the annotation value.
    assert_eq!(
        cdx_v, "true",
        "{ecosystem}: CDX demote annotation MUST be 'true', got {cdx_v:?}"
    );
    assert_eq!(
        cdx_v, spdx23_v,
        "{ecosystem}: CDX vs SPDX 2.3 demote annotation value MUST be byte-identical",
    );
    assert_eq!(
        cdx_v, spdx3_v,
        "{ecosystem}: CDX vs SPDX 3 demote annotation value MUST be byte-identical",
    );

    // Sanity: the root component identity should be the operator override,
    // not the manifest-derived identity. Verify in CDX (cheapest check).
    let root_name = cdx
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(
        root_name,
        Some(ROOT_NAME),
        "{ecosystem}: CDX metadata.component.name MUST be the operator override",
    );
}

fn fixture(subpath: &str) -> PathBuf {
    let fixtures_dir = std::env::var("WAYBILL_FIXTURES_DIR")
        .expect("WAYBILL_FIXTURES_DIR env var (set by build.rs from milestone 090)");
    PathBuf::from(fixtures_dir).join(subpath)
}

#[test]
fn demote_cargo_main_module_emits_byte_identical_annotation_across_formats_md149() {
    // Use `transitive_parity/cargo` — a single-crate fixture with
    // [package].name in Cargo.toml. (`cargo/lockfile-v3` has only a
    // Cargo.lock and no manifest, so no main-module is tagged.)
    let fx = fixture("transitive_parity/cargo");
    assert_cross_format_demote_annotation(&fx, "pkg:cargo/", "cargo");
}

#[test]
fn demote_npm_main_module_emits_byte_identical_annotation_across_formats_md149() {
    let fx = fixture("npm/node-modules-walk");
    assert_cross_format_demote_annotation(&fx, "pkg:npm/", "npm");
}

#[test]
fn demote_go_main_module_emits_byte_identical_annotation_across_formats_md149() {
    let fx = fixture("go/simple-module");
    assert_cross_format_demote_annotation(&fx, "pkg:golang/", "go");
}
