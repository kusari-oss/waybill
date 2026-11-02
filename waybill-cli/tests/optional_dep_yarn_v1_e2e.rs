//! Milestone 181 US1 — end-to-end integration tests for the yarn v1
//! optional-dep classifier. Scans the m181 fixture
//! (`tests/fixtures/optional_dep/yarn-v1/`) via the compiled `waybill`
//! binary and asserts the same shape m180 established for npm/pnpm.
//!
//! Contract references:
//! - `specs/181-yarn-optional-dep/contracts/yarn-classifier-extension.md`
//! - `specs/181-yarn-optional-dep/spec.md` FR-001, FR-002, FR-008..FR-011
//! - Success criteria SC-001, SC-006

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/yarn-v1")
}

fn run_scan(project_root: &Path, extra_args: &[&str]) -> (Value, Value) {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let spdx23_path = out_dir.path().join("out.spdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()));
    for arg in extra_args {
        cmd.arg(arg);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let cdx: Value = serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    let spdx23: Value = serde_json::from_slice(&std::fs::read(&spdx23_path).unwrap()).unwrap();
    (cdx, spdx23)
}

fn find_component_by_name<'a>(cdx: &'a Value, name: &str) -> Option<&'a Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn find_property<'a>(component: &'a Value, name: &str) -> Option<&'a Value> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
}

#[test]
fn t011_yarn_v1_optional_full_mode_end_to_end() {
    let (cdx, spdx23) = run_scan(&fixture_path(), &[]);

    // ---- CDX 1.6 ----
    let optional_child = find_component_by_name(&cdx, "optional-child-lib")
        .expect("optional-child-lib component in CDX");
    assert_eq!(
        optional_child.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "yarn v1 optional-classified optional-child-lib MUST emit CDX scope: \"excluded\""
    );
    assert_eq!(
        find_property(optional_child, "waybill:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "yarn v1 optional-classified optional-child-lib MUST carry waybill:optional-derivation"
    );

    // Regression guard: runtime-util (runtime) MUST stay Runtime.
    let runtime = find_component_by_name(&cdx, "runtime-util")
        .expect("runtime-util component in CDX");
    assert!(
        runtime.get("scope").and_then(|v| v.as_str()) != Some("excluded"),
        "runtime-util MUST NOT be marked excluded (regular runtime dep)"
    );
    assert!(
        find_property(runtime, "waybill:optional-derivation").is_none(),
        "runtime-util MUST NOT carry waybill:optional-derivation"
    );

    // ---- SPDX 2.3 (Full mode) ----
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let has_optional_dep_of = rels.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str()) == Some("OPTIONAL_DEPENDENCY_OF")
    });
    assert!(
        has_optional_dep_of,
        "SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge under Full mode"
    );
}

#[test]
fn t011b_yarn_v1_optional_basic_mode_collapses() {
    // FR-010 / SC-006 — under --spdx2-relationship-compat=basic, ALL
    // typed dep-scope edges collapse to natural-direction DEPENDS_ON
    // per m228's contract. CDX emission is INDEPENDENT of the compat
    // flag; annotation is orthogonal.
    let (cdx, spdx23) = run_scan(&fixture_path(), &["--spdx2-relationship-compat=basic"]);

    // CDX side unchanged — scope: "excluded" still emitted.
    let optional_child = find_component_by_name(&cdx, "optional-child-lib")
        .expect("optional-child-lib component in CDX");
    assert_eq!(
        optional_child.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX scope emission is INDEPENDENT of --spdx2-relationship-compat"
    );

    // Annotation still present (orthogonal).
    assert!(
        find_property(optional_child, "waybill:optional-derivation").is_some(),
        "waybill:optional-derivation MUST be present in CDX regardless of compat mode"
    );

    // SPDX 2.3 has zero OPTIONAL_DEPENDENCY_OF edges under basic mode.
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let optional_count = rels
        .iter()
        .filter(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        })
        .count();
    assert_eq!(
        optional_count, 0,
        "SPDX 2.3 basic mode MUST NOT emit any OPTIONAL_DEPENDENCY_OF edges (m228 escape hatch)"
    );
}

// ────────────────────────────────────────────────────────────────────
// Milestone 1272 US4 — the optional AXIS for yarn v1.
//
// m181 (above) read the per-entry `optionalDependencies:` sub-block and
// subtracted the regular children one level deep. Two things were
// missing, both measured in `measurements/yarn-v1-probe.sh` (Probe G):
//
//   * the ROOT declaration, which writes zero occurrences of the string
//     `optional` into a v1 lockfile (G3), so `package.json` is its only
//     possible source -- yarn v1 is the only arm of the four for which
//     that is true; and
//   * the CLOSURE, because a contested package and an optional-only one
//     produce a byte-identical entry (G2), so there is nothing in the
//     file to read and the answer has to be derived.
// ────────────────────────────────────────────────────────────────────

fn measured_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/optional_dep/yarn_v1_measured")
}

