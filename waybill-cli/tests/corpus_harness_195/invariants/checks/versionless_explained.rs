//! I8 — a component with no version carries an annotation explaining
//! why it has none.
//!
//! A consumer reading a component with no version cannot tell which of
//! two things happened: the component has no version to have (a file, a
//! synthetic anchor), or waybill looked for one and failed. Those have
//! opposite consequences — the first is complete information, the second
//! is a gap — and an empty `version` field renders them identical.
//!
//! **Boundary with I6**: I6's population is components carrying an
//! unknown-version *sentinel*, and it asks whether one package appears
//! under several spellings of unknown. I8's population is the narrower
//! absent-or-empty case, and it asks whether the absence is accounted
//! for at all. A document can satisfy either and fail the other: a
//! consistent `v0.0.0-unknown` everywhere passes I6 and is outside I8's
//! precondition, while a single unexplained empty version fails I8 and
//! is invisible to I6.
//!
//! **Why `unknown` and `NOASSERTION` are outside the precondition.**
//! They are a statement on the wire — a poor one, but the component is
//! saying the version is not known. An absent or empty field says
//! nothing at all. I8 is about silence, not about imprecision.

use super::super::{CellOutcome, Violation};
use super::{cdx_component_ref, cdx_components, cdx_prop};

pub const KIND_VERSIONLESS_AND_UNEXPLAINED: &str = "versionless-and-unexplained";
pub const KINDS: &[&str] = &[KIND_VERSIONLESS_AND_UNEXPLAINED];

/// True when the component's `version` is absent or empty after
/// trimming. A sentinel spelling such as `unknown` is deliberately
/// *not* in this population — see the module header.
fn is_versionless(component: &serde_json::Value) -> bool {
    match component.get("version").and_then(|v| v.as_str()) {
        None => true,
        Some(v) => v.trim().is_empty(),
    }
}

/// Annotations that account for a missing version.
///
/// - `component-tier = file` — a file is identified by its path and
///   hash; it has no version to carry. Same reasoning as I7's.
/// - `waybill:unresolved-reason` — C151, milestone 236's universal
///   "why could this not be resolved" annotation. This is the one
///   designed for the job.
/// - `waybill:haskell-version-unresolved-reason` — the Haskell reader's
///   more specific form of the same statement (`compiler-supplied`,
///   `absent-from-package-set`).
/// - `waybill:closure-role` — a Nix store path's role; the store path
///   is the identity and the version is not separable from it.
///
/// Deliberately **not** included: `waybill:component-kind =
/// lockfile-resolve`. A Pants resolve anchor is inherently versionless,
/// so it is tempting to read its kind as self-explanatory the way
/// `component-tier = file` is. The difference is that the file tier is a
/// frozen vocabulary whose meaning is "this is not a package", whereas
/// `component-kind` (milestone 1064) describes resolve ownership and
/// says nothing about versions. Treating it as an explanation would
/// silence the five cells this invariant exists to report, and a
/// consumer still could not tell an anchor from a failed resolution.
/// If the project decides a resolve anchor needs no explanation, the
/// right move is to add a marker that says so, not to widen this set.
fn is_explained(component: &serde_json::Value) -> bool {
    cdx_prop(component, "waybill:component-tier") == Some("file")
        || cdx_prop(component, "waybill:unresolved-reason").is_some()
        || cdx_prop(component, "waybill:haskell-version-unresolved-reason").is_some()
        || cdx_prop(component, "waybill:closure-role").is_some()
}

pub fn check(sboms: &super::EmittedSboms) -> CellOutcome {
    let components = cdx_components(&sboms.cdx);

    let versionless: Vec<&serde_json::Value> =
        components.iter().filter(|c| is_versionless(c)).collect();
    if versionless.is_empty() {
        return CellOutcome::NotApplicable {
            reason: "every component carries a version".to_string(),
        };
    }

    let mut offenders: Vec<String> = versionless
        .into_iter()
        .filter(|c| !is_explained(c))
        .map(|c| {
            cdx_component_ref(c)
                .map(|r| r.to_string())
                .or_else(|| {
                    c.get("name").and_then(|n| n.as_str()).map(|n| n.to_string())
                })
                .unwrap_or_else(|| "<component with neither ref nor name>".to_string())
        })
        .collect();
    if offenders.is_empty() {
        return CellOutcome::Pass;
    }
    offenders.sort();
    let count = offenders.len();
    CellOutcome::Fail(vec![Violation {
        kind: KIND_VERSIONLESS_AND_UNEXPLAINED,
        count,
        example: offenders.into_iter().next(),
    }])
}
