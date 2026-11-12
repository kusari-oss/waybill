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

/// C216 — how the closure scope marks were decided (milestone 1423).
pub const ANN_CLOSURE_SCOPE_METHOD: &str = "waybill:closure-scope-method";

/// C217 — the import-from-derivation policy Nix evaluations ran under, and
/// nix's sandbox setting (milestone 1283).
pub const ANN_NIX_EVALUATION_POLICY: &str = "waybill:nix-evaluation-policy";

/// Milestone 1283 (#1283) — C217, how this scan's Nix evaluations were
/// configured.
///
/// Emitted whenever a Nix tier (`--nix-closure` or `--nix-eval`) was admitted,
/// on success and on degradation alike, which is why it is its own value
/// rather than a field of C216 (absent on degradation) or C190 (absent on
/// success) — research R4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NixEvaluationPolicy {
    /// What was *requested*: `Allow` only with `--nix-allow-ifd`.
    pub(crate) import_from_derivation: crate::scan_fs::package_db::nix::eval::preflight::IfdPolicy,
    /// True only when a pre-flight confirmed nix reports the requested value.
    pub(crate) verified: bool,
    /// nix's reported `sandbox` value, verbatim; `None` when nix reported none
    /// or no pre-flight completed.
    pub(crate) sandbox: Option<String>,
}

impl NixEvaluationPolicy {
    /// The record for a pre-flight that confirmed the policy. Taking the
    /// token is what keeps `verified: true` from being written by hand.
    pub(crate) fn verified(
        ifd: &crate::scan_fs::package_db::nix::eval::result::IfdPolicyVerified,
        environment: &crate::scan_fs::package_db::nix::eval::preflight::NixEnvironment,
    ) -> Self {
        Self {
            import_from_derivation: ifd.policy(),
            verified: true,
            sandbox: environment.sandbox.clone(),
        }
    }

    /// The record for a tier that was admitted and whose pre-flight did not
    /// confirm the requested policy, or never ran.
    pub(crate) fn unverified(
        requested: crate::scan_fs::package_db::nix::eval::preflight::IfdPolicy,
    ) -> Self {
        Self {
            import_from_derivation: requested,
            verified: false,
            sandbox: None,
        }
    }

    /// Two tiers' records, as one. Verified only if both were, so a document
    /// never claims more than the weaker tier established.
    pub(crate) fn merge(self, other: Self) -> Self {
        Self {
            import_from_derivation: self.import_from_derivation,
            verified: self.verified && other.verified,
            sandbox: self.sandbox.or(other.sandbox),
        }
    }

    /// The C217 wire value: a JSON object with sorted keys, identical in all
    /// three formats. Inserted in sorted order, as C216's is, so the bytes do
    /// not depend on whether `serde_json` preserves insertion order.
    pub(crate) fn wire(&self) -> String {
        let mut o = serde_json::Map::new();
        o.insert(
            "import-from-derivation".into(),
            self.import_from_derivation.wire().into(),
        );
        o.insert(
            "sandbox".into(),
            self.sandbox.as_deref().map_or(Value::Null, Value::from),
        );
        o.insert("schema".into(), 1.into());
        o.insert("verified".into(), self.verified.into());
        Value::Object(o).to_string()
    }
}

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
    /// Dependency edges the closure contributed (milestone 1280, FR-008): the
    /// distinct pairs in the document, not the `.drv`-keyed projection (#1445).
    ///
    /// Recorded so a closure that contributed **no** edges is distinguishable
    /// from one that was never read. Before milestone 1280 this was always
    /// zero and the tier emitted no edges at all, which is why every closure
    /// component was an orphan by construction.
    pub edges: usize,
    /// Milestone 1281 (#1346) — how many members were offered a declared
    /// license and how many received one.
    pub license_coverage: super::licenses::LicenseCoverage,
    /// Milestone 1377 — C184 `identifiers`.
    pub identifier_coverage: super::identifiers::IdentifierCoverage,
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
    /// Milestone 1423 (#1423) — C216, how the document's scope marks were
    /// decided. Set by the scan after the scope pass runs; `None` until then,
    /// and never emitted as a placeholder.
    pub(crate) scope_method: Option<super::runtime::ScopeMethodRecord>,
}

