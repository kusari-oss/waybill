//! I9 — a component the compiler supplies carries no non-runtime scope.
//!
//! Milestone 1276 (#1340). The complement of I3, and the two partition: I3
//! asserts that a component reachable only through non-runtime edges carries
//! a non-runtime marker *unless the compiler supplies it*; this one asserts
//! that one which the compiler supplies carries no such marker at all.
//! Together every component satisfies exactly one of the two preconditions,
//! which is the discipline I3 already documents for its boundary with I7 —
//! and which it records getting wrong once, when a loose boundary put two
//! instances of one defect class under the wrong number.
//!
//! **Why the claim is a conformance claim, not a preference.** CycloneDX 1.6
//! defines `optional` as "not capable of being called due to them **not being
//! installed** or otherwise accessible by any means", and instructs that a
//! component which IS installed but is prohibited from being called "must be
//! scoped as `required`"; `excluded` as "not reachable within a call graph at
//! runtime". A package the compiler ships is installed in every build and is
//! in the artifact's call graph, so both values are false of it by the
//! schema's own text.
//!
//! **Why it exists although the population is zero.** Measured at the point
//! milestone 1276 landed, all 10 affected components derived their scope
//! transitively, so the propagation's immunity is the whole fix and nothing
//! else can currently assign one. This row covers the origin the fix does not
//! reach: a direct mark, or a producer this milestone did not anticipate.
//! Without it such a scope would be a silent defect rather than a failing
//! invariant — and milestone 1275's F12 is the precedent, where a new
//! producer reached a population nobody had connected it to.

use std::collections::HashSet;

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_components, cdx_is_non_runtime, cdx_prop};

pub const KIND_COMPILER_SUPPLIED_NON_RUNTIME: &str = "compiler-supplied-non-runtime";
pub const KINDS: &[&str] = &[KIND_COMPILER_SUPPLIED_NON_RUNTIME];

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = cdx_components(&sboms.cdx);

    // The population is the components the pinned revision's compiler
    // supplies, identified by C170 — the same signal the propagation's
    // immunity reads. Both must read one source, or this row and the
    // propagation disagree about one document and each is locally
    // satisfiable while the pair is not.
    //
    // Milestone 966: a boot library that took its compiler's version carries
    // C218 (`waybill:boot-library-compiler`) instead of C170 and remains
    // compiler-supplied, so both signals define the population.
    let compiler_supplied: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| {
            cdx_prop(c, "waybill:haskell-version-unresolved-reason") == Some("compiler-supplied")
                || cdx_prop(c, "waybill:boot-library-compiler").is_some()
        })
        .collect();

    if compiler_supplied.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "no compiler-supplied components in this document".to_string(),
        };
    }

    let offenders: HashSet<&str> = compiler_supplied
        .iter()
        .filter(|c| cdx_is_non_runtime(c))
        .filter_map(|c| cdx_component_ref(c))
        .collect();

    if offenders.is_empty() {
        return CellOutcome::Pass;
    }

    let mut refs: Vec<String> = offenders.into_iter().map(str::to_string).collect();
    refs.sort();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_COMPILER_SUPPLIED_NON_RUNTIME,
        count: refs.len(),
        // The first offender by sorted PURL, so the example is stable across
        // runs rather than whichever the hash set yielded first.
        example: refs.first().cloned(),
    }])
}
