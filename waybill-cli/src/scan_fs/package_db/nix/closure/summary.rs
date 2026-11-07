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

/// C204 — what the closure reader saw but did not consult (#1348).
///
/// A sibling of C184 rather than a field inside it, because the two have
/// different emission gating: C184 is emitted whenever a closure was read,
/// and this is emitted only when something went unconsulted. Folding a
/// sometimes-absent fact into an always-present object would mean either a
/// zero-valued field on every document — whose presence then says nothing —
/// or a key that comes and goes inside one value, which the parity extractors
/// would have to special-case.
pub const ANN_NIX_CLOSURE_READ: &str = "waybill:nix-closure-read";

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
    /// What the reader saw but did not consult (#1348).
    pub read_outcome: super::derivation::ReadOutcome,
    /// Dependency edges the closure contributed (milestone 1280, FR-008).
    ///
    /// Recorded so a closure that contributed **no** edges is distinguishable
    /// from one that was never read. Before milestone 1280 this was always
    /// zero and the tier emitted no edges at all, which is why every closure
    /// component was an orphan by construction.
    pub edges: usize,
    /// Milestone 1281 (#1346) — how many members were offered a declared
    /// license and how many received one.
    pub license_coverage: super::licenses::LicenseCoverage,
    /// Why the license lookup produced nothing, when it failed (FR-006).
    pub licenses_degraded: Option<String>,
    /// Milestone 1282 FR-009 — how many components each of the four member
    /// roles contributed.
    ///
    /// **Additive beside [`Self::role_counts`], not a replacement for it,
    /// and the two are not addends of one another.** `role_counts` is the
    /// milestone-1277 C182 census and counts **derivations** — 1,749 on the
    /// pinned closure, covering this whole population already, most of it
    /// under `unreferenced`. This counts **components**. Summing the two
    /// reconciles nothing; the reconciliation FR-009 asks for is
    /// `components-emitted` = components carrying no member role + these
    /// four.
    ///
    /// Every variant is present even at zero. An absent key would make a
    /// closure with no scaffolding indistinguishable from a classifier that
    /// stopped producing that role.
    pub member_role_counts: std::collections::BTreeMap<&'static str, usize>,
    /// Components emitted that predate milestone 1282 — those carrying no
    /// member role. The other addend of the FR-009 reconciliation.
    pub components_without_member_role: usize,
    /// Milestone 1282 FR-015 — components this scan emitted that **cannot**
    /// be dependency-edge endpoints, by design.
    ///
    /// Without it a reader cannot tell a closure whose graph is *incomplete*
    /// from one that merely gained components FR-013 excludes from the graph
    /// on purpose. Both show up as a rise in the orphan count, and on the
    /// pinned closure the second accounts for 513 of them — a population, not
    /// an exception. Stating the figure is what makes the orphan accounting
    /// readable rather than alarming.
    pub edge_ineligible_components: usize,
}

