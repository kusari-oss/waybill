//! SPDX 3.0.1 adapter for `waybill sbom edit` (milestone 1071).
//!
//! Dependencies are `dependsOn` relationships grouped per (from, scope)
//! (milestone 1069): a plain `Relationship` for runtime, a
//! `LifecycleScopedRelationship` with `scope` otherwise, each carrying
//! `completeness`. Bridged edges join the dependent's relationship of the
//! right scope, or a new one; a changed list is marked `incomplete`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::cdx::purl_path;
use super::select::{ComponentView, Scope};
use super::{bridge, envelope, envelope_field, envelope_value, is_protected, string_values, DropOutcome, SbomAdapter};

pub struct Spdx3 {
    doc: Value,
}

const DEP_TYPES: [&str; 2] = ["Relationship", "LifecycleScopedRelationship"];

fn ty(e: &Value) -> &str {
    e.get("type").and_then(Value::as_str).unwrap_or("")
}

fn sid(e: &Value) -> Option<&str> {
    e.get("spdxId").and_then(Value::as_str)
}

fn is_component(e: &Value) -> bool {
    matches!(ty(e), "software_Package" | "software_File")
}

fn is_dep(e: &Value) -> bool {
    DEP_TYPES.contains(&ty(e)) && e.get("relationshipType").and_then(Value::as_str) == Some("dependsOn")
}

fn scope_of(e: &Value) -> Scope {
    e.get("scope").and_then(Value::as_str).and_then(Scope::parse).unwrap_or(Scope::Runtime)
}

