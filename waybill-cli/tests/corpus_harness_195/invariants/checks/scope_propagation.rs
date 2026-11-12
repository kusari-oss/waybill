//! I3 — a component reachable from the subject only through non-runtime
//! edges carries a non-runtime marker.
//!
//! A development, test, build or optional dependency that reads as a
//! runtime one gives a consumer filtering to runtime components
//! something that is not in the shipped artifact.
//!
//! **Exemption, milestone 1276 (#1340)**: a package the compiler ships is
//! excluded from this invariant's population. CycloneDX 1.6 defines
//! `optional` as "not capable of being called due to them **not being
//! installed**", and instructs that a component which IS installed but is
//! prohibited from being called "must be scoped as `required`"; `excluded` as
//! "not reachable within a call graph at runtime". A GHC boot library is
//! installed in every build and is in the call graph, so demanding a
//! non-runtime marker for it would demand a value the schema says is false.
//! Measured: without this exemption all **10** such components across the
//! corpus become violations of this row the moment milestone 1276 stops
//! marking them — 8 on `haskell-security-advisories`, 2 on
//! `nix-closure-moat` — and since this row has no declarations they would be
//! *undeclared*, which fails the lane.
//!
//! The exemption is identified the same way the propagation identifies it:
//! C170 `waybill:haskell-version-unresolved-reason = compiler-supplied`,
//! which the pass emits for exactly the packages the pinned revision's
//! compiler configuration nulls. **Both must read one source**, or this row
//! and the propagation disagree about one document and each is locally
//! satisfiable while the pair is not.
//!
//! **Boundary with the milestone-1276 complement**: that row asserts no
//! compiler-supplied component *carries* a non-runtime scope; this one
//! asserts every other reachable-only-through-non-runtime component *does*.
//! Together they partition, the way this row already partitions with I7.
//!
//! **Boundary with I7**: this invariant's population is components that
//! *are* reachable, by a path that is entirely non-runtime. I7's is
//! components not reachable at all. Every component is one or the other,
//! so a defect satisfies exactly one of the two preconditions — which
//! was not true of the first draft of these rows and is why two
//! instances of one defect class landed under the wrong number during
//! the investigation (FR-019).
//!
//! **What counts as the marker, #1283.** An edge is non-runtime when its
//! target carries C42 `waybill:lifecycle-scope`; that defines the
//! population and is unchanged. Whether a component in the population
//! *carries* a marker reads both spellings
//! ([`super::cdx_carries_non_runtime_marker`]): C42, or the native CycloneDX
//! `scope`. The not-in-artifact marker of milestones 1277 and 1423 is the
//! native field alone, set without C42 on purpose. Measured: every one of
//! the 48 offenders on the three IFD targets, and 15 of the 16 on
//! `nix-closure-moat`, were the source tarballs, `.cabal` revisions and
//! patches of test libraries (and four test libraries reached only through
//! the build graph), each already `scope: "excluded"` in CycloneDX,
//! `BUILD_TOOL_OF` in SPDX 2.3 and `usesTool` in SPDX 3. A consumer
//! filtering to runtime components drops every one of them, so the defect
//! this row catches is not there. moat's declaration had silently absorbed
//! the 15; it now accepts only `hspec-meta`, the defect it was written for,
//! which is unmarked in every format.

use std::collections::{HashMap, HashSet, VecDeque};

use super::super::{CellOutcome, Violation};
use super::{
    cdx_adjacency, cdx_carries_non_runtime_marker, cdx_component_ref, cdx_components,
    cdx_is_non_runtime, cdx_subject_ref,
};

pub const KIND_UNMARKED_NON_RUNTIME_REACHABLE: &str = "unmarked-non-runtime-reachable";
pub const KINDS: &[&str] = &[KIND_UNMARKED_NON_RUNTIME_REACHABLE];

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let cdx = &sboms.cdx;
    let components = cdx_components(cdx);

    let marked: HashSet<&str> = components
        .iter()
        .filter(|c| cdx_is_non_runtime(c))
        .filter_map(cdx_component_ref)
        .collect();

    // Precondition: the document must declare at least one non-runtime
    // edge. Seeding from *marked components* instead is what made the
    // first draft overlap with I7 — a population never marked at all
    // produced no seed and fell through.
    if marked.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "no non-runtime dependency edges declared in this document".to_string(),
        };
    }
    let Some(root) = cdx_subject_ref(cdx) else {
        return CellOutcome::NotApplicable { reason: "no resolvable subject".to_string() };
    };

    // Reachability restricted to runtime edges: an edge into a marked
    // component is a non-runtime edge and is not traversed.
    let adj: HashMap<&str, Vec<&str>> = cdx_adjacency(cdx);
    let mut runtime_reachable: HashSet<&str> = HashSet::new();
    let mut q: VecDeque<&str> = VecDeque::new();
    q.push_back(root);
    runtime_reachable.insert(root);
    while let Some(cur) = q.pop_front() {
        for next in adj.get(cur).into_iter().flatten() {
            if marked.contains(next) {
                continue;
            }
            if runtime_reachable.insert(next) {
                q.push_back(next);
            }
        }
    }

    // Anything reachable at all, but not by a runtime-only path, must be
    // marked. Unreachable components are I7's business, not ours.
    let mut all_reachable: HashSet<&str> = HashSet::new();
    let mut q2: VecDeque<&str> = VecDeque::new();
    q2.push_back(root);
    all_reachable.insert(root);
    while let Some(cur) = q2.pop_front() {
        for next in adj.get(cur).into_iter().flatten() {
            if all_reachable.insert(next) {
                q2.push_back(next);
            }
        }
    }

    // Milestone 1276 — the compiler-supplied exemption. See the module
    // comment: demanding a non-runtime marker for a package the compiler
    // ships would demand a value CycloneDX defines as false of it.
    //
    // Milestone 966: a boot library that took its compiler's version carries
    // C218 instead of C170, and is no less compiler-supplied.
    let compiler_supplied: HashSet<&str> = components
        .iter()
        .filter(|c| {
            super::cdx_prop(c, "waybill:haskell-version-unresolved-reason")
                == Some("compiler-supplied")
                || super::cdx_prop(c, "waybill:boot-library-compiler").is_some()
        })
        .filter_map(cdx_component_ref)
        .collect();

    // #1283 — the marker in either spelling (module docs). `marked` stays
    // C42-only because it decides which edges are non-runtime.
    let natively_marked: HashSet<&str> = components
        .iter()
        .filter(|c| cdx_carries_non_runtime_marker(c))
        .filter_map(cdx_component_ref)
        .collect();

    let mut offenders: Vec<String> = all_reachable
        .iter()
        .filter(|r| {
            !runtime_reachable.contains(*r)
                && !natively_marked.contains(*r)
                && !compiler_supplied.contains(*r)
        })
        .map(|r| (*r).to_string())
        .collect();
    if offenders.is_empty() {
        return CellOutcome::Pass;
    }
    offenders.sort();
    let count = offenders.len();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_UNMARKED_NON_RUNTIME_REACHABLE,
        count,
        example: offenders.into_iter().next(),
    }])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::corpus_harness_195::invariants::EmittedSboms;

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
}
