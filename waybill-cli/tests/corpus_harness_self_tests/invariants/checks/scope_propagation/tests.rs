#[allow(unused_imports)]
use std::collections::{HashMap, HashSet, VecDeque};
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::{CellOutcome, Violation};
#[allow(unused_imports)]
use crate::corpus_harness_195::invariants::checks::{
    cdx_adjacency, cdx_carries_non_runtime_marker, cdx_component_ref, cdx_components,
    cdx_is_non_runtime, cdx_subject_ref,
};
use crate::corpus_harness_195::invariants::checks::scope_propagation::*;
use crate::corpus_harness_195::harness::EmittedSboms;

/// A document in which the subject reaches `boot` only through a
/// non-runtime component, and `boot` is compiler-supplied.
fn doc_with_compiler_supplied_behind_non_runtime(mark_boot: bool) -> serde_json::Value {
    let boot_props = if mark_boot {
        serde_json::json!([
            { "name": "waybill:haskell-version-unresolved-reason", "value": "compiler-supplied" }
        ])
    } else {
        serde_json::json!([])
    };
    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": { "component": { "bom-ref": "pkg:hackage/app@1.0.0" } },
        "components": [
            // `cdx_is_non_runtime` reads the C42 property, not the
            // native `scope` field, so the fixture must carry it.
            { "bom-ref": "pkg:hackage/testdep@1.0.0", "name": "testdep",
              "purl": "pkg:hackage/testdep@1.0.0", "type": "library",
              "scope": "excluded",
              "properties": [
                { "name": "waybill:lifecycle-scope", "value": "test" }
              ] },
            { "bom-ref": "pkg:hackage/boot@1.0.0", "name": "boot",
              "purl": "pkg:hackage/boot@1.0.0", "type": "library",
              "properties": boot_props },
        ],
        "dependencies": [
            { "ref": "pkg:hackage/app@1.0.0",
              "dependsOn": ["pkg:hackage/testdep@1.0.0"] },
            { "ref": "pkg:hackage/testdep@1.0.0",
              "dependsOn": ["pkg:hackage/boot@1.0.0"] },
        ],
    })
}

fn sboms(cdx: serde_json::Value) -> EmittedSboms {
    use std::path::PathBuf;
    EmittedSboms {
        cdx,
        spdx_2_3: serde_json::json!({}),
        spdx_3: serde_json::json!({}),
        // This invariant reads only the CycloneDX document; the paths are
        // part of the struct's shape and nothing here opens them.
        paths: crate::corpus_harness_195::harness::EmittedPaths {
            cdx: PathBuf::new(),
            spdx_2_3: PathBuf::new(),
            spdx_3: PathBuf::new(),
        },
    }
}

/// Milestone 1276 (#1340) — a compiler-supplied component is exempt from
/// this invariant even though it is reachable only through a non-runtime
/// edge and carries no marker.
///
/// **This test exists because the corpus cannot catch its removal.**
/// Mutation M5 — deleting the exemption — SURVIVED against
/// `public_corpus`, and not because the exemption is untested but because
/// the committed goldens still carry the `excluded` markers the exemption
/// exists to stop requiring. With the markers present this row passes
/// either way; it will only distinguish them after the goldens are
/// refreshed. A synthetic document distinguishes them now.
#[test]
fn m1276_a_compiler_supplied_component_is_exempt() {
    let out = check(&sboms(doc_with_compiler_supplied_behind_non_runtime(true)));
    assert!(
        matches!(out, CellOutcome::Pass),
        "a compiler-supplied component was demanded a non-runtime marker: {out:?}"
    );
}

/// The control. The same document with the C170 marker removed must
/// FAIL — otherwise the test above would pass for a checker that exempts
/// everything, which is the vacuity this pair exists to exclude.
#[test]
fn m1276_without_the_marker_the_same_component_is_an_offender() {
    let out = check(&sboms(doc_with_compiler_supplied_behind_non_runtime(false)));
    match out {
        CellOutcome::Fail(v) => {
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].kind, KIND_UNMARKED_NON_RUNTIME_REACHABLE);
        }
        other => panic!("expected a failure without the marker, got {other:?}"),
    }
}

