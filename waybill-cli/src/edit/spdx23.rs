//! SPDX 2.3 adapter for `waybill sbom edit` (milestone 1071).
//!
//! Dependencies are relationships in two directions: `A DEPENDS_ON B`, and
//! the scoped, reversed `B DEV_DEPENDENCY_OF A` family. Bridging keeps every
//! existing relationship as it is (only edges to dropped packages go), and
//! adds bridged edges in the form their scope calls for.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::cdx::purl_path;
use super::select::{ComponentView, Scope};
use super::{bridge, envelope, envelope_field, envelope_value, is_protected, string_values, DropOutcome, SbomAdapter};

pub struct Spdx23 {
    doc: Value,
}

impl Spdx23 {
    pub fn new(doc: Value) -> Self {
        Self { doc }
    }

    fn packages(&self) -> &[Value] {
        self.doc.get("packages").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }

    fn relationships(&self) -> &[Value] {
        self.doc.get("relationships").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Dependency edges as (dependent → dependency, scope).
    fn edges(&self) -> BTreeMap<String, Vec<(String, Scope)>> {
        let mut edges: BTreeMap<String, Vec<(String, Scope)>> = BTreeMap::new();
        for p in self.packages() {
            if let Some(id) = p.get("SPDXID").and_then(Value::as_str) {
                edges.entry(id.to_string()).or_default();
            }
        }
        for r in self.relationships() {
            let (Some(a), Some(t), Some(b)) = (
                r.get("spdxElementId").and_then(Value::as_str),
                r.get("relationshipType").and_then(Value::as_str),
                r.get("relatedSpdxElement").and_then(Value::as_str),
            ) else {
                continue;
            };
            if let Some((from, to, scope)) = dependency_edge(a, t, b) {
                edges.entry(from.to_string()).or_default().push((to.to_string(), scope));
            }
        }
        edges
    }
}

/// `(dependent, dependency, scope)` for a dependency relationship.
fn dependency_edge<'a>(a: &'a str, t: &str, b: &'a str) -> Option<(&'a str, &'a str, Scope)> {
    match t {
        "DEPENDS_ON" => Some((a, b, Scope::Runtime)),
        "DEPENDENCY_OF" | "RUNTIME_DEPENDENCY_OF" | "PROVIDED_DEPENDENCY_OF" => Some((b, a, Scope::Runtime)),
        "OPTIONAL_DEPENDENCY_OF" => Some((b, a, Scope::Optional)),
        "DEV_DEPENDENCY_OF" => Some((b, a, Scope::Development)),
        "BUILD_DEPENDENCY_OF" => Some((b, a, Scope::Build)),
        "TEST_DEPENDENCY_OF" => Some((b, a, Scope::Test)),
        _ => None,
    }
}

fn edge_relationship(from: &str, to: &str, scope: Scope) -> Value {
    let comment = "waybill sbom edit: bridged across a dropped package";
    match scope {
        Scope::Runtime => json!({"spdxElementId": from, "relationshipType": "DEPENDS_ON", "relatedSpdxElement": to, "comment": comment}),
        s => {
            let t = match s {
                Scope::Development => "DEV_DEPENDENCY_OF",
                Scope::Build => "BUILD_DEPENDENCY_OF",
                Scope::Optional => "OPTIONAL_DEPENDENCY_OF",
                _ => "TEST_DEPENDENCY_OF",
            };
            json!({"spdxElementId": to, "relationshipType": t, "relatedSpdxElement": from, "comment": comment})
        }
    }
}

/// Values of a `waybill-annotation/v1` field among an object's annotations.
fn annotations_of(v: &Value, field: &str) -> Vec<Value> {
    v.get("annotations")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("comment").and_then(Value::as_str))
                .filter(|c| envelope_field(c).as_deref() == Some(field))
                .filter_map(envelope_value)
                .collect()
        })
        .unwrap_or_default()
}

fn annotation_strings(v: &Value, field: &str) -> Vec<String> {
    annotations_of(v, field).iter().flat_map(string_values).collect()
}

fn purl_of_package(p: &Value) -> Option<String> {
    p.get("externalRefs")?
        .as_array()?
        .iter()
        .find(|r| r.get("referenceType").and_then(Value::as_str) == Some("purl"))
        .and_then(|r| r.get("referenceLocator").and_then(Value::as_str))
        .map(str::to_string)
}

fn remove_envelopes(obj: &mut Value, namespace: &str) -> usize {
    let Some(a) = obj.get_mut("annotations").and_then(Value::as_array_mut) else { return 0 };
    let before = a.len();
    a.retain(|x| match x.get("comment").and_then(Value::as_str).and_then(envelope_field) {
        Some(f) => !f.starts_with(namespace) || is_protected(&f),
        None => true,
    });
    let removed = before - a.len();
    if a.is_empty() {
        if let Some(m) = obj.as_object_mut() {
            m.remove("annotations");
        }
    }
    removed
}

