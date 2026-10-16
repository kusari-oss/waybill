//! I1 — no dependency edge targets a file-tier component.
//!
//! A file is *contained in* a component, not *required by* it. A
//! `dependsOn` edge pointing at a file makes a consumer walking the
//! graph for a vulnerability-relevant dependency set pick up maintainer
//! scripts and shell snippets.

use super::super::{CellOutcome, Violation};
use super::{cdx_adjacency, cdx_component_ref, cdx_components, cdx_is_file_tier};

pub const KIND_FILE_TIER_DEPENDENCY_EDGE: &str = "file-tier-dependency-edge";
pub const KINDS: &[&str] = &[KIND_FILE_TIER_DEPENDENCY_EDGE];

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let cdx = &sboms.cdx;

    // Precondition FIRST, and it returns NotApplicable.
    let file_refs: std::collections::HashSet<&str> = cdx_components(cdx)
        .iter()
        .filter(|c| cdx_is_file_tier(c))
        .filter_map(cdx_component_ref)
        .collect();
    if file_refs.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "no file-tier components in this document".to_string(),
        };
    }

    let mut offenders: Vec<String> = Vec::new();
    for (from, tos) in cdx_adjacency(cdx) {
        for to in tos {
            if file_refs.contains(to) {
                offenders.push(format!("{from} -> {to}"));
            }
        }
    }
    if offenders.is_empty() {
        return CellOutcome::Pass;
    }
    offenders.sort();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_FILE_TIER_DEPENDENCY_EDGE,
        count: offenders.len(),
        example: offenders.first().cloned(),
    }])
}
