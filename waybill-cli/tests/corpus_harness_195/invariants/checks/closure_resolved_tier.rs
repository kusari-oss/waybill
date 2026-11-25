//! I11 — a component the closure resolved is not labelled `design`
//! (spec 1650, #1681).
//!
//! **Catches**: in a `--nix-closure` document,
//!
//! - (a) a component that is `design` while the closure is evidence it is in
//!   the build graph: C203 `exempt-evaluated-package` (the closure evaluated
//!   it) or C218 `waybill:boot-library-compiler` (a closure compiler supplies
//!   it);
//! - (b) a root whose direct dependencies are all `design`, which is the
//!   shape that left a consumer dropping `design` with nothing at all (moat,
//!   openai, slack-web and slacklinker at `main` `18691d8`).
//!
//! **Boundary with I9 and I10**: those assert *scope* (`optional`,
//! `excluded`) and read C203 and C218 to decide which scope a member may
//! carry. This row asserts *tier* and reads the same two annotations as
//! evidence of resolution. The rows share inputs, not claims.
//!
//! **Not applicable** to a document with no closure member (no component
//! carrying C214 `waybill:closure-contents`): there `design` is correct by
//! definition, because nothing resolved it.

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_prop};

pub const KIND_RESOLVED_BUT_DESIGN: &str = "closure-resolved-but-design";
pub const KIND_ROOT_HANGS_OFF_DESIGN: &str = "root-direct-dependencies-all-design";
pub const KINDS: &[&str] = &[KIND_RESOLVED_BUT_DESIGN, KIND_ROOT_HANGS_OFF_DESIGN];

pub(crate) const TIER: &str = "waybill:sbom-tier";

/// Every CycloneDX component, nested ones included: cargo-auditable and the
/// closure's recovered modules can nest, and a top-level-only walk would miss
/// them.
pub(crate) fn all_components(cdx: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut out = Vec::new();
    let mut stack: Vec<&serde_json::Value> =
        cdx.get("components").and_then(|c| c.as_array()).into_iter().flatten().collect();
    while let Some(c) = stack.pop() {
        out.push(c);
        stack.extend(c.get("components").and_then(|c| c.as_array()).into_iter().flatten());
    }
    out
}

pub(crate) fn is_design(c: &serde_json::Value) -> bool {
    cdx_prop(c, TIER) == Some("design")
}

/// The closure's evidence that this component is in the build graph.
pub(crate) fn closure_resolved(c: &serde_json::Value) -> bool {
    cdx_prop(c, "waybill:scope-classification-source") == Some("exempt-evaluated-package")
        || cdx_prop(c, "waybill:boot-library-compiler").is_some()
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = all_components(&sboms.cdx);
    if !components.iter().any(|c| cdx_prop(c, "waybill:closure-contents").is_some()) {
        return CellOutcome::NotApplicable {
            reason: "no closure members in this document".to_string(),
        };
    }

    let mut violations = Vec::new();

    let mut resolved_design: Vec<String> = components
        .iter()
        .filter(|c| is_design(c) && closure_resolved(c))
        .filter_map(|c| cdx_component_ref(c).map(str::to_string))
        .collect();
    resolved_design.sort();
    if !resolved_design.is_empty() {
        violations.push(Violation {
            kind: KIND_RESOLVED_BUT_DESIGN,
            count: resolved_design.len(),
            example: resolved_design.first().cloned(),
        });
    }

    let root = sboms.cdx.pointer("/metadata/component/bom-ref").and_then(|r| r.as_str());
    let direct: Vec<&str> = sboms
        .cdx
        .get("dependencies")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .find(|d| root.is_some() && d.get("ref").and_then(|r| r.as_str()) == root)
        .and_then(|d| d.get("dependsOn"))
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .filter_map(|t| t.as_str())
        .collect();
    let design_of = |r: &str| {
        components
            .iter()
            .find(|c| cdx_component_ref(c) == Some(r))
            .is_some_and(|c| is_design(c))
    };
    if !direct.is_empty() && direct.iter().all(|r| design_of(r)) {
        let mut d: Vec<&str> = direct.clone();
        d.sort_unstable();
        violations.push(Violation {
            kind: KIND_ROOT_HANGS_OFF_DESIGN,
            count: d.len(),
            example: Some(d.join(", ")),
        });
    }

    if violations.is_empty() {
        CellOutcome::Pass
    } else {
        CellOutcome::Fail(violations)
    }
}
