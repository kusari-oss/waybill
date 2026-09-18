//! Issue #911 (#902 item 1) — resolve membership is plural and survives dedup.
//!
//! **Fixture shape** (`tests/fixtures/pants_resolve_edges/`):
//!
//! ```text
//! app.lock     consumer-a 1.0.0 -> shared      shared 1.0.0
//!              common     1.0.0 -> shared
//! tools.lock   consumer-b 1.0.0 -> shared      shared 2.0.0
//!              common     1.0.0 -> shared
//! ```
//!
//! `common` is pinned by BOTH resolves at the SAME version, so dedup collapses
//! its two entries into one component — and pre-#911 kept one resolve name and
//! dropped the other. Its dependency `shared` is pinned DIFFERENTLY by the two
//! resolves, which is the FR-011b case.

use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pants_resolve_edges")
}

fn scan_to(format: &str, file: &str) -> serde_json::Value {
    let out = tempfile::tempdir().expect("tempdir");
    let path = out.path().join(file);
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture().to_str().expect("fixture"),
            "--offline",
            "--format",
            format,
            "--output",
            path.to_str().expect("out"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");
    serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse")
}

fn scan() -> serde_json::Value {
    scan_to("cyclonedx-json", "actual.cdx.json")
}

/// Decode membership from a CycloneDX property.
///
/// CycloneDX spec'es `properties[].value` as a **string**, so an array is
/// carried as JSON-in-string: `"[\"app\",\"tools\"]"`. That is not a
/// waybill quirk — `waybill:source-files` and `waybill:file-paths` have always
/// been carried this way. SPDX 2.3 and SPDX 3 put a real JSON array inside
/// their annotation envelope, because their carriers permit it.
///
/// So "identical across formats" means identical **decoded value**, not
/// identical bytes; the encoding is each format's business.
fn membership(doc: &serde_json::Value, purl: &str) -> Option<Vec<String>> {
    let raw = membership_raw(doc, purl)?;
    let decoded: serde_json::Value = match raw {
        serde_json::Value::String(s) => serde_json::from_str(s).ok()?,
        other => other.clone(),
    };
    Some(
        decoded
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect(),
    )
}

/// The raw, still-encoded value — for the tests that care about the encoding
/// itself rather than what it decodes to.
fn membership_raw<'a>(doc: &'a serde_json::Value, purl: &str) -> Option<&'a serde_json::Value> {
    doc["components"].as_array()?.iter().find_map(|c| {
        if c["purl"].as_str()? != purl {
            return None;
        }
        c["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some("waybill:pants-resolve"))
            .map(|p| &p["value"])
    })
}

fn edges(doc: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dep in doc["dependencies"].as_array().into_iter().flatten() {
        let from = dep["ref"].as_str().unwrap_or_default().to_string();
        for to in dep["dependsOn"].as_array().into_iter().flatten() {
            out.push((from.clone(), to.as_str().unwrap_or_default().to_string()));
        }
    }
    out.sort();
    out
}

const COMMON: &str = "pkg:pypi/waybill-fixture-common@1.0.0";
const SHARED_1: &str = "pkg:pypi/waybill-fixture-shared@1.0.0";
const SHARED_2: &str = "pkg:pypi/waybill-fixture-shared@2.0.0";

/// T006b / FR-011b / SC-007a — guards behaviour that is **emergent, not
/// designed**, and therefore unprotected.
///
/// Edges are emitted per `PackageDbEntry` at `scan_fs/mod.rs:1081`, before
/// `deduplicate` at `:1253`. An entry comes from one lockfile, so the two
/// `common` entries each resolve `shared` inside their own resolve and emit
/// their own edge. Dedup then collapses the components but never touches
/// relationships, so both edges survive.
///
/// Nothing states that today. If someone moves dedup earlier, or makes edge
/// emission operate on deduplicated components, this silently drops to one
/// edge — and for a consumer matching advisories, the dropped edge is a
/// vulnerability in the other version that goes unattributed.
#[test]
fn a_component_in_two_resolves_reaches_both_pinnings_of_its_dependency() {
    let doc = scan();
    let edges = edges(&doc);
    for target in [SHARED_1, SHARED_2] {
        assert!(
            edges.contains(&(COMMON.to_string(), target.to_string())),
            "common is pinned by both resolves, and each pins `shared` \
             differently, so it depends on BOTH. Missing edge to {target}. \
             Edges emitted:\n{edges:#?}"
        );
    }
}

/// FR-001 / SC-001 — the defect. `common` is in both lockfiles; both resolves
/// must name it.
#[test]
fn membership_names_every_resolve_that_pins_the_package() {
    let doc = scan();
    let names = membership(&doc, COMMON)
        .unwrap_or_else(|| panic!("no membership on {COMMON}"));
    assert_eq!(
        names,
        vec!["app", "tools"],
        "common is pinned by app.lock AND tools.lock; dedup must union the \
         membership rather than keeping the winner's"
    );
}

/// FR-006a / SC-004 / C-1 — the array form is used even for one resolve.
/// A shape that varies with cardinality makes every consumer write two paths.
#[test]
fn a_single_resolve_component_uses_the_array_form_too() {
    let doc = scan();
    assert_eq!(
        membership(&doc, SHARED_1).as_deref(),
        Some(&["app".to_string()][..]),
        "a one-resolve component must still use the array encoding, not a \
         bare name. Raw value: {:?}",
        membership_raw(&doc, SHARED_1)
    );
}

/// C-1a — no emitted component may carry the pre-#911 bare-string form.
/// This, not the lenient accessor, is what catches a writer left behind.
#[test]
fn no_component_emits_the_pre_911_bare_string_form() {
    let doc = scan();
    // Every CDX property value is a string, so "is it a string" proves
    // nothing. The question is whether the string DECODES to an array — a
    // writer left on the old form emits `"app"`, which parses as a JSON
    // string, not as `["app"]`.
    let mut offenders: Vec<String> = Vec::new();
    for c in doc["components"].as_array().into_iter().flatten() {
        for p in c["properties"].as_array().into_iter().flatten() {
            if p["name"].as_str() != Some("waybill:pants-resolve") {
                continue;
            }
            let decodes_to_array = p["value"]
                .as_str()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .is_some_and(|v| v.is_array());
            if !decodes_to_array {
                offenders.push(format!(
                    "{} = {}",
                    c["purl"].as_str().unwrap_or("?"),
                    p["value"]
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these components still emit the bare-string form — a writer was \
         missed, and the lenient accessor will not catch it:\n{offenders:#?}"
    );
}

/// FR-003 / SC-003 — two scans of one repository agree byte-for-byte.
/// Order-dependence is how the defect hid: any single run looks consistent.
#[test]
fn membership_is_identical_across_repeated_scans() {
    let a = scan();
    let b = scan();
    for purl in [COMMON, SHARED_1, SHARED_2] {
        assert_eq!(
            membership(&a, purl),
            membership(&b, purl),
            "membership for {purl} differs between two scans of one repository"
        );
    }
}