fn strs(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

impl Spdx3 {
    pub fn new(doc: Value) -> Self {
        Self { doc }
    }

    fn graph(&self) -> &[Value] {
        self.doc.get("@graph").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }

    fn graph_mut(&mut self) -> &mut Vec<Value> {
        if !self.doc.get("@graph").is_some_and(Value::is_array) {
            self.doc["@graph"] = json!([]);
        }
        self.doc.get_mut("@graph").and_then(Value::as_array_mut).unwrap_or_else(|| unreachable!())
    }

    fn document(&self) -> Option<&Value> {
        self.graph().iter().find(|e| ty(e) == "SpdxDocument")
    }

    fn doc_iri(&self) -> Option<String> {
        self.document().and_then(sid).map(str::to_string)
    }

    fn annotations_about(&self, subject: &str, field: &str) -> Vec<String> {
        self.graph()
            .iter()
            .filter(|e| ty(e) == "Annotation" && e.get("subject").and_then(Value::as_str) == Some(subject))
            .filter_map(|e| e.get("statement").and_then(Value::as_str))
            .filter(|s| envelope_field(s).as_deref() == Some(field))
            .filter_map(envelope_value)
            .flat_map(|v| string_values(&v))
            .collect()
    }

    fn edges(&self) -> BTreeMap<String, Vec<(String, Scope)>> {
        let mut edges: BTreeMap<String, Vec<(String, Scope)>> = BTreeMap::new();
        for e in self.graph() {
            if is_component(e) {
                if let Some(id) = sid(e) {
                    edges.entry(id.to_string()).or_default();
                }
            }
            if is_dep(e) {
                if let Some(from) = e.get("from").and_then(Value::as_str) {
                    let s = scope_of(e);
                    let entry = edges.entry(from.to_string()).or_default();
                    for t in strs(e.get("to")) {
                        entry.push((t, s));
                    }
                }
            }
        }
        edges
    }
}

impl SbomAdapter for Spdx3 {
    fn components(&self) -> Vec<ComponentView> {
        let mut rel_scopes: BTreeMap<String, BTreeSet<Scope>> = BTreeMap::new();
        for e in self.graph().iter().filter(|e| is_dep(e)) {
            let s = scope_of(e);
            if s != Scope::Runtime {
                for t in strs(e.get("to")) {
                    rel_scopes.entry(t).or_default().insert(s);
                }
            }
        }
        self.graph()
            .iter()
            .filter(|e| is_component(e))
            .filter_map(|e| {
                let id = sid(e)?.to_string();
                let mut scopes: BTreeSet<Scope> = self
                    .annotations_about(&id, "waybill:lifecycle-scope")
                    .iter()
                    .filter_map(|s| Scope::parse(s))
                    .collect();
                // SPDX 3 has no optional scope; the signal rides this
                // annotation (milestone 179).
                if !self.annotations_about(&id, "waybill:optional-derivation").is_empty() {
                    scopes.insert(Scope::Optional);
                }
                if scopes.is_empty() {
                    scopes = rel_scopes.get(&id).cloned().unwrap_or_default();
                }
                let mut roles: BTreeSet<String> = self.annotations_about(&id, "waybill:component-role").into_iter().collect();
                if let Some(p) = e.get("software_primaryPurpose").and_then(Value::as_str) {
                    roles.insert(p.to_string());
                }
                Some(ComponentView {
                    purl: e.get("software_packageUrl").and_then(Value::as_str).map(str::to_string),
                    name: e.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    scopes,
                    tier: self
                        .annotations_about(&id, "waybill:sbom-tier")
                        .into_iter()
                        .chain(self.annotations_about(&id, "waybill:component-tier"))
                        .next(),
                    roles,
                    id,
                })
            })
            .collect()
    }

    fn root_ids(&self) -> BTreeSet<String> {
        self.graph()
            .iter()
            .filter(|e| matches!(ty(e), "SpdxDocument" | "software_Sbom"))
            .flat_map(|e| strs(e.get("rootElement")))
            .collect()
    }

    fn drop_components(&mut self, ids: &BTreeSet<String>) -> DropOutcome {
        if ids.is_empty() {
            return DropOutcome::default();
        }
        let edges = self.edges();
        let (bridged, changed) = bridge(&edges, ids);
        let doc_iri = self.doc_iri().unwrap_or_default();
        let creation = self
            .graph()
            .iter()
            .find_map(|e| e.get("creationInfo").and_then(Value::as_str))
            .unwrap_or("_:creation-info")
            .to_string();

        // Bridged additions per (from, scope), beyond the surviving edges.
        let mut additions: BTreeMap<(String, Scope), Vec<String>> = BTreeMap::new();
        for from in &changed {
            let kept: BTreeSet<(String, Scope)> = edges
                .get(from)
                .map(|ts| ts.iter().filter(|(t, _)| !ids.contains(t)).cloned().collect())
                .unwrap_or_default();
            for (to, s) in bridged.get(from).map(Vec::as_slice).unwrap_or(&[]) {
                if !kept.contains(&(to.clone(), *s)) {
                    additions.entry((from.clone(), *s)).or_default().push(to.clone());
                }
            }
        }

        let graph = self.graph_mut();
        let before = graph.len();
        graph.retain(|e| sid(e).is_none_or(|i| !ids.contains(i)));
        let removed = before - graph.len();

        for e in graph.iter_mut() {
            // Trim references to dropped elements.
            if let Some(to) = e.get_mut("to").and_then(Value::as_array_mut) {
                to.retain(|t| t.as_str().is_none_or(|s| !ids.contains(s)));
            }
            for k in ["rootElement", "element"] {
                if let Some(a) = e.get_mut(k).and_then(Value::as_array_mut) {
                    a.retain(|t| t.as_str().is_none_or(|s| !ids.contains(s)));
                }
            }
            // Join bridged edges to the dependent's relationship of that scope.
            if is_dep(e) {
                let from = e.get("from").and_then(Value::as_str).unwrap_or("").to_string();
                let key = (from.clone(), scope_of(e));
                if let Some(add) = additions.remove(&key) {
                    if let Some(to) = e.get_mut("to").and_then(Value::as_array_mut) {
                        to.extend(add.into_iter().map(Value::String));
                        to.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                        to.dedup();
                    }
                }
                if changed.contains(&from) {
                    e["completeness"] = json!("incomplete");
                }
            }
        }
        // Scopes the dependent had no relationship for yet.
        for ((from, s), mut to) in additions {
            to.sort();
            let suffix = &super::derivation::sha256_hex(format!("{from}|{}|{}", s.as_str(), to.join(",")).as_bytes())[..16];
            let mut rel = json!({
                "type": if s == Scope::Runtime { "Relationship" } else { "LifecycleScopedRelationship" },
                "spdxId": format!("{doc_iri}/relationship/edit-bridge-{suffix}"),
                "creationInfo": creation,
                "from": from,
                "relationshipType": "dependsOn",
                "to": to,
                "completeness": "incomplete",
            });
            if s != Scope::Runtime {
                rel["scope"] = json!(s.as_str());
            }
            graph.push(rel);
        }
        // Relationships from a dropped element, or now pointing at nothing.
        graph.retain(|e| {
            if !DEP_TYPES.contains(&ty(e)) && !ty(e).ends_with("Relationship") {
                return true;
            }
            let from_ok = e.get("from").and_then(Value::as_str).is_none_or(|f| !ids.contains(f));
            let to_ok = e.get("to").and_then(Value::as_array).is_none_or(|t| !t.is_empty());
            from_ok && to_ok
        });
        // Annotations about dropped elements.
        graph.retain(|e| ty(e) != "Annotation" || e.get("subject").and_then(Value::as_str).is_none_or(|s| !ids.contains(s)));
        // Licence expressions and vulnerabilities nothing refers to any more.
        // Any property may refer to them (`dataLicense` names a licence
        // element), so look in every value but an element's own identifier.
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for e in graph.iter() {
            for (k, v) in e.as_object().into_iter().flatten() {
                if k != "spdxId" {
                    super::redact::walk_strings(v, &mut |s| {
                        referenced.insert(s.to_string());
                    });
                }
            }
        }
        graph.retain(|e| {
            let orphanable = ty(e) == "simplelicensing_LicenseExpression" || ty(e).starts_with("security_");
            !orphanable || sid(e).is_some_and(|i| referenced.contains(i))
        });
        DropOutcome { removed, changed }
    }

    fn remove_annotations(&mut self, namespace: &str) -> usize {
        let graph = self.graph_mut();
        let before = graph.len();
        graph.retain(|e| {
            if ty(e) != "Annotation" {
                return true;
            }
            match e.get("statement").and_then(Value::as_str).and_then(envelope_field) {
                Some(f) => !f.starts_with(namespace) || is_protected(&f),
                None => true,
            }
        });
        before - graph.len()
    }

    fn downgrade_graph_completeness(&mut self) {
        let graph = self.graph_mut();
        graph.retain(|e| {
            ty(e) != "Annotation"
                || e.get("statement").and_then(Value::as_str).and_then(envelope_field).as_deref()
                    != Some("waybill:graph-completeness-reason")
        });
        for e in graph.iter_mut() {
            if ty(e) == "Annotation"
                && e.get("statement").and_then(Value::as_str).and_then(envelope_field).as_deref()
                    == Some("waybill:graph-completeness")
            {
                e["statement"] = json!(envelope("waybill:graph-completeness", json!("unknown")));
            }
        }
    }

    fn dangling_references(&self) -> Vec<String> {
        let mut known: BTreeSet<String> = self.graph().iter().filter_map(sid).map(str::to_string).collect();
        known.extend(["NoAssertionElement", "NoneElement"].map(str::to_string));
        for e in self.graph() {
            for m in e.get("import").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(x) = m.get("externalSpdxId").and_then(Value::as_str) {
                    known.insert(x.to_string());
                }
            }
        }
        let mut out = Vec::new();
        for e in self.graph() {
            let mut refs: Vec<(&str, String)> = Vec::new();
            if ty(e).ends_with("Relationship") {
                refs.extend(e.get("from").and_then(Value::as_str).map(|f| ("from", f.to_string())));
                refs.extend(strs(e.get("to")).into_iter().map(|t| ("to", t)));
            }
            if ty(e) == "Annotation" {
                refs.extend(e.get("subject").and_then(Value::as_str).map(|s| ("subject", s.to_string())));
            }
            for k in ["rootElement", "element"] {
                refs.extend(strs(e.get(k)).into_iter().map(|t| (k, t)));
            }
            for (k, r) in refs {
                if !known.contains(&r) {
                    out.push(format!("{} {k} -> {r}", ty(e)));
                }
            }
        }
        out
    }

    fn annotation_values(&self, subject: Option<&str>, field: &str) -> Vec<Value> {
        let Some(subject) = subject.map(str::to_string).or_else(|| self.doc_iri()) else { return Vec::new() };
        self.graph()
            .iter()
            .filter(|e| ty(e) == "Annotation" && e.get("subject").and_then(Value::as_str) == Some(subject.as_str()))
            .filter_map(|e| e.get("statement").and_then(Value::as_str))
            .filter(|s| envelope_field(s).as_deref() == Some(field))
            .filter_map(envelope_value)
            .collect()
    }

    fn native_paths(&self) -> BTreeMap<String, Vec<String>> {
        BTreeMap::new()
    }

    fn component_subtrees_mut(&mut self, id: &str) -> Vec<&mut Value> {
        self.graph_mut()
            .iter_mut()
            .filter(|e| {
                sid(e) == Some(id) || (ty(e) == "Annotation" && e.get("subject").and_then(Value::as_str) == Some(id))
            })
            .collect()
    }

    fn purl_of(&self, id: &str) -> Option<String> {
        self.graph()
            .iter()
            .find(|e| sid(e) == Some(id))
            .and_then(|e| e.get("software_packageUrl").and_then(Value::as_str))
            .and_then(purl_path)
    }

    fn derivation_record(&self) -> Option<Value> {
        let doc = self.doc_iri()?;
        self.annotations_about(&doc, "waybill:derivation").first().and_then(|s| serde_json::from_str(s).ok())
    }

    fn attach_derivation(&mut self, record: &Value, original_sha256: &str) {
        let Some(old_iri) = self.doc_iri() else { return };
        // A derived document is a new SpdxDocument; its elements keep their
        // IRIs, since they are the same elements.
        let suffix = &super::derivation::sha256_hex(format!("{original_sha256}{record}").as_bytes())[..16];
        let new_iri = format!("{old_iri}-edit-{suffix}");
        let creation = self
            .graph()
            .iter()
            .find_map(|e| e.get("creationInfo").and_then(Value::as_str))
            .unwrap_or("_:creation-info")
            .to_string();
        let graph = self.graph_mut();
        // Retarget references to the document itself (not element IRIs that
        // merely share its prefix).
        for e in graph.iter_mut() {
            for k in ["spdxId", "subject", "from"] {
                if e.get(k).and_then(Value::as_str) == Some(old_iri.as_str()) {
                    e[k] = json!(new_iri);
                }
            }
            if let Some(to) = e.get_mut("to").and_then(Value::as_array_mut) {
                for t in to.iter_mut() {
                    if t.as_str() == Some(old_iri.as_str()) {
                        *t = json!(new_iri);
                    }
                }
            }
        }
        // The previous derivation record (now an ancestor inside the new one).
        graph.retain(|e| {
            ty(e) != "Annotation"
                || e.get("statement").and_then(Value::as_str).and_then(envelope_field).as_deref()
                    != Some("waybill:derivation")
        });
        if let Some(doc) = graph.iter_mut().find(|e| ty(e) == "SpdxDocument") {
            let map = json!({
                "type": "ExternalMap",
                "externalSpdxId": old_iri,
                "verifiedUsing": [{ "type": "Hash", "algorithm": "sha256", "hashValue": original_sha256 }],
            });
            match doc.get_mut("import").and_then(Value::as_array_mut) {
                Some(a) => a.push(map),
                None => doc["import"] = json!([map]),
            }
        }
        graph.push(json!({
            "type": "Relationship",
            "spdxId": format!("{new_iri}/relationship/amended-by"),
            "creationInfo": creation,
            "from": old_iri,
            "relationshipType": "amendedBy",
            "to": [new_iri],
            "comment": "waybill sbom edit: this document is derived from the imported one",
        }));
        graph.push(json!({
            "type": "Annotation",
            "spdxId": format!("{new_iri}/anno-derivation"),
            "creationInfo": creation,
            "annotationType": "other",
            "subject": new_iri,
            "statement": envelope("waybill:derivation", json!(super::derivation::canonical(record))),
        }));
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
        json!({"@graph": [
            {"type": "SpdxDocument", "spdxId": "D", "creationInfo": "_:c", "rootElement": ["app"]},
            {"type": "software_Package", "spdxId": "app", "name": "app", "creationInfo": "_:c"},
            {"type": "software_Package", "spdxId": "b", "name": "b", "creationInfo": "_:c"},
            {"type": "software_Package", "spdxId": "c", "name": "c", "creationInfo": "_:c"},
            {"type": "software_Package", "spdxId": "d", "name": "d", "creationInfo": "_:c"},
            {"type": "simplelicensing_LicenseExpression", "spdxId": "L", "creationInfo": "_:c"},
            {"type": "Relationship", "spdxId": "r1", "from": "app", "relationshipType": "dependsOn", "to": ["b"], "completeness": "complete", "creationInfo": "_:c"},
            {"type": "Relationship", "spdxId": "r2", "from": "b", "relationshipType": "dependsOn", "to": ["c", "d"], "completeness": "complete", "creationInfo": "_:c"},
            {"type": "Relationship", "spdxId": "r3", "from": "b", "relationshipType": "hasDeclaredLicense", "to": ["L"], "creationInfo": "_:c"},
            {"type": "Annotation", "spdxId": "a1", "subject": "b", "statement": "x", "creationInfo": "_:c"}
        ]})
    }

    #[test]
    fn drop_bridges_marks_incomplete_and_removes_orphans() {
        let mut a = Spdx3::new(doc());
        let out = a.drop_components(&BTreeSet::from(["b".to_string()]));
        assert_eq!(out.removed, 1);
        let g = a.doc()["@graph"].as_array().unwrap();
        let r1 = g.iter().find(|e| e["spdxId"] == "r1").unwrap();
        assert_eq!(r1["to"], json!(["c", "d"]));
        assert_eq!(r1["completeness"], json!("incomplete"));
        assert!(!g.iter().any(|e| e["spdxId"] == "L" || e["spdxId"] == "a1" || e["spdxId"] == "r2" || e["spdxId"] == "r3"));
        assert!(a.dangling_references().is_empty());
    }

    #[test]
    fn attach_gives_a_new_document_iri_and_links_the_old_one() {
        let mut a = Spdx3::new(doc());
        a.attach_derivation(&json!({"schema": "waybill-derivation/v1"}), "ab");
        let g = a.doc()["@graph"].as_array().unwrap();
        let d = g.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
        assert!(d["spdxId"].as_str().unwrap().starts_with("D-edit-"));
        assert_eq!(d["import"][0]["externalSpdxId"], json!("D"));
        assert!(g.iter().any(|e| e["relationshipType"] == "amendedBy" && e["from"] == "D"));
        assert!(a.dangling_references().is_empty());
    }
}
