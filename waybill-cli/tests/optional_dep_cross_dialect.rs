//! Milestone 1272 T056 — the five-dialect agreement test.
//!
//! SC-002, SC-003 and contract C10. ONE dependency graph, expressed in
//! all five npm-ecosystem dialects waybill reads, scanned through the
//! compiled binary, asserting that all five produce the *same* optional
//! classification.
//!
//! This is the test the milestone exists to make passable. At its start
//! one dialect of five answered the optional axis; the other four
//! reported absence, and absence is indistinguishable from "not
//! optional" in every output format. A per-dialect test can only ever
//! show that a dialect answers. Only a cross-dialect one shows they
//! AGREE, which is the property an operator comparing two SBOMs of the
//! same project actually depends on.
//!
//! The graph, identical in all five fixtures under
//! `tests/fixtures/optional_dep/cross_dialect/`:
//!
//! ```text
//!   root --dependencies--------> runtime ----hard----> contested
//!   root --optionalDependencies-> optional --hard----> transitive
//!                                 optional --hard----> contested
//! ```
//!
//! and the one correct answer, which every dialect must reach by a
//! different route through a differently-shaped file:
//!
//! | package    | optional? | why |
//! |------------|-----------|-----|
//! | runtime    | no        | declared non-optional at the root |
//! | optional   | **yes**   | declared optional at the root |
//! | transitive | **yes**   | reachable only through an optional package (FR-002) |
//! | contested  | no        | a non-optional path reaches it (FR-006) |
//!
//! What makes this non-trivial is that no two dialects record the same
//! thing. Measured in `specs/1272-npm-optional-scope/measurements/
//! findings.md`:
//!
//! - **npm** and **pnpm v9** compute the closure themselves and write a
//!   per-package flag — so for them the fixtures carry the markers the
//!   managers were measured to write, and the reader's job is to find
//!   them (pnpm's only under `snapshots:`, and AND-folded across peer
//!   contexts).
//! - **bun**, **yarn Berry** and **yarn v1** record declarations and
//!   edges and never a closure. Probes E and G measured contested and
//!   optional-only graphs producing byte-identical package entries in
//!   bun and in v1. For those three the whole classification is
//!   waybill's own derivation.
//! - **yarn v1** cannot even answer the ROOT case from its lockfile:
//!   Probe G3 measured a root `optionalDependencies` declaration
//!   writing zero occurrences of the string `optional`. Its answer comes
//!   from `package.json` alone.
//!
//! So five agreeing answers here is five different derivations agreeing,
//! not one code path exercised five times.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The five dialects, by fixture directory name.
const DIALECTS: [&str; 5] = ["npm", "pnpm", "yarn_v1", "yarn_berry", "bun"];

/// The one correct optional set, for every dialect.
const EXPECTED_OPTIONAL: [&str; 2] = ["waybill-fixture-optional", "waybill-fixture-transitive"];

/// Packages that MUST NOT be classified optional in any dialect.
const EXPECTED_NOT_OPTIONAL: [&str; 2] = ["waybill-fixture-runtime", "waybill-fixture-contested"];

fn fixture_path(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/optional_dep/cross_dialect")
        .join(dialect)
}

fn run_scan(project_root: &Path) -> (Value, Value) {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let spdx23_path = out_dir.path().join("out.spdx.json");
    let result = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .arg("--offline")
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
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan of {} failed: stderr={}",
        project_root.display(),
        String::from_utf8_lossy(&result.stderr),
    );
    let cdx: Value = serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    let spdx23: Value = serde_json::from_slice(&std::fs::read(&spdx23_path).unwrap()).unwrap();
    (cdx, spdx23)
}

/// Every component the CDX document marks `scope: "excluded"`, sorted.
/// Only the four graph packages are considered, so a dialect's own
/// main-module component cannot affect the comparison.
fn optional_set(cdx: &Value) -> Vec<String> {
    let graph: std::collections::HashSet<&str> = EXPECTED_OPTIONAL
        .iter()
        .chain(EXPECTED_NOT_OPTIONAL.iter())
        .copied()
        .collect();
    let mut names: Vec<String> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .expect("CDX components")
        .iter()
        .filter(|c| c.get("scope").and_then(|v| v.as_str()) == Some("excluded"))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .filter(|n| graph.contains(n))
        .map(str::to_string)
        .collect();
    names.sort();
    names
}

fn component_names(cdx: &Value) -> std::collections::HashSet<String> {
    cdx.get("components")
        .and_then(|v| v.as_array())
        .expect("CDX components")
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect()
}