impl SbomAdapter for Spdx23 {
    fn components(&self) -> Vec<ComponentView> {
        let mut rel_scopes: BTreeMap<String, BTreeSet<Scope>> = BTreeMap::new();
        for r in self.relationships() {
            if let (Some(a), Some(t), Some(b)) = (
                r.get("spdxElementId").and_then(Value::as_str),
                r.get("relationshipType").and_then(Value::as_str),
                r.get("relatedSpdxElement").and_then(Value::as_str),
            ) {
                if let Some((_, to, s)) = dependency_edge(a, t, b) {
                    if s != Scope::Runtime {
                        rel_scopes.entry(to.to_string()).or_default().insert(s);
                    }
                }
            }
        }
        self.packages()
            .iter()
            .chain(self.doc.get("files").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]))
            .filter_map(|p| {
                let id = p.get("SPDXID").and_then(Value::as_str)?.to_string();
                let mut scopes: BTreeSet<Scope> =
                    annotation_strings(p, "waybill:lifecycle-scope").iter().filter_map(|s| Scope::parse(s)).collect();
                if !annotation_strings(p, "waybill:optional-derivation").is_empty() {
                    scopes.insert(Scope::Optional);
                }
                if scopes.is_empty() {
                    scopes = rel_scopes.get(&id).cloned().unwrap_or_default();
                }
                let mut roles: BTreeSet<String> = annotation_strings(p, "waybill:component-role").into_iter().collect();
                if let Some(t) = p.get("primaryPackagePurpose").and_then(Value::as_str) {
                    roles.insert(t.to_ascii_lowercase());
                }
                Some(ComponentView {
                    purl: purl_of_package(p),
                    name: p.get("name").or_else(|| p.get("fileName")).and_then(Value::as_str).unwrap_or("").to_string(),
                    scopes,
                    tier: annotation_strings(p, "waybill:sbom-tier")
                        .into_iter()
                        .chain(annotation_strings(p, "waybill:component-tier"))
                        .next(),
                    roles,
                    id,
                })
            })
            .collect()
    }

    fn root_ids(&self) -> BTreeSet<String> {
        let mut roots: BTreeSet<String> = self
            .doc
            .get("documentDescribes")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        for r in self.relationships() {
            if r.get("spdxElementId").and_then(Value::as_str) == Some("SPDXRef-DOCUMENT")
                && r.get("relationshipType").and_then(Value::as_str) == Some("DESCRIBES")
            {
                if let Some(b) = r.get("relatedSpdxElement").and_then(Value::as_str) {
                    roots.insert(b.to_string());
                }
            }
        }
        roots
    }

    fn drop_components(&mut self, ids: &BTreeSet<String>) -> DropOutcome {
        if ids.is_empty() {
            return DropOutcome::default();
        }
        let edges = self.edges();
        let (bridged, changed) = bridge(&edges, ids);
        let mut removed = 0;
        for key in ["packages", "files"] {
            if let Some(a) = self.doc.get_mut(key).and_then(Value::as_array_mut) {
                let before = a.len();
                a.retain(|p| p.get("SPDXID").and_then(Value::as_str).is_none_or(|i| !ids.contains(i)));
                removed += before - a.len();
            }
        }
        // New edges: what bridging added beyond the dependent's surviving edges.
        let mut added = Vec::new();
        for from in &changed {
            let kept: BTreeSet<(String, Scope)> = edges
                .get(from)
                .map(|ts| ts.iter().filter(|(t, _)| !ids.contains(t)).cloned().collect())
                .unwrap_or_default();
            for (to, s) in bridged.get(from).map(Vec::as_slice).unwrap_or(&[]) {
                if !kept.contains(&(to.clone(), *s)) {
                    added.push(edge_relationship(from, to, *s));
                }
            }
        }
        if let Some(rels) = self.doc.get_mut("relationships").and_then(Value::as_array_mut) {
            rels.retain(|r| {
                ["spdxElementId", "relatedSpdxElement"]
                    .iter()
                    .all(|k| r.get(*k).and_then(Value::as_str).is_none_or(|i| !ids.contains(i)))
            });
            rels.extend(added);
        }
        if let Some(d) = self.doc.get_mut("documentDescribes").and_then(Value::as_array_mut) {
            d.retain(|v| v.as_str().is_none_or(|s| !ids.contains(s)));
        }
        // Extracted licences no remaining package refers to.
        if self.doc.get("hasExtractedLicensingInfos").is_some() {
            let mut rest = self.doc.clone();
            if let Some(m) = rest.as_object_mut() {
                m.remove("hasExtractedLicensingInfos");
            }
            let text = rest.to_string();
            if let Some(infos) = self.doc.get_mut("hasExtractedLicensingInfos").and_then(Value::as_array_mut) {
                infos.retain(|i| i.get("licenseId").and_then(Value::as_str).is_none_or(|l| text.contains(l)));
            }
        }
        DropOutcome { removed, changed }
    }

    fn remove_annotations(&mut self, namespace: &str) -> usize {
        let mut n = remove_envelopes(&mut self.doc, namespace);
        for key in ["packages", "files"] {
            for p in self.doc.get_mut(key).and_then(Value::as_array_mut).map(|a| a.as_mut_slice()).unwrap_or(&mut []) {
                n += remove_envelopes(p, namespace);
            }
        }
        n
    }

    fn downgrade_graph_completeness(&mut self) {
        let Some(a) = self.doc.get_mut("annotations").and_then(Value::as_array_mut) else { return };
        a.retain(|x| {
            x.get("comment").and_then(Value::as_str).and_then(envelope_field).as_deref()
                != Some("waybill:graph-completeness-reason")
        });
        for x in a.iter_mut() {
            if x.get("comment").and_then(Value::as_str).and_then(envelope_field).as_deref()
                == Some("waybill:graph-completeness")
            {
                x["comment"] = json!(envelope("waybill:graph-completeness", json!("unknown")));
            }
        }
    }

    fn dangling_references(&self) -> Vec<String> {
        let mut known: BTreeSet<String> = BTreeSet::from(["SPDXRef-DOCUMENT".to_string()]);
        for key in ["packages", "files", "snippets"] {
            for p in self.doc.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(i) = p.get("SPDXID").and_then(Value::as_str) {
                    known.insert(i.to_string());
                }
            }
        }
        let resolves = |s: &str| known.contains(s) || s.starts_with("DocumentRef-") || s == "NOASSERTION" || s == "NONE";
        let mut out = Vec::new();
        for r in self.relationships() {
            for k in ["spdxElementId", "relatedSpdxElement"] {
                if let Some(s) = r.get(k).and_then(Value::as_str) {
                    if !resolves(s) {
                        out.push(format!("relationships[].{k} -> {s}"));
                    }
                }
            }
        }
        for v in self.doc.get("documentDescribes").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(s) = v.as_str().filter(|s| !resolves(s)) {
                out.push(format!("documentDescribes -> {s}"));
            }
        }
        out
    }

    fn annotation_values(&self, subject: Option<&str>, field: &str) -> Vec<Value> {
        match subject {
            None => annotations_of(&self.doc, field),
            Some(id) => ["packages", "files"]
                .iter()
                .flat_map(|k| self.doc.get(*k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]))
                .filter(|p| p.get("SPDXID").and_then(Value::as_str) == Some(id))
                .flat_map(|p| annotations_of(p, field))
                .collect(),
        }
    }

    fn native_paths(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for key in ["packages", "files"] {
            for p in self.doc.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                let Some(id) = p.get("SPDXID").and_then(Value::as_str) else { continue };
                for k in ["packageFileName", "fileName"] {
                    if let Some(s) = p.get(k).and_then(Value::as_str) {
                        out.entry(id.to_string()).or_default().push(s.trim_start_matches("./").to_string());
                    }
                }
            }
        }
        out
    }

    fn component_subtrees_mut(&mut self, id: &str) -> Vec<&mut Value> {
        let Some(obj) = self.doc.as_object_mut() else { return Vec::new() };
        obj.iter_mut()
            .filter(|(k, _)| matches!(k.as_str(), "packages" | "files"))
            .filter_map(|(_, v)| v.as_array_mut())
            .flat_map(|a| a.iter_mut())
            .filter(|p| p.get("SPDXID").and_then(Value::as_str) == Some(id))
            .collect()
    }

    fn purl_of(&self, id: &str) -> Option<String> {
        self.packages()
            .iter()
            .find(|p| p.get("SPDXID").and_then(Value::as_str) == Some(id))
            .and_then(purl_of_package)
            .and_then(|p| purl_path(&p))
    }

    fn derivation_record(&self) -> Option<Value> {
        annotation_strings(&self.doc, "waybill:derivation").first().and_then(|s| serde_json::from_str(s).ok())
    }

    fn attach_derivation(&mut self, record: &Value, original_sha256: &str) {
        // A derived document is a new document: it gets its own namespace,
        // and points at the original's (SPDX requires namespaces be unique).
        let original_ns = self.doc.get("documentNamespace").and_then(Value::as_str).unwrap_or("").to_string();
        let suffix = &super::derivation::sha256_hex(format!("{original_sha256}{record}").as_bytes())[..16];
        self.doc["documentNamespace"] = json!(format!("{original_ns}-edit-{suffix}"));

        let refs = self.doc.get("externalDocumentRefs").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        let doc_ref = if refs == 0 { "DocumentRef-original".to_string() } else { format!("DocumentRef-original-{}", refs + 1) };
        let ext = json!({
            "externalDocumentId": doc_ref,
            "spdxDocument": if original_ns.is_empty() { format!("urn:sha256:{original_sha256}") } else { original_ns },
            "checksum": { "algorithm": "SHA256", "checksumValue": original_sha256 },
        });
        match self.doc.get_mut("externalDocumentRefs").and_then(Value::as_array_mut) {
            Some(a) => a.push(ext),
            None => self.doc["externalDocumentRefs"] = json!([ext]),
        }
        let rel = json!({
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "AMENDS",
            "relatedSpdxElement": format!("{doc_ref}:SPDXRef-DOCUMENT"),
            "comment": "waybill sbom edit: this document is derived from the referenced one",
        });
        match self.doc.get_mut("relationships").and_then(Value::as_array_mut) {
            Some(a) => a.push(rel),
            None => self.doc["relationships"] = json!([rel]),
        }
        let annotation = json!({
            "annotationDate": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "annotationType": "OTHER",
            "annotator": format!("Tool: waybill-{}", env!("CARGO_PKG_VERSION")),
            "comment": envelope("waybill:derivation", json!(super::derivation::canonical(record))),
        });
        match self.doc.get_mut("annotations").and_then(Value::as_array_mut) {
            Some(a) => {
                a.retain(|x| {
                    x.get("comment").and_then(Value::as_str).and_then(envelope_field).as_deref() != Some("waybill:derivation")
                });
                a.push(annotation);
            }
            None => self.doc["annotations"] = json!([annotation]),
        }
    }

    fn doc(&self) -> &Value {
        &self.doc
    }

    fn doc_mut(&mut self) -> &mut Value {
        &mut self.doc
    }

    fn into_doc(self: Box<Self>) -> Value {
        self.doc
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn doc() -> Value {
        json!({
            "spdxVersion": "SPDX-2.3", "SPDXID": "SPDXRef-DOCUMENT", "documentNamespace": "https://x/doc",
            "documentDescribes": ["SPDXRef-app"],
            "annotations": [{"comment": envelope("waybill:graph-completeness", json!("complete"))},
                            {"comment": envelope("waybill:generation-context", json!("filesystem-scan"))}],
            "packages": [
                {"SPDXID": "SPDXRef-app", "name": "app"},
                {"SPDXID": "SPDXRef-b", "name": "b", "licenseDeclared": "LicenseRef-x"},
                {"SPDXID": "SPDXRef-c", "name": "c"},
                {"SPDXID": "SPDXRef-d", "name": "d"}
            ],
            "hasExtractedLicensingInfos": [{"licenseId": "LicenseRef-x", "extractedText": "x"}],
            "relationships": [
                {"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": "SPDXRef-app"},
                {"spdxElementId": "SPDXRef-app", "relationshipType": "DEPENDS_ON", "relatedSpdxElement": "SPDXRef-b"},
                {"spdxElementId": "SPDXRef-b", "relationshipType": "DEPENDS_ON", "relatedSpdxElement": "SPDXRef-c"},
                {"spdxElementId": "SPDXRef-d", "relationshipType": "DEV_DEPENDENCY_OF", "relatedSpdxElement": "SPDXRef-app"}
            ]
        })
    }

    #[test]
    fn dev_scope_is_read_from_the_relationship_type() {
        let a = Spdx23::new(doc());
        let d = a.components().into_iter().find(|c| c.id == "SPDXRef-d").unwrap();
        assert!(d.scopes.contains(&Scope::Development));
    }

    #[test]
    fn drop_bridges_and_cleans_licences() {
        let mut a = Spdx23::new(doc());
        let out = a.drop_components(&BTreeSet::from(["SPDXRef-b".to_string()]));
        assert_eq!(out.removed, 1);
        let rels = a.doc()["relationships"].as_array().unwrap();
        assert!(rels.iter().any(|r| r["spdxElementId"] == "SPDXRef-app" && r["relatedSpdxElement"] == "SPDXRef-c"));
        assert!(a.doc()["hasExtractedLicensingInfos"].as_array().unwrap().is_empty());
        assert!(a.dangling_references().is_empty());
    }

    #[test]
    fn graph_completeness_downgrades_to_unknown() {
        let mut a = Spdx23::new(doc());
        a.downgrade_graph_completeness();
        assert_eq!(annotation_strings(a.doc(), "waybill:graph-completeness"), vec!["unknown".to_string()]);
    }
}
