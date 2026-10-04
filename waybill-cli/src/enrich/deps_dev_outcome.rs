//! Milestone 1067 (#1058) — why deps.dev did not enrich a component.
//!
//! C191 `waybill:deps-dev-outcome` is written per component by the
//! enrichment pass; C192 `waybill:deps-dev-outcomes` is counted from the
//! final components at emission, so the two cannot disagree (SC-005).
//! Both appear only when the deps.dev pass ran online (FR-007).

use std::collections::BTreeMap;

use waybill_common::resolution::ResolvedComponent;

use super::deps_dev_system::deps_dev_system_for;

/// C191 — per component.
pub(crate) const OUTCOME_ANNOTATION: &str = "waybill:deps-dev-outcome";
/// Counted in C192 only: a component deps.dev does not index never
/// carries C191 (FR-002a).
const UNSUPPORTED_ECOSYSTEM: &str = "not-queried:unsupported-ecosystem";

/// A closed set. Upstream strings never reach the document (Q2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// deps.dev answered and holds no such version.
    Absent,
    /// deps.dev returned licence strings and every one failed SPDX
    /// canonicalisation.
    DeclinedInvalidLicense,
    /// The request failed; nothing was learnt.
    TransportFailure,
    /// No request was sent: the version is empty or a placeholder.
    IncompleteCoordinate,
}

impl Outcome {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::DeclinedInvalidLicense => "declined-invalid-license",
            Self::TransportFailure => "transport-failure",
            Self::IncompleteCoordinate => "not-queried:incomplete-coordinate",
        }
    }
}

/// Set C191 to `outcome`, or clear it when deps.dev matched. Each pass
/// overwrites, so the final pass's outcome is what is emitted.
pub(crate) fn record(component: &mut ResolvedComponent, outcome: Option<Outcome>) {
    match outcome {
        Some(o) => {
            component.extra_annotations.insert(
                OUTCOME_ANNOTATION.to_string(),
                serde_json::Value::String(o.wire().to_string()),
            );
        }
        None => {
            component.extra_annotations.remove(OUTCOME_ANNOTATION);
        }
    }
}

/// C192's value over the components a document emits: canonical JSON,
/// keys sorted, non-zero counts only. `None` when every count is zero.
pub(crate) fn document_value(components: &[ResolvedComponent]) -> Option<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for c in components {
        match c.extra_annotations.get(OUTCOME_ANNOTATION).and_then(|v| v.as_str()) {
            Some(v) => *counts.entry(v).or_default() += 1,
            None if deps_dev_system_for(c.purl.ecosystem()).is_none() => {
                *counts.entry(UNSUPPORTED_ECOSYSTEM).or_default() += 1;
            }
            None => {}
        }
    }
    if counts.is_empty() {
        return None;
    }
    Some(serde_json::to_string(&counts).expect("a map of counts serialises"))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::enrich::depsdev_source::tests::make_component;

    fn with(purl: &str, outcome: Option<Outcome>) -> ResolvedComponent {
        let mut c = make_component(purl);
        record(&mut c, outcome);
        c
    }

    #[test]
    fn counts_are_sorted_and_include_unsupported_ecosystems() {
        let cs = vec![
            with("pkg:cargo/a@1.0.0", Some(Outcome::TransportFailure)),
            with("pkg:cargo/b@1.0.0", Some(Outcome::Absent)),
            with("pkg:npm/c@1.0.0", Some(Outcome::Absent)),
            with("pkg:pypi/d@1.0.0", Some(Outcome::DeclinedInvalidLicense)),
            with("pkg:cargo/matched@1.0.0", None),
            with("pkg:deb/debian/e@1.0.0", None),
        ];
        assert_eq!(
            document_value(&cs).unwrap(),
            r#"{"absent":2,"declined-invalid-license":1,"not-queried:unsupported-ecosystem":1,"transport-failure":1}"#,
        );
    }

    #[test]
    fn nothing_to_report_is_none() {
        assert_eq!(document_value(&[with("pkg:cargo/a@1.0.0", None)]), None);
        assert_eq!(document_value(&[]), None);
    }

    #[test]
    fn a_match_clears_an_earlier_outcome() {
        let mut c = with("pkg:cargo/a@1.0.0", Some(Outcome::Absent));
        record(&mut c, None);
        assert!(!c.extra_annotations.contains_key(OUTCOME_ANNOTATION));
    }
}
