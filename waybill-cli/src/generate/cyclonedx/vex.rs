//! CycloneDX `vulnerabilities[]` — milestone 1068 (#1039).
//!
//! Carries the VEX statements waybill makes about a scan, the same ones the
//! OpenVEX sidecar carries beside SPDX (`openvex::vex_statements`), in
//! CycloneDX's own vocabulary. Today they come from two sources: graded
//! nixpkgs backports (m1035) and nixpkgs' own security declarations (m1050).
//! waybill does not match components against advisory databases, so an empty
//! array is not a claim that nothing is vulnerable
//! (`docs/architecture/enrichment.md`).

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::generate::openvex::statements::{
    OpenVexJustification, OpenVexStatement, OpenVexStatus,
};

/// The CycloneDX `analysis.state` for an OpenVEX status. Exhaustive on
/// purpose: a new status must be mapped before it can be emitted, and no
/// mapping may make a claim stronger (FR-003).
pub(crate) fn cdx_state(status: OpenVexStatus) -> &'static str {
    match status {
        OpenVexStatus::NotAffected => "not_affected",
        // CycloneDX: "may be directly or indirectly exploitable".
        OpenVexStatus::Affected => "exploitable",
        OpenVexStatus::UnderInvestigation => "in_triage",
        OpenVexStatus::Fixed => "resolved",
    }
}

/// The CycloneDX `analysis.justification`, or `None` where CycloneDX has no
/// faithful equivalent. That case keeps the original in `analysis.detail`
/// rather than borrowing the nearest value, which would be a different claim.
pub(crate) fn cdx_justification(j: OpenVexJustification) -> Option<&'static str> {
    match j {
        OpenVexJustification::VulnerableCodeNotPresent
        | OpenVexJustification::ComponentNotPresent => Some("code_not_present"),
        OpenVexJustification::VulnerableCodeNotInExecutePath => Some("code_not_reachable"),
        OpenVexJustification::InlineMitigationsAlreadyExist => {
            Some("protected_by_mitigating_control")
        }
        OpenVexJustification::VulnerableCodeCannotBeControlledByAdversary => None,
    }
}

/// How a statement's PURLs become `bom-ref`s in one CycloneDX document.
pub struct BomRefIndex {
    by_purl: HashMap<String, Vec<String>>,
    root: String,
    root_aliases: HashSet<String>,
}

impl BomRefIndex {
    /// `root` is `metadata.component`'s `bom-ref`.
    pub fn new(root: String) -> Self {
        Self { by_purl: HashMap::new(), root, root_aliases: HashSet::new() }
    }

    /// Index every component in a CycloneDX `components[]` tree. A component
    /// is `<purl>`, or `<purl>#<parent>` when nested, so one PURL can have
    /// several `bom-ref`s.
    pub fn add_components(&mut self, components: &[Value]) {
        for c in components {
            if let (Some(r), Some(p)) = (c["bom-ref"].as_str(), c["purl"].as_str()) {
                self.by_purl.entry(p.to_string()).or_default().push(r.to_string());
            }
            if let Some(nested) = c["components"].as_array() {
                self.add_components(nested);
            }
        }
    }

    /// A PURL that names the document root: the main module, which a root
    /// override drops from `components[]` but which statements still name as
    /// the build.
    pub fn alias_root(&mut self, purl: &str) {
        self.root_aliases.insert(purl.to_string());
    }

