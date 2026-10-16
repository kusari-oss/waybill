//! I3 — a component reachable from the subject only through non-runtime
//! edges carries a non-runtime marker.
//!
//! A development, test, build or optional dependency that reads as a
//! runtime one gives a consumer filtering to runtime components
//! something that is not in the shipped artifact.
//!
//! **Boundary with I7**: this invariant's population is components that
//! *are* reachable, by a path that is entirely non-runtime. I7's is
//! components not reachable at all. Every component is one or the other,
//! so a defect satisfies exactly one of the two preconditions — which
//! was not true of the first draft of these rows and is why two
//! instances of one defect class landed under the wrong number during
//! the investigation (FR-019).

use std::collections::{HashMap, HashSet, VecDeque};

use super::super::{CellOutcome, Violation};
use super::{
    cdx_adjacency, cdx_component_ref, cdx_components, cdx_is_non_runtime, cdx_subject_ref,
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

    let mut offenders: Vec<String> = all_reachable
        .iter()
        .filter(|r| !runtime_reachable.contains(*r) && !marked.contains(*r))
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
