//! The document-scope record of a closure query.
//!
//! Three annotations are emitted from one struct rather than three
//! independently-plumbed values, because they answer one question together
//! and would be misleading apart. A CVE count without the no-CVE count reads
//! as coverage; the grade without either reads as confidence.

use serde_json::{json, Value};

use super::emit;
use super::patches::{ComponentPatches, EvidenceGrade};
use super::ClassifiedClosure;

/// C183 — how CVE associations in this document were established.
pub const ANN_PATCH_EVIDENCE_GRADE: &str = "waybill:patch-evidence-grade";

/// C184 — what the closure query saw.
pub const ANN_NIX_CLOSURE: &str = "waybill:nix-closure";

/// What one closure query contributed, as emitted at document scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixClosureSummary {
    /// The attribute evaluated. Load-bearing: two attributes of one flake
    /// yield different closures, so a reader cannot interpret the counts
    /// without knowing which was taken.
    pub attribute: String,
    pub derivations: usize,
    /// Members per role, keyed by the wire form of the role.
    pub role_counts: std::collections::BTreeMap<&'static str, usize>,
    pub components_emitted: usize,
    pub patches: usize,
    /// Patches whose filename names no CVE. Measured at 167 of 187 and 153
    /// of 169 on the two projects — the figure that stops a reader taking
    /// the CVE count for coverage.
    pub patches_without_cve: usize,
    pub distinct_cves: usize,
    /// The per-component patch records themselves, carried alongside the
    /// counts rather than plumbed separately: they are the same
    /// contribution, and a second channel could drift out of step with the
    /// totals emitted from the first.
    pub patched: Vec<ComponentPatches>,
}

impl NixClosureSummary {
    pub fn build(closure: &ClassifiedClosure, patched: &[ComponentPatches]) -> Self {
        // Delegated rather than recounted here. Two implementations of one
        // total drift, and the drift would be invisible: both would still
        // produce a plausible number.
        let totals = super::patches::totals(patched);
        Self {
            attribute: closure.attribute.clone(),
            derivations: closure.raw.derivations.len(),
            role_counts: closure.role_counts(),
            components_emitted: emit::components(closure).len(),
            patches: totals.patches,
            patches_without_cve: totals.without_cve,
            distinct_cves: totals.distinct_cves,
            patched: patched.to_vec(),
        }
    }

    /// The C184 wire value, identical in all three formats.
    ///
    /// One canonical string rather than an object in SPDX and a string in
    /// CycloneDX: a CDX property value must be a string, and a row that
    /// differs in shape between formats forces the parity extractor to
    /// compensate — which has silently papered over a real defect before.
    pub fn nix_closure_wire(&self) -> String {
        self.nix_closure_value().to_string()
    }

    /// The C184 annotation value.
    pub fn nix_closure_value(&self) -> Value {
        json!({
            "attribute": self.attribute,
            "derivations": self.derivations,
            "roles": self.role_counts,
            "components-emitted": self.components_emitted,
            "patches": self.patches,
            "patches-without-cve": self.patches_without_cve,
            "distinct-cves": self.distinct_cves,
        })
    }

    /// The C183 annotation value.
    ///
    /// Emitted whenever a CVE was recovered, and only then: a grade attached
    /// to nothing would assert a standard of evidence for claims that do not
    /// exist.
    pub fn patch_evidence_grade_value(&self) -> Option<Value> {
        (self.distinct_cves > 0)
            .then(|| Value::String(EvidenceGrade::FilenameDerived.wire().to_string()))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::classify::classify;
    use super::super::derivation::RawClosure;
    use super::super::patches::attribute;
    use super::*;

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-lib.drv": { "env": {"pname":"lib","version":"1.0",
                               "patches":"/nix/store/h-CVE-2020-1.patch /nix/store/h-tidy.patch"},
                       "outputs":{"out":{"path":"h1-lib"}} },
        "d-app.drv": { "env": {"pname":"app","version":"2.0",
                               "buildInputs":"/nix/store/h1-lib"},
                       "outputs":{"out":{"path":"h5-app"}} }
      },
      "version": 3
    }"#;

    fn summary() -> NixClosureSummary {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let roles = classify(&raw);
        let closure = ClassifiedClosure {
            attribute: "packages.x86_64-linux.thing".into(),
            raw,
            roles,
        };
        let patched = attribute(&closure.raw);
        NixClosureSummary::build(&closure, &patched)
    }

    #[test]
    fn the_record_counts_patches_and_the_ones_naming_no_cve() {
        let s = summary();
        // CONTROL: the fixture has patches at all, so the no-CVE assertion
        // below is not passing over an empty set.
        assert_eq!(s.patches, 2);
        assert_eq!(s.patches_without_cve, 1);
        assert_eq!(s.distinct_cves, 1);
    }

    #[test]
    fn the_attribute_is_carried_because_counts_mean_nothing_without_it() {
        assert_eq!(summary().attribute, "packages.x86_64-linux.thing");
        assert_eq!(
            summary().nix_closure_value()["attribute"],
            "packages.x86_64-linux.thing"
        );
    }

    #[test]
    fn a_grade_is_absent_when_no_cve_was_recovered() {
        // A grade attached to nothing asserts a standard of evidence for
        // claims that do not exist.
        const NO_CVE: &str = r#"{
          "derivations": {
            "d-lib.drv": { "env": {"pname":"lib","version":"1.0",
                                   "patches":"/nix/store/h-tidy.patch"},
                           "outputs":{"out":{"path":"h1-lib"}} }
          },
          "version": 3
        }"#;
        let raw = RawClosure::parse(NO_CVE).unwrap();
        let roles = classify(&raw);
        let closure = ClassifiedClosure { attribute: "default".into(), raw, roles };
        let patched = attribute(&closure.raw);
        let s = NixClosureSummary::build(&closure, &patched);
        // CONTROL: a patch was seen, so absence of the grade is about the
        // CVE and not about the closure being empty.
        assert_eq!(s.patches, 1);
        assert!(s.patch_evidence_grade_value().is_none());
        assert_eq!(
            summary().patch_evidence_grade_value(),
            Some(serde_json::Value::String("filename-derived".into())),
            "and present when one was"
        );
    }

    #[test]
    fn role_counts_and_emitted_components_are_both_recorded() {
        let s = summary();
        assert_eq!(s.derivations, 2);
        assert_eq!(s.role_counts.get("artifact-input").copied(), Some(1));
        // `app` is the root and Unreferenced; `lib` is an artifact input.
        assert_eq!(s.components_emitted, 1);
    }
}