#[test]
fn t056_all_five_dialects_agree_on_the_optional_set() {
    let mut expected: Vec<String> = EXPECTED_OPTIONAL.iter().map(|s| s.to_string()).collect();
    expected.sort();

    // Collected rather than asserted per-iteration, so a disagreement
    // reports every dialect's answer at once. Reading one dialect's
    // failure in isolation does not say whether it is the outlier or
    // the only one that is right.
    let mut answers: Vec<(&str, Vec<String>)> = Vec::new();
    for dialect in DIALECTS {
        let (cdx, _) = run_scan(&fixture_path(dialect));

        // A dialect that emitted none of the graph's packages would
        // agree vacuously -- an empty optional set matches an empty
        // optional set. Pin the population first.
        let names = component_names(&cdx);
        for pkg in EXPECTED_OPTIONAL.iter().chain(EXPECTED_NOT_OPTIONAL.iter()) {
            assert!(
                names.contains(*pkg),
                "{dialect}: {pkg} is not in the emitted component set, so this \
                 dialect's agreement would be vacuous. Emitted: {names:?}"
            );
        }
        answers.push((dialect, optional_set(&cdx)));
    }

    let disagreeing: Vec<&(&str, Vec<String>)> =
        answers.iter().filter(|(_, set)| *set != expected).collect();
    assert!(
        disagreeing.is_empty(),
        "SC-002 / SC-003 / C10: all five dialects MUST classify the same \
         optional set for the same graph.\n  expected: {expected:?}\n  \
         every answer: {answers:?}\n  disagreeing: {disagreeing:?}"
    );
}

#[test]
fn t056_all_five_dialects_agree_on_the_derivation_annotation() {
    // FR-008 across the board. The value differs by dialect by design --
    // bun says `bun-optional-dependencies`, the rest reuse m180's
    // `npm-optional-dependencies` -- so what must agree is that the
    // annotation is PRESENT on exactly the optional packages and absent
    // on the others. Asserting presence rather than the string is the
    // point: a dialect that classified correctly but annotated nothing
    // would be invisible to a consumer filtering on the annotation.
    for dialect in DIALECTS {
        let (cdx, _) = run_scan(&fixture_path(dialect));
        let components = cdx.get("components").and_then(|v| v.as_array()).unwrap();
        for component in components {
            let Some(name) = component.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let has_derivation = component
                .get("properties")
                .and_then(|v| v.as_array())
                .map(|props| {
                    props.iter().any(|p| {
                        p.get("name").and_then(|v| v.as_str())
                            == Some("waybill:optional-derivation")
                    })
                })
                .unwrap_or(false);
            if EXPECTED_OPTIONAL.contains(&name) {
                assert!(
                    has_derivation,
                    "{dialect}: {name} is optional and MUST carry \
                     waybill:optional-derivation (FR-008)"
                );
            } else if EXPECTED_NOT_OPTIONAL.contains(&name) {
                assert!(
                    !has_derivation,
                    "{dialect}: {name} is NOT optional and MUST NOT carry \
                     waybill:optional-derivation"
                );
            }
        }
    }
}

#[test]
fn t056_all_five_dialects_agree_in_spdx_2_3() {
    // SC-003 in the second format. CDX agreement alone would not show
    // that the two emitters agree with each other -- m1272's whole
    // premise is that one format can be silent where another speaks.
    let mut expected: Vec<String> = EXPECTED_OPTIONAL.iter().map(|s| s.to_string()).collect();
    expected.sort();

    let mut answers: Vec<(&str, Vec<String>)> = Vec::new();
    for dialect in DIALECTS {
        let (_, spdx23) = run_scan(&fixture_path(dialect));
        let packages = spdx23
            .get("packages")
            .and_then(|v| v.as_array())
            .expect("SPDX 2.3 packages");
        // SPDXID -> name, so the relationship's element ref can be
        // resolved back to a package name.
        let id_to_name: std::collections::HashMap<&str, &str> = packages
            .iter()
            .filter_map(|p| {
                Some((
                    p.get("SPDXID").and_then(|v| v.as_str())?,
                    p.get("name").and_then(|v| v.as_str())?,
                ))
            })
            .collect();
        let graph: std::collections::HashSet<&str> = EXPECTED_OPTIONAL
            .iter()
            .chain(EXPECTED_NOT_OPTIONAL.iter())
            .copied()
            .collect();
        let mut names: Vec<String> = spdx23
            .get("relationships")
            .and_then(|v| v.as_array())
            .expect("SPDX 2.3 relationships")
            .iter()
            .filter(|r| {
                r.get("relationshipType").and_then(|v| v.as_str())
                    == Some("OPTIONAL_DEPENDENCY_OF")
            })
            .filter_map(|r| r.get("spdxElementId").and_then(|v| v.as_str()))
            .filter_map(|id| id_to_name.get(id).copied())
            .filter(|n| graph.contains(n))
            .map(str::to_string)
            .collect();
        names.sort();
        names.dedup();
        answers.push((dialect, names));
    }

    let disagreeing: Vec<&(&str, Vec<String>)> =
        answers.iter().filter(|(_, set)| *set != expected).collect();
    assert!(
        disagreeing.is_empty(),
        "SC-003: all five dialects MUST emit the same OPTIONAL_DEPENDENCY_OF \
         set in SPDX 2.3.\n  expected: {expected:?}\n  every answer: \
         {answers:?}\n  disagreeing: {disagreeing:?}"
    );
}
