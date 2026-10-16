//! I4 — CycloneDX and SPDX 2.3 agree on the subject's direct dependency
//! set.
//!
//! If the two formats disagree about what the thing being described
//! directly depends on, whichever one a consumer reads they get a
//! different answer to the same question and only one can be right.
//!
//! **Read the SPDX side correctly before concluding there is a defect.**
//! SPDX 2.3 expresses non-runtime dependencies with *inverse*
//! relationship types that place the subject in `relatedSpdxElement`;
//! `super::spdx_dependency_edges` normalises them. Treating only
//! `DEPENDS_ON` as an edge fabricates a mismatch on every target with
//! any scoped dependency.

use std::collections::BTreeSet;

use super::super::{CellOutcome, Violation};
use super::{
    cdx_adjacency, cdx_component_ref, cdx_components, cdx_subject_purl, cdx_subject_ref,
    spdx_subject_direct_purls, spdx_subject_id,
};

pub const KIND_ONLY_IN_CDX: &str = "root-direct-only-in-cdx";
pub const KIND_ONLY_IN_SPDX: &str = "root-direct-only-in-spdx";
pub const KINDS: &[&str] = &[KIND_ONLY_IN_CDX, KIND_ONLY_IN_SPDX];

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let cdx = &sboms.cdx;
    let spdx = &sboms.spdx_2_3;

    let Some(cdx_root) = cdx_subject_ref(cdx) else {
        return CellOutcome::NotApplicable {
            reason: "CycloneDX document has no resolvable subject".to_string(),
        };
    };
    if spdx_subject_id(spdx).is_none() {
        return CellOutcome::NotApplicable {
            reason: "SPDX 2.3 document has no resolvable subject".to_string(),
        };
    }
    let Some(spdx_direct) = spdx_subject_direct_purls(spdx) else {
        return CellOutcome::NotApplicable {
            reason: "SPDX 2.3 subject direct set not resolvable".to_string(),
        };
    };

    // CDX side: subject's dependsOn refs mapped to PURLs, which is the
    // only identity the two formats share.
    let ref_to_purl: std::collections::HashMap<&str, &str> = cdx_components(cdx)
        .iter()
        .filter_map(|c| {
            let r = cdx_component_ref(c)?;
            let p = c.get("purl").and_then(|p| p.as_str())?;
            Some((r, p))
        })
        .collect();
    let subject_purl = cdx_subject_purl(cdx);
    let adj = cdx_adjacency(cdx);
    let cdx_direct: BTreeSet<String> = adj
        .get(cdx_root)
        .into_iter()
        .flatten()
        .filter_map(|r| ref_to_purl.get(r).map(|p| (*p).to_string()))
        .collect();

    // The subject itself is not its own dependency in either format;
    // drop it if a self-edge exists so the comparison is apples to
    // apples.
    let strip = |s: BTreeSet<String>| -> BTreeSet<String> {
        match subject_purl {
            Some(sp) => s.into_iter().filter(|p| p != sp).collect(),
            None => s,
        }
    };
    let cdx_direct = strip(cdx_direct);
    let spdx_direct = strip(spdx_direct);

    if cdx_direct.is_empty() && spdx_direct.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "subject has no direct dependencies in either format".to_string(),
        };
    }

    let only_cdx: Vec<&String> = cdx_direct.difference(&spdx_direct).collect();
    let only_spdx: Vec<&String> = spdx_direct.difference(&cdx_direct).collect();
    if only_cdx.is_empty() && only_spdx.is_empty() {
        return CellOutcome::Pass;
    }

    let mut violations = Vec::new();
    if !only_cdx.is_empty() {
        violations.push(Violation {
            kind: KIND_ONLY_IN_CDX,
            count: only_cdx.len(),
            example: only_cdx.first().map(|s| (*s).clone()),
        });
    }
    if !only_spdx.is_empty() {
        violations.push(Violation {
            kind: KIND_ONLY_IN_SPDX,
            count: only_spdx.len(),
            example: only_spdx.first().map(|s| (*s).clone()),
        });
    }
    CellOutcome::Fail(violations)
}