    fn refs(&self, purl: &str) -> Vec<&str> {
        if purl == self.root || self.root_aliases.contains(purl) {
            return vec![self.root.as_str()];
        }
        self.by_purl
            .get(purl)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// Build `vulnerabilities[]`, and count the claims this document cannot
/// carry because their component is not in it (C193).
///
/// A version statement's products go in `affects[]`. A build statement's
/// product is the root, in `affects[]`, and its subcomponents go in
/// `waybill:vex-subcomponent` properties: `affects[]` items allow only `ref`
/// and `versions`, and putting the component there would assert the version
/// itself is not affected, collapsing the two claims a backport keeps apart.
pub fn build_vulnerabilities(statements: &[OpenVexStatement], refs: &BomRefIndex) -> (Value, usize) {
    let mut entries = Vec::new();
    let mut omitted = 0usize;
    for st in statements {
        let mut affects: Vec<&str> = Vec::new();
        let mut subcomponents: Vec<&str> = Vec::new();
        for p in &st.products {
            if p.subcomponents.is_empty() {
                let r = refs.refs(&p.id);
                if r.is_empty() {
                    omitted += 1;
                }
                affects.extend(r);
                continue;
            }
            let mut carried = 0usize;
            for sc in &p.subcomponents {
                let r = refs.refs(&sc.id);
                if r.is_empty() {
                    omitted += 1;
                }
                carried += r.len();
                subcomponents.extend(r);
            }
            if carried > 0 {
                affects.extend(refs.refs(&p.id));
            }
        }
        if affects.is_empty() {
            continue;
        }
        affects.dedup();

        let mut analysis = serde_json::Map::new();
        analysis.insert("state".into(), json!(cdx_state(st.status)));
        let mut detail = st.impact_statement.clone().unwrap_or_default();
        if let Some(j) = st.justification {
            match cdx_justification(j) {
                Some(mapped) => {
                    analysis.insert("justification".into(), json!(mapped));
                }
                None => {
                    let wire = serde_json::to_value(j)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_default();
                    if !detail.is_empty() {
                        detail.push(' ');
                    }
                    detail.push_str(&format!("(OpenVEX justification: {wire})"));
                }
            }
        }
        if !detail.is_empty() {
            analysis.insert("detail".into(), json!(detail));
        }

        let mut entry = serde_json::Map::new();
        entry.insert("id".into(), json!(st.vulnerability.name));
        entry.insert("analysis".into(), Value::Object(analysis));
        entry.insert(
            "affects".into(),
            Value::Array(affects.iter().map(|r| json!({ "ref": r })).collect()),
        );
        if let Some(a) = &st.action_statement {
            entry.insert("recommendation".into(), json!(a));
        }
        if !subcomponents.is_empty() {
            entry.insert(
                "properties".into(),
                Value::Array(
                    subcomponents
                        .iter()
                        .map(|r| json!({ "name": "waybill:vex-subcomponent", "value": r }))
                        .collect(),
                ),
            );
        }
        entries.push(Value::Object(entry));
    }
    (Value::Array(entries), omitted)
}

// The shared CycloneDX 1.6 validator the integration tests use, so the schema
// check here runs against the same schema and resolver (m1068 T008).
#[cfg(test)]
#[path = "../../../tests/common/cdx_schema.rs"]
#[allow(dead_code)]
mod cdx_schema;

// Milestone 1068 (#1039) US1 — the VEX statements waybill already makes, in
// CycloneDX's own `vulnerabilities[]`.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m1068_tests {
    use std::collections::BTreeSet;

    use super::*;
    use waybill_common::resolution::ResolvedComponent;
    use crate::generate::openvex::tests::{
        backport_scan, contested_scan, declared_summary, empty_integrity, mk_artifacts, mk_cfg,
        mk_component,
    };
    use crate::generate::{ScanArtifacts, SbomSerializer};

    fn serialize(s: &dyn SbomSerializer, arts: &ScanArtifacts<'_>) -> serde_json::Value {
        serde_json::from_slice(&s.serialize(arts, &mk_cfg()).unwrap()[0].bytes).unwrap()
    }
    fn cdx(arts: &ScanArtifacts<'_>) -> serde_json::Value {
        serialize(&crate::generate::cyclonedx::CycloneDxJsonSerializer, arts)
    }
    fn sidecar(arts: &ScanArtifacts<'_>) -> Vec<serde_json::Value> {
        let a = crate::generate::openvex::serialize_openvex(arts, &mk_cfg()).unwrap().unwrap();
        let doc: serde_json::Value = serde_json::from_slice(&a.bytes).unwrap();
        doc["statements"].as_array().unwrap().clone()
    }
    fn vulns(doc: &serde_json::Value) -> Vec<serde_json::Value> {
        doc["vulnerabilities"].as_array().unwrap().clone()
    }
    fn root_ref(doc: &serde_json::Value) -> String {
        doc["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string()
    }
    /// `bom-ref` → PURL over every component, nested included, plus the root.
    fn purl_of(doc: &serde_json::Value, root_purl: &str) -> std::collections::HashMap<String, String> {
        fn walk(v: &serde_json::Value, out: &mut std::collections::HashMap<String, String>) {
            for c in v.as_array().into_iter().flatten() {
                if let (Some(r), Some(p)) = (c["bom-ref"].as_str(), c["purl"].as_str()) {
                    out.insert(r.to_string(), p.to_string());
                }
                walk(&c["components"], out);
            }
        }
        let mut out = std::collections::HashMap::new();
        walk(&doc["components"], &mut out);
        out.insert(root_ref(doc), root_purl.to_string());
        out
    }
    fn subcomponents(v: &serde_json::Value) -> Vec<String> {
        v["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["name"] == "waybill:vex-subcomponent")
            .map(|p| p["value"].as_str().unwrap().to_string())
            .collect()
    }
    fn ref_for(doc: &serde_json::Value, purl: &str) -> String {
        purl_of(doc, "")
            .into_iter()
            .find(|(_, p)| p == purl)
            .map(|(r, _)| r)
            .unwrap_or_else(|| panic!("no bom-ref for {purl}"))
    }
    fn property(doc: &serde_json::Value, name: &str) -> Option<String> {
        doc["metadata"]["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == name)
            .map(|p| p["value"].as_str().unwrap().to_string())
    }
    fn with_root(mut comps: Vec<ResolvedComponent>) -> Vec<ResolvedComponent> {
        let mut root = mk_component("pkg:generic/the-build@1.0");
        root.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        comps.push(root);
        comps
    }

    /// T004 — the contract's mapping, and no claim made stronger.
    #[test]
    fn statuses_and_justifications_map_without_strengthening() {
        assert_eq!(cdx_state(OpenVexStatus::NotAffected), "not_affected");
        assert_eq!(cdx_state(OpenVexStatus::Affected), "exploitable");
        assert_eq!(cdx_state(OpenVexStatus::UnderInvestigation), "in_triage");
        assert_eq!(cdx_state(OpenVexStatus::Fixed), "resolved");
        use OpenVexJustification as J;
        assert_eq!(cdx_justification(J::VulnerableCodeNotPresent), Some("code_not_present"));
        assert_eq!(cdx_justification(J::ComponentNotPresent), Some("code_not_present"));
        assert_eq!(cdx_justification(J::VulnerableCodeNotInExecutePath), Some("code_not_reachable"));
        assert_eq!(
            cdx_justification(J::InlineMitigationsAlreadyExist),
            Some("protected_by_mitigating_control"),
        );
        assert_eq!(cdx_justification(J::VulnerableCodeCannotBeControlledByAdversary), None);
    }

    /// T005 — a backport is two entries with distinct subjects, each with
    /// its grade; a declaration names the build. Shares the mutator's keys.
    #[test]
    fn a_backport_is_two_entries_with_distinct_subjects() {
        let comps = backport_scan();
        let integ = empty_integrity();
        let arts = mk_artifacts(&comps, &integ);
        let doc = cdx(&arts);
        let notes: BTreeSet<String> = sidecar(&arts)
            .iter()
            .map(|s| s["impact_statement"].as_str().unwrap().to_string())
            .collect();
        let v = vulns(&doc);
        assert_eq!(v.len(), 2, "{v:#?}");
        let unzip = ref_for(&doc, "pkg:generic/unzip@6.0");

        let affected = v.iter().find(|e| e["analysis"]["state"] == "exploitable").unwrap();
        assert_eq!(affected["id"], "CVE-2019-13232");
        assert_eq!(affected["affects"], json!([{ "ref": unzip }]));
        assert!(subcomponents(affected).is_empty());

        let build = v.iter().find(|e| e["analysis"]["state"] == "not_affected").unwrap();
        assert_eq!(build["analysis"]["justification"], "code_not_present");
        assert_eq!(build["affects"], json!([{ "ref": root_ref(&doc) }]));
        assert_eq!(subcomponents(build), vec![unzip]);

        for e in &v {
            assert!(notes.contains(e["analysis"]["detail"].as_str().unwrap()), "{e:#}");
            assert!(e["analysis"]["state"] != "resolved_with_pedigree" && e["analysis"]["state"] != "resolved");
            // The keys the `sbom enrich` mutator also writes.
            assert!(e["id"].is_string() && e["analysis"]["state"].is_string());
            assert!(e["affects"].as_array().unwrap().iter().all(|a| a["ref"].is_string()));
        }
    }

    #[test]
    fn a_declaration_names_the_build_with_the_component_as_subcomponent() {
        let comps = with_root(vec![mk_component("pkg:generic/unzip@6.0")]);
        let s = declared_summary(&[("pkg:generic/unzip@6.0", "CVE-2024-0001", "insecure")]);
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&s);
        let doc = cdx(&arts);
        let v = vulns(&doc);
        assert_eq!(v.len(), 1, "{v:#?}");
        assert_eq!(v[0]["analysis"]["state"], "exploitable");
        assert_eq!(v[0]["affects"], json!([{ "ref": root_ref(&doc) }]));
        assert_eq!(subcomponents(&v[0]), vec![ref_for(&doc, "pkg:generic/unzip@6.0")]);
    }

    /// The claims a CycloneDX document carries, as (id, state, product PURL,
    /// subcomponent PURL), next to the same from the sidecar.
    fn claim_sets(arts: &ScanArtifacts<'_>) -> (BTreeSet<[String; 4]>, BTreeSet<[String; 4]>) {
        let doc = cdx(arts);
        let root_purl = "pkg:generic/the-build@1.0";
        let purls = purl_of(&doc, root_purl);
        let mut c = BTreeSet::new();
        for e in vulns(&doc) {
            let (id, state) = (e["id"].as_str().unwrap(), e["analysis"]["state"].as_str().unwrap());
            for a in e["affects"].as_array().unwrap() {
                let product = purls[a["ref"].as_str().unwrap()].clone();
                let subs = subcomponents(&e);
                if subs.is_empty() {
                    c.insert([id.into(), state.into(), product.clone(), String::new()]);
                }
                for s in subs {
                    c.insert([id.into(), state.into(), product.clone(), purls[&s].clone()]);
                }
            }
        }
        let mut o = BTreeSet::new();
        for st in sidecar(arts) {
            let id = st["vulnerability"]["name"].as_str().unwrap();
            let state = cdx_state(match st["status"].as_str().unwrap() {
                "not_affected" => OpenVexStatus::NotAffected,
                "affected" => OpenVexStatus::Affected,
                "fixed" => OpenVexStatus::Fixed,
                "under_investigation" => OpenVexStatus::UnderInvestigation,
                other => panic!("unknown OpenVEX status {other}"),
            });
            for p in st["products"].as_array().unwrap() {
                let product = p["@id"].as_str().unwrap().to_string();
                let subs = p["subcomponents"].as_array().cloned().unwrap_or_default();
                if subs.is_empty() {
                    o.insert([id.into(), state.into(), product.clone(), String::new()]);
                }
                for s in subs {
                    o.insert([id.into(), state.into(), product.clone(), s["@id"].as_str().unwrap().into()]);
                }
            }
        }
        (c, o)
    }

    /// T006 (FR-006, SC-001) — CycloneDX carries exactly the sidecar's claims.
    #[test]
    fn cyclonedx_and_the_sidecar_carry_the_same_claims() {
        let integ = empty_integrity();

        let comps = backport_scan();
        let (c, o) = claim_sets(&mk_artifacts(&comps, &integ));
        assert!(!o.is_empty());
        assert_eq!(c, o, "backport");

        let comps = with_root(vec![mk_component("pkg:generic/unzip@6.0")]);
        let s = declared_summary(&[("pkg:generic/unzip@6.0", "CVE-2024-0001", "insecure")]);
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&s);
        let (c, o) = claim_sets(&arts);
        assert!(!o.is_empty());
        assert_eq!(c, o, "declaration");

        let (comps, s) = contested_scan();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&s);
        let (c, o) = claim_sets(&arts);
        assert!(!o.is_empty());
        assert_eq!(c, o, "a declaration withholding the patch-derived not_affected");
    }

    /// T007.1 — under a root override the build entries name the override
    /// root, and nothing is lost.
    #[test]
    fn a_root_override_keeps_the_build_statement_on_the_new_root() {
        let comps = backport_scan();
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.root_override = crate::generate::RootComponentOverride {
            name: Some("renamed".to_string()),
            version: Some("2.0".to_string()),
            ..Default::default()
        };
        let doc = cdx(&arts);
        let build = vulns(&doc)
            .into_iter()
            .find(|e| e["analysis"]["state"] == "not_affected")
            .unwrap();
        assert_eq!(build["affects"], json!([{ "ref": root_ref(&doc) }]));
        assert_eq!(property(&doc, "waybill:vex-claims-omitted"), None);
    }

    /// T007.2 — a claim whose component is absent is not carried, and every
    /// format counts it.
    #[test]
    fn an_absent_subject_is_omitted_and_counted_in_every_format() {
        let comps = backport_scan();
        let s = declared_summary(&[("pkg:generic/absent@1.0", "CVE-2024-0002", "insecure")]);
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&s);
        let doc = cdx(&arts);
        assert!(vulns(&doc).iter().all(|e| e["id"] != "CVE-2024-0002"));
        assert_eq!(property(&doc, "waybill:vex-claims-omitted").as_deref(), Some("1"));
        for spdx in [
            serialize(&crate::generate::spdx::Spdx2_3JsonSerializer, &arts),
            serialize(&crate::generate::spdx::Spdx3JsonSerializer, &arts),
        ] {
            let text = spdx.to_string();
            assert!(
                text.contains(r#"\"field\":\"waybill:vex-claims-omitted\""#)
                    && text.contains(r#"\"value\":\"1\""#),
                "C193 missing from SPDX",
            );
        }
    }

    /// T012 — F1 and C193 agree across the three formats on documents
    /// waybill writes, including one that had to drop a claim.
    #[test]
    fn f1_and_c193_agree_across_formats() {
        let comps = backport_scan();
        let s = declared_summary(&[("pkg:generic/absent@1.0", "CVE-2024-0002", "insecure")]);
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&s);
        let (c, s23, s3) = (
            cdx(&arts),
            serialize(&crate::generate::spdx::Spdx2_3JsonSerializer, &arts),
            serialize(&crate::generate::spdx::Spdx3JsonSerializer, &arts),
        );
        for row in ["F1", "C193"] {
            let e = waybill::parity::extractors::EXTRACTORS
                .iter()
                .find(|e| e.row_id == row)
                .unwrap();
            let (a, b, d) = ((e.cdx)(&c), (e.spdx23)(&s23), (e.spdx3)(&s3));
            assert!(!a.is_empty(), "{row}: CDX extractor found nothing");
            assert_eq!(a, b, "{row}: CDX vs SPDX 2.3");
            assert_eq!(a, d, "{row}: CDX vs SPDX 3");
        }
    }

    /// T007.3 — a nested component is referenced by its composite `bom-ref`.
    #[test]
    fn a_nested_component_is_referenced_by_its_composite_bom_ref() {
        let mut comps = backport_scan();
        let parent = mk_component("pkg:generic/vendor@1.0");
        comps[0].parent_purl = Some(parent.purl.as_str().to_string());
        comps.push(parent);
        let integ = empty_integrity();
        let doc = cdx(&mk_artifacts(&comps, &integ));
        let unzip = ref_for(&doc, "pkg:generic/unzip@6.0");
        assert!(unzip.contains('#'), "expected a nested bom-ref, got {unzip}");
        let affected = vulns(&doc)
            .into_iter()
            .find(|e| e["analysis"]["state"] == "exploitable")
            .unwrap();
        assert_eq!(affected["affects"], json!([{ "ref": unzip }]));
    }

    /// T007.4 — a split document reaches the same path with its own
    /// components: a declaration about a component in another split is
    /// counted, not emitted.
    #[test]
    fn a_split_document_omits_what_another_split_holds() {
        let comps = with_root(vec![mk_component("pkg:generic/unzip@6.0")]);
        let s = declared_summary(&[("pkg:generic/unzip@6.0", "CVE-2024-0001", "insecure")]);
        let integ = empty_integrity();
        let mut arts = mk_artifacts(&comps, &integ);
        arts.nixpkgs_security_summary = Some(&s);
        let only_root = [comps[1].clone()];
        let narrowed = arts.narrow(&only_root, &[]);
        let doc = cdx(&narrowed);
        assert!(vulns(&doc).is_empty());
        assert_eq!(property(&doc, "waybill:vex-claims-omitted").as_deref(), Some("1"));
    }

    /// T008 (SC-003) — a statement-bearing document is valid CycloneDX 1.6.
    #[test]
    fn a_statement_bearing_document_is_valid_cyclonedx() {
        let comps = backport_scan();
        let integ = empty_integrity();
        let doc = cdx(&mk_artifacts(&comps, &integ));
        assert!(!vulns(&doc).is_empty());
        let errors = super::cdx_schema::cdx_validation_errors(&doc);
        assert!(errors.is_empty(), "{errors:#?}");
    }
}