impl NixClosureSummary {
    pub fn build(closure: &ClassifiedClosure, patched: &[ComponentPatches]) -> Self {
        // Delegated rather than recounted here. Two implementations of one
        // total drift, and the drift would be invisible: both would still
        // produce a plausible number.
        let totals = super::patches::totals(patched);
        // Built once and measured twice. Calling `emit::components` again for
        // the member-role counts would be a second traversal that could
        // disagree with the first, and the disagreement would be invisible:
        // both numbers would look plausible.
        let components = emit::components(closure);
        let mut member_role_counts: std::collections::BTreeMap<&'static str, usize> =
            super::member::MemberRole::ALL
                .iter()
                .map(|r| (r.wire(), 0usize))
                .collect();
        let mut without_member_role = 0usize;
        let mut edge_ineligible = 0usize;
        for c in &components {
            match c
                .extra_annotations
                .get(super::member::ANN_MEMBER_ROLE)
                .and_then(|v| v.as_str())
            {
                Some(role) => {
                    if let Some(slot) = member_role_counts.get_mut(role) {
                        *slot += 1;
                    }
                    // Read back from the emitted value rather than
                    // re-deriving it from the derivation: the figure must
                    // describe the document a consumer holds, and a second
                    // derivation could disagree with it plausibly.
                    if !super::member::MemberRole::ALL
                        .iter()
                        .any(|r| r.wire() == role && r.is_edge_endpoint())
                    {
                        edge_ineligible += 1;
                    }
                }
                None => without_member_role += 1,
            }
        }
        Self {
            attribute: closure.attribute.clone(),
            derivations: closure.raw.derivations.len(),
            role_counts: closure.role_counts(),
            components_emitted: components.len(),
            patches: totals.patches,
            patches_without_cve: totals.without_cve,
            distinct_cves: totals.distinct_cves,
            read_outcome: closure.read_outcome.clone(),
            // The projected, `.drv`-keyed count. Deliberately not the
            // post-join relationship count: that depends on the final
            // component set, which does not exist when this summary is built,
            // and a figure that changes depending on when you ask is worse
            // than one that is explicit about what it counts.
            edges: closure.graph.edges.len(),
            license_coverage: closure.license_coverage.clone(),
            licenses_degraded: closure.degraded_licenses.as_ref().map(|r| r.wire().to_string()),
            member_role_counts,
            components_without_member_role: without_member_role,
            edge_ineligible_components: edge_ineligible,
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
            // Milestone 1282 FR-018 — this is the figure that **changes**.
            // FR-013 stops the walk at `source` and `package` members, which
            // replaces wide transitive fan-outs with direct edges, so the
            // count moves rather than merely growing. It is carried here
            // already, which is what FR-018 requires: a graph that silently
            // changes size is indistinguishable from one that broke, and a
            // reader comparing two scans sees the new number rather than
            // inferring it.
            "edges": self.edges,
            // Milestone 1282 FR-009 — the four member-role counts, and the
            // count of components carrying none. Those two sum to
            // `components-emitted`; the C182 `roles` census above does NOT
            // participate, because it counts derivations.
            "member-roles": self.member_role_counts,
            "components-without-member-role": self.components_without_member_role,
            // Milestone 1282 FR-015 — the population the edge count does NOT
            // describe. A rise in orphans that this number accounts for is a
            // design consequence; a rise it does not account for is a defect,
            // and without the figure the two are the same observation.
            "edge-ineligible-components": self.edge_ineligible_components,
            // Milestone 1281 (#1346) — FR-008. Carried even when zero, and
            // deliberately: an absent key would make a closure whose members
            // declare no licenses indistinguishable from a lookup that never
            // ran, which is the one distinction this record exists to make.
            // Coverage cannot reach 100% either -- a `.drv` hash encodes the
            // whole transitive input closure, so the same package built in a
            // different context is a different store path -- so the shortfall
            // is permanent and has to be legible rather than inferred.
            "licenses": {
                "wanted": self.license_coverage.wanted,
                "received": self.license_coverage.received,
                "withheld-ambiguous": self.license_coverage.withheld_ambiguous,
                "degraded": self.licenses_degraded,
            },
        })
    }

    /// The C204 annotation value, and `None` when there is nothing to
    /// disclose.
    ///
    /// **Absence means "nothing went unconsulted", never "not checked".** That
    /// is the whole distinction this milestone exists to make, so the gating
    /// is on a positive signal — something was seen and skipped — and not on
    /// the closure having been read at all. A closure that was never read is
    /// C190's case.
    ///
    /// The format version is carried here rather than in C184 because it is
    /// evidence about *this* record: it says which output shape the reader was
    /// looking at when it failed to consult something.
    pub fn nix_closure_read_value(&self) -> Option<Value> {
        if !self.read_outcome.has_gap() {
            return None;
        }
        let mut obj = serde_json::Map::new();
        if let Some(v) = self.read_outcome.format_version {
            obj.insert("format-version".into(), json!(v));
        }
        obj.insert(
            "fields-unconsulted".into(),
            json!(self
                .read_outcome
                .fields_unconsulted
                .iter()
                .cloned()
                .collect::<Vec<String>>()),
        );
        obj.insert(
            "declarations-unconsulted".into(),
            json!(self.read_outcome.declarations_unconsulted),
        );
        obj.insert(
            "derivations-affected".into(),
            json!(self.read_outcome.derivations_affected),
        );
        if !self.read_outcome.carrier_disagreements.is_empty() {
            obj.insert(
                "carrier-disagreements".into(),
                json!(self
                    .read_outcome
                    .carrier_disagreements
                    .iter()
                    .cloned()
                    .collect::<Vec<String>>()),
            );
        }
        Some(Value::Object(obj))
    }

    /// The C204 wire value — one canonical string, identical in all three
    /// formats, for the reason C184's own doc comment gives.
    pub fn nix_closure_read_wire(&self) -> Option<String> {
        self.nix_closure_read_value().map(|v| v.to_string())
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
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
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
        let closure = ClassifiedClosure {
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
        };
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