impl NixClosureSummary {
    pub fn build(closure: &ClassifiedClosure, patched: &[ComponentPatches]) -> Self {
        // Delegated rather than recounted here. Two implementations of one
        // total drift, and the drift would be invisible: both would still
        // produce a plausible number.
        let totals = super::patches::totals(patched);
        let mut summary = Self {
            attribute: closure.attribute.clone(),
            derivations: closure.raw.derivations.len(),
            role_counts: closure.role_counts(),
            components_emitted: 0,
            patches: totals.patches,
            patches_without_cve: totals.without_cve,
            distinct_cves: totals.distinct_cves,
            read_outcome: closure.read_outcome.clone(),
            edges: 0,
            license_coverage: closure.license_coverage.clone(),
            identifier_coverage: closure.identifiers.coverage.clone(),
            licenses_degraded: closure.degraded_licenses.as_ref().map(|r| r.wire().to_string()),
            member_role_counts: Default::default(),
            components_without_member_role: 0,
            edge_ineligible_components: 0,
            scope_method: None,
        };
        // Provisional: what the closure alone would contribute, before it
        // meets any other reader's components. The scan replaces these with
        // [`Self::count_emitted`] over the final document; a caller holding
        // only the closure gets the same measure over what it has.
        let components = emit::components(closure);
        let (edges, _) = super::graph::relationships(&closure.graph, &closure.raw, &components);
        summary.count_emitted(&components, &edges);
        summary
    }

