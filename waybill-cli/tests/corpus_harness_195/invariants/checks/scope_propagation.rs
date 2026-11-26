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
