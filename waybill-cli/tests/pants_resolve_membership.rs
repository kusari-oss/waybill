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

/// m922 — scan an arbitrary Pants fixture unsplit, for the pairing check.
fn scan_unsplit(fixture_name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture_name);
    let out = tempfile::tempdir().expect("tempdir");
    let file = out.path().join("actual.cdx.json");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            path.to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            file.to_str().expect("out"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed for {fixture_name}: {status}");
    serde_json::from_slice(&std::fs::read(&file).expect("read")).expect("parse")
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

// ---------------------------------------------------------------
// Issue #919 (m922) — the per-component namespace
// ---------------------------------------------------------------

/// T008 — C-1 and FR-007. Every component that carries resolve membership
/// carries exactly one namespace, and every component that carries no
/// membership carries no namespace.
///
/// The absence half is the part that matters most: absence must mean exactly
/// one thing ("this component is in no Pants resolve"), which is also what
/// makes a document produced before this feature readable — the field is
/// missing rather than blank. And this is what stops a reader being added
/// later that writes membership without a namespace, which would silently
/// produce components the split cannot place.
#[test]
fn every_component_with_membership_has_exactly_one_namespace() {
    const MEMBERSHIP: &str = "waybill:pants-resolve";
    const NAMESPACE: &str = "waybill:pants-resolve-namespace";
    const KNOWN: [&str; 2] = ["python", "jvm"];

    for fixture in [
        "pants_resolve_edges",
        "pants_discovered_resolves",
        "pants_namespace_collision",
        "pants_coursier_jvm/multi_resolve",
        // NOT bare `pants_pex` — that directory is a CONTAINER of sub-fixtures
        // (not_pants/, malformed_pants_toml/, …), so scanning its root finds one
        // stray component and no resolve membership at all. The vacuous-pass
        // guard below caught that on the first run.
        "pants_pex/multi_resolve_map",
    ] {
        let doc = scan_unsplit(fixture);
        let mut with_membership = 0usize;

        for c in doc["components"].as_array().into_iter().flatten() {
            let props: std::collections::BTreeMap<&str, &str> = c["properties"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some((p["name"].as_str()?, p["value"].as_str()?)))
                .collect();

            let purl = c["purl"].as_str().unwrap_or("<no purl>");
            match (props.get(MEMBERSHIP), props.get(NAMESPACE)) {
                (Some(_), Some(ns)) => {
                    with_membership += 1;
                    assert!(
                        KNOWN.contains(ns),
                        "{fixture}: {purl} has namespace {ns:?}, outside the closed set"
                    );
                }
                (Some(_), None) => panic!(
                    "{fixture}: {purl} carries resolve membership but NO namespace — \
                     the split cannot place it, and #919 is exactly what happens when \
                     a resolve cannot be told apart from another of the same name"
                ),
                (None, Some(ns)) => panic!(
                    "{fixture}: {purl} carries namespace {ns:?} but no membership — \
                     absence of membership must mean absence of namespace, or absence \
                     stops meaning one thing (FR-007)"
                ),
                (None, None) => {}
            }
        }

        assert!(
            with_membership > 0,
            "{fixture}: no component carried resolve membership, so this fixture \
             asserted nothing — a vacuous pass, not a passing test"
        );
    }
}

/// T009 — FR-006a / SC-007 / contract C-4. **The additive guarantee.**
///
/// `waybill:pants-resolve` keeps its v0.9.0 key and its array-of-BARE-names
/// value. This is the promise the entire design choice rests on: the namespace
/// went into a *new* annotation specifically so this field would not change a
/// second time in one release cycle — v0.9.0 had just changed it from a bare
/// string to an array, and asking consumers to absorb another change to the
/// same key immediately afterwards was the alternative that was rejected.
///
/// Nothing else in the suite checks it. The split-output byte-identity test
/// covers a different artifact.
#[test]
fn membership_keeps_its_v090_key_and_bare_name_shape() {
    let doc = scan_unsplit("pants_resolve_edges");
    let mut checked = 0usize;

    for c in doc["components"].as_array().into_iter().flatten() {
        let Some(raw) = c["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"].as_str() == Some("waybill:pants-resolve"))
            .and_then(|p| p["value"].as_str())
        else {
            continue;
        };
        checked += 1;
        let purl = c["purl"].as_str().unwrap_or("<no purl>");

        // Still an array, not a bare string (the v0.9.0 shape).
        let names: Vec<String> = serde_json::from_str(raw).unwrap_or_else(|e| {
            panic!("{purl}: membership {raw:?} is no longer a JSON array: {e}")
        });

        // Still BARE names. If the namespace had been folded in here instead
        // of riding alongside, these would read `python:app` and every
        // existing consumer would break.
        for n in &names {
            assert!(
                !n.contains(':'),
                "{purl}: membership name {n:?} is namespace-qualified. The namespace \
                 belongs in waybill:pants-resolve-namespace; qualifying this field \
                 would be the second breaking change to it in one release cycle"
            );
        }
    }

    assert!(
        checked > 0,
        "no component carried membership — this asserted nothing"
    );
}