    /// Take `components-emitted`, its member-role breakdown and `edges` from
    /// the components and relationships a document will be written from
    /// (#1445).
    ///
    /// Called by the scan after every pass that can add, merge, rename or
    /// remove a closure member, so each figure is checkable against the
    /// document it is emitted in. Taken earlier they were not: `edges` was the
    /// `.drv`-keyed projection, counted before build variants of one package
    /// collapse onto one component (moat: 7,978 projected, 6,167 contributed),
    /// and `--exclude-scope` removes members after the record used to be
    /// built.
    ///
    /// - A closure member is a component carrying the C182 role, whether the
    ///   closure appended it or merged it into another reader's component.
    /// - An edge is a distinct `(from, to)` pair of a relationship the closure
    ///   graph contributed whose endpoints are both components. A pair another
    ///   reader also contributed is counted: it is in the document. Edges from
    ///   other sources -- manifests, the identity pass's declared edges, the
    ///   contents pass, the subject anchoring the emitters add -- are not, so
    ///   a document's dependency total is at least this and usually more.
    pub(crate) fn count_emitted(
        &mut self,
        components: &[waybill_common::resolution::ResolvedComponent],
        relationships: &[waybill_common::resolution::Relationship],
    ) {
        let mut member_role_counts: std::collections::BTreeMap<&'static str, usize> =
            super::member::MemberRole::ALL
                .iter()
                .map(|r| (r.wire(), 0usize))
                .collect();
        let mut members = 0usize;
        let mut without_member_role = 0usize;
        let mut edge_ineligible = 0usize;
        for c in components
            .iter()
            .filter(|c| c.extra_annotations.contains_key(emit::ANN_CLOSURE_ROLE))
        {
            members += 1;
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
        let present: std::collections::HashSet<&str> =
            components.iter().map(|c| c.purl.as_str()).collect();
        let edges: std::collections::BTreeSet<(&str, &str)> = relationships
            .iter()
            .filter(|r| super::graph::is_closure_edge(r))
            .filter(|r| present.contains(r.from.as_str()) && present.contains(r.to.as_str()))
            .map(|r| (r.from.as_str(), r.to.as_str()))
            .collect();
        self.components_emitted = members;
        self.member_role_counts = member_role_counts;
        self.components_without_member_role = without_member_role;
        self.edge_ineligible_components = edge_ineligible;
        self.edges = edges.len();
    }

    /// The C216 wire value — a JSON object with sorted keys and `schema: 1`,
    /// identical in all three formats — or `None` when the scope pass did not
    /// record a method.
    pub fn closure_scope_method_wire(&self) -> Option<String> {
        self.scope_method.as_ref().map(super::runtime::ScopeMethodRecord::wire)
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
            // Milestone 1377 (#1394) FR-009 — how far the meta evaluation
            // reached for declared CPEs. `wanted − received` members were
            // not reached, so their missing nixpkgs CPE is a coverage gap
            // rather than an absence. Carried when zero, as `licenses` is.
            // Degradation is `licenses.degraded`: one evaluation feeds both.
            "identifiers": {
                "wanted": self.identifier_coverage.wanted,
                "received": self.identifier_coverage.received,
                "by-drv": self.identifier_coverage.by_drv,
                "by-name-version": self.identifier_coverage.by_name_version,
                "declared": self.identifier_coverage.declared,
                "rejected": self.identifier_coverage.rejected,
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
            runtime: None,
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
            identifiers: Default::default(),
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
            runtime: None,
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
            identifiers: Default::default(),
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

    // ── T009 (#1283) — C217's exact bytes ───────────────────────────────

    use crate::scan_fs::package_db::nix::eval::preflight::{IfdPolicy, NixEnvironment};
    use crate::scan_fs::package_db::nix::eval::result::IfdPolicyVerified;

    fn env(sandbox: Option<&str>) -> NixEnvironment {
        NixEnvironment { sandbox: sandbox.map(str::to_string) }
    }

    #[test]
    fn t009_c217_allowed_and_verified() {
        let v = IfdPolicyVerified::assumed_for_test(IfdPolicy::Allow);
        assert_eq!(
            NixEvaluationPolicy::verified(&v, &env(Some("false"))).wire(),
            r#"{"import-from-derivation":"allowed","sandbox":"false","schema":1,"verified":true}"#
        );
    }

    #[test]
    fn t009_c217_refused_and_verified() {
        let v = IfdPolicyVerified::assumed_for_test(IfdPolicy::Refuse);
        assert_eq!(
            NixEvaluationPolicy::verified(&v, &env(Some("true"))).wire(),
            r#"{"import-from-derivation":"refused","sandbox":"true","schema":1,"verified":true}"#
        );
    }

    #[test]
    fn t009_c217_unverified_carries_the_request_and_no_sandbox() {
        assert_eq!(
            NixEvaluationPolicy::unverified(IfdPolicy::Allow).wire(),
            r#"{"import-from-derivation":"allowed","sandbox":null,"schema":1,"verified":false}"#
        );
    }

    #[test]
    fn t009_c217_sandbox_unreported_is_null_not_absent() {
        let v = IfdPolicyVerified::assumed_for_test(IfdPolicy::Refuse);
        let w = NixEvaluationPolicy::verified(&v, &env(None)).wire();
        assert_eq!(
            w,
            r#"{"import-from-derivation":"refused","sandbox":null,"schema":1,"verified":true}"#
        );
        let parsed: serde_json::Value = serde_json::from_str(&w).unwrap();
        assert_eq!(parsed.as_object().unwrap().len(), 4, "every key, always: {w}");
    }

    #[test]
    fn t009_c217_merge_is_verified_only_if_both_were() {
        let v = IfdPolicyVerified::assumed_for_test(IfdPolicy::Allow);
        let ok = NixEvaluationPolicy::verified(&v, &env(Some("false")));
        let merged = ok.clone().merge(NixEvaluationPolicy::unverified(IfdPolicy::Allow));
        assert!(!merged.verified);
        assert_eq!(merged.sandbox.as_deref(), Some("false"));
        assert!(ok.clone().merge(ok).verified);
    }
}