/// The exact optional set the measured fixture must produce (SC-001).
const M1272_US4_EXPECTED_OPTIONAL: [&str; 4] = [
    // root `optionalDependencies` declaration -- FR-005, answerable
    // only from the manifest.
    "waybill-fixture-optroot",
    // its hard child, reachable only through that optional root.
    "waybill-fixture-optchild",
    // a transitive `optionalDependencies:` sub-block target.
    "waybill-fixture-transopt",
    // and that target's own hard child -- the transitive step the
    // one-level subtraction could not take.
    "waybill-fixture-deepopt",
];

#[test]
fn m1272_us4_v1_root_declaration_is_classified_optional() {
    // T048 -- contract C1, FR-005, FR-008, acceptance scenario 1.
    let (cdx, spdx23) = run_scan(&measured_fixture_path(), &[]);

    let optroot = find_component_by_name(&cdx, "waybill-fixture-optroot")
        .expect("waybill-fixture-optroot component in CDX");
    assert_eq!(
        optroot.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "a yarn v1 ROOT optionalDependencies declaration MUST emit CDX \
         scope: \"excluded\" (FR-005). Probe G3 measured the lockfile \
         carrying no trace of it, so a reader that consults only the \
         lockfile reports absence here."
    );
    assert_eq!(
        find_property(optroot, "waybill:optional-derivation").and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "FR-008: the root declaration MUST carry waybill:optional-derivation, \
         reusing the value m180 established (no new catalog row)"
    );

    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    assert!(
        rels.iter().any(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        }),
        "SPDX 2.3 MUST emit OPTIONAL_DEPENDENCY_OF for the v1 root declaration"
    );
}

#[test]
fn m1272_us4_v1_transitive_optional_and_runtime_cancellation() {
    // T049 -- contracts C2 and C3, FR-002, FR-006, acceptance scenario 2.
    let (cdx, _spdx23) = run_scan(&measured_fixture_path(), &[]);

    // C2: reachable only through optional edges -> optional. Both the
    // sub-block target and ITS hard child, which is the transitive step.
    for name in [
        "waybill-fixture-optchild",
        "waybill-fixture-transopt",
        "waybill-fixture-deepopt",
    ] {
        let c = find_component_by_name(&cdx, name)
            .unwrap_or_else(|| panic!("{name} component in CDX"));
        assert_eq!(
            c.get("scope").and_then(|v| v.as_str()),
            Some("excluded"),
            "{name} is reachable only through an optional edge and MUST be \
             classified optional (FR-002). The one-level subtraction m181 \
             shipped classifies the sub-block target and stops."
        );
    }

    // C3: a non-optional path cancels. `waybill-fixture-shared` is a
    // hard child of the runtime root AND a hard child of the optional
    // `transopt`. Probe G2 measured its entry byte-identical in both
    // arrangements, so this answer is derived, never read.
    let shared = find_component_by_name(&cdx, "waybill-fixture-shared")
        .expect("waybill-fixture-shared component in CDX");
    assert_ne!(
        shared.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "FR-006: waybill-fixture-shared is reachable by a non-optional path \
         from the root and MUST NOT be classified optional"
    );
    assert!(
        find_property(shared, "waybill:optional-derivation").is_none(),
        "FR-006: a cancelled package MUST NOT carry the derivation annotation"
    );

    // And the runtime root itself.
    let runtime = find_component_by_name(&cdx, "waybill-fixture-runtime")
        .expect("waybill-fixture-runtime component in CDX");
    assert_ne!(
        runtime.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "a plain root dependency MUST NOT be classified optional"
    );
}

#[test]
fn m1272_us4_v1_exact_optional_set() {
    // SC-001 / T057 -- the optional set is EXACTLY the four expected
    // names. An over-broad closure is the failure mode this arm risks
    // most (a root-seeded closure over an absent manifest would read
    // every package as optional), and only an exact-set assertion
    // catches it; a per-name check would pass while the set grew.
    let (cdx, _spdx23) = run_scan(&measured_fixture_path(), &[]);
    let mut actual: Vec<String> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .expect("CDX components")
        .iter()
        .filter(|c| c.get("scope").and_then(|v| v.as_str()) == Some("excluded"))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    actual.sort();
    let mut expected: Vec<String> = M1272_US4_EXPECTED_OPTIONAL
        .iter()
        .map(|s| s.to_string())
        .collect();
    expected.sort();
    assert_eq!(
        actual, expected,
        "SC-001: the yarn v1 optional set MUST be exactly the declared \
         closure -- no more (over-broad closure) and no fewer (one-level \
         subtraction)"
    );
}

#[test]
fn m1272_us4_v1_no_optional_declarations_is_unchanged() {
    // T053 -- acceptance scenario 3. The m181 fixture carries no ROOT
    // optionalDependencies at all, so this arm must not move it: its
    // one optional classification comes from the transitive sub-block
    // and nothing else may join it.
    let (cdx, _spdx23) = run_scan(&fixture_path(), &[]);
    let mut excluded: Vec<&str> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .expect("CDX components")
        .iter()
        .filter(|c| c.get("scope").and_then(|v| v.as_str()) == Some("excluded"))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    excluded.sort();
    assert_eq!(
        excluded,
        vec!["optional-child-lib"],
        "the m181 v1 fixture's optional set MUST be unchanged by US4 -- \
         `transitive-helper` is a HARD child of a runtime root and must \
         not be swept in by the new closure"
    );
}