/// The nix-closure shape #1283 measured: a test library the propagation
/// scoped `test` (C42), and its source tarball, reached from it through
/// a build-graph edge. The tarball carries no C42; the runtime closure
/// marked it not-in-artifact, which is the native `scope` alone.
fn doc_with_source_behind_test_library(native_scope: Option<&str>) -> serde_json::Value {
    let mut tarball = serde_json::json!({
        "bom-ref": "pkg:generic/testdep-1.0.0.tar.gz-sha256.x",
        "name": "testdep-1.0.0.tar.gz",
        "purl": "pkg:generic/testdep-1.0.0.tar.gz-sha256.x",
        "type": "library",
        "properties": [
            { "name": "waybill:scope-classification-source", "value": "runtime-closure" }
        ],
    });
    if let Some(s) = native_scope {
        tarball["scope"] = serde_json::Value::String(s.to_string());
    }
    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": { "component": { "bom-ref": "pkg:hackage/app@1.0.0" } },
        "components": [
            { "bom-ref": "pkg:hackage/testdep@1.0.0", "name": "testdep",
              "purl": "pkg:hackage/testdep@1.0.0", "type": "library",
              "scope": "excluded",
              "properties": [
                { "name": "waybill:lifecycle-scope", "value": "test" }
              ] },
            tarball,
        ],
        "dependencies": [
            { "ref": "pkg:hackage/app@1.0.0",
              "dependsOn": ["pkg:hackage/testdep@1.0.0"] },
            { "ref": "pkg:hackage/testdep@1.0.0",
              "dependsOn": ["pkg:generic/testdep-1.0.0.tar.gz-sha256.x"] },
        ],
    })
}

/// #1283 — the native not-in-artifact marker is a non-runtime marker.
#[test]
fn m1283_a_natively_excluded_component_carries_the_marker() {
    let out = check(&sboms(doc_with_source_behind_test_library(Some("excluded"))));
    assert!(matches!(out, CellOutcome::Pass), "native `scope: excluded` was not read: {out:?}");
}

/// The control: the same document with the tarball unmarked fails, so
/// the test above is not passing because the row stopped looking.
#[test]
fn m1283_without_any_marker_the_same_component_is_an_offender() {
    for scope in [None, Some("required")] {
        match check(&sboms(doc_with_source_behind_test_library(scope))) {
            CellOutcome::Fail(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].kind, KIND_UNMARKED_NON_RUNTIME_REACHABLE);
                assert_eq!(v[0].count, 1);
            }
            other => panic!("expected the tarball to be reported with scope {scope:?}, got {other:?}"),
        }
    }
}

/// The population is unchanged: an edge into a component carrying only
/// the native marker is still a runtime edge, so what is reached through
/// it is not demanded a marker. Widening the population as well would
/// change which components I3 covers.
#[test]
fn m1283_a_native_marker_alone_does_not_make_an_edge_non_runtime() {
    let doc = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "metadata": { "component": { "bom-ref": "pkg:hackage/app@1.0.0" } },
        "components": [
            { "bom-ref": "pkg:hackage/testdep@1.0.0", "name": "testdep",
              "purl": "pkg:hackage/testdep@1.0.0", "type": "library",
              "scope": "excluded",
              "properties": [ { "name": "waybill:lifecycle-scope", "value": "test" } ] },
            { "bom-ref": "pkg:generic/tool@1", "name": "tool",
              "purl": "pkg:generic/tool@1", "type": "library", "scope": "excluded" },
            { "bom-ref": "pkg:generic/lib@1", "name": "lib",
              "purl": "pkg:generic/lib@1", "type": "library" },
        ],
        "dependencies": [
            { "ref": "pkg:hackage/app@1.0.0",
              "dependsOn": ["pkg:hackage/testdep@1.0.0", "pkg:generic/tool@1"] },
            { "ref": "pkg:generic/tool@1", "dependsOn": ["pkg:generic/lib@1"] },
        ],
    });
    assert_eq!(check(&sboms(doc)), CellOutcome::Pass);
}
