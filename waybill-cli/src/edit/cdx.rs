//! CycloneDX 1.6 adapter for `waybill sbom edit` (milestone 1071).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::select::{ComponentView, Scope};
use super::{bridge, is_protected, string_values, DropOutcome, SbomAdapter};

pub struct Cdx {
    doc: Value,
}

impl Cdx {
    pub fn new(doc: Value) -> Self {
        Self { doc }
    }

    fn all_components(&self) -> Vec<&Value> {
        fn walk<'a>(arr: Option<&'a Value>, out: &mut Vec<&'a Value>) {
            for c in arr.and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                out.push(c);
                walk(c.get("components"), out);
            }
        }
        let mut out = Vec::new();
        if let Some(root) = self.doc.pointer("/metadata/component") {
            out.push(root);
        }
        walk(self.doc.get("components"), &mut out);
        out
    }

    fn deps_edges(&self) -> BTreeMap<String, Vec<(String, Scope)>> {
        let mut edges = BTreeMap::new();
        for d in self.doc.get("dependencies").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
            let Some(r) = d.get("ref").and_then(Value::as_str) else { continue };
            let targets = d
                .get("dependsOn")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(|t| (t.to_string(), Scope::Runtime)).collect())
                .unwrap_or_default();
            edges.insert(r.to_string(), targets);
        }
        edges
    }
}

fn id_of(c: &Value) -> Option<String> {
    c.get("bom-ref").or_else(|| c.get("purl")).and_then(Value::as_str).map(str::to_string)
}

fn prop_values(c: &Value, name: &str) -> Vec<String> {
    c.get("properties")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter(|p| p.get("name").and_then(Value::as_str) == Some(name))
                .filter_map(|p| p.get("value"))
                .flat_map(string_values)
                .collect()
        })
        .unwrap_or_default()
}

fn retain_components(arr: &mut Value, ids: &BTreeSet<String>) -> usize {
    let Some(a) = arr.as_array_mut() else { return 0 };
    let before = a.len();
    a.retain(|c| id_of(c).is_none_or(|id| !ids.contains(&id)));
    let mut removed = before - a.len();
    for c in a.iter_mut() {
        if let Some(nested) = c.get_mut("components") {
            removed += retain_components(nested, ids);
        }
    }
    removed
}

fn strip_ids(arr: Option<&mut Value>, ids: &BTreeSet<String>) {
    if let Some(a) = arr.and_then(Value::as_array_mut) {
        a.retain(|v| v.as_str().is_none_or(|s| !ids.contains(s)));
    }
}

fn is_sorted(a: &[Value]) -> bool {
    a.windows(2).all(|w| w[0].as_str() <= w[1].as_str())
}

fn remove_props(obj: &mut Value, namespace: &str) -> usize {
    let Some(ps) = obj.get_mut("properties").and_then(Value::as_array_mut) else { return 0 };
    let before = ps.len();
    ps.retain(|p| {
        let name = p.get("name").and_then(Value::as_str).unwrap_or("");
        !name.starts_with(namespace) || is_protected(name)
    });
    let removed = before - ps.len();
    if ps.is_empty() {
        if let Some(m) = obj.as_object_mut() {
            m.remove("properties");
        }
    }
    removed
}

fn find_component_mut<'a>(arr: Option<&'a mut Value>, id: &str) -> Option<&'a mut Value> {
    for c in arr?.as_array_mut()?.iter_mut() {
        if id_of(c).as_deref() == Some(id) {
            return Some(c);
        }
        if let Some(found) = find_component_mut(c.get_mut("components"), id) {
            return Some(found);
        }
    }
    None
}

/// The PURL's `namespace/name` part, as written: `%40acme/internal-utils`.
pub(crate) fn purl_path(purl: &str) -> Option<String> {
    let rest = purl.strip_prefix("pkg:")?;
    let (_, after_type) = rest.split_once('/')?;
    let end = after_type.find(['@', '?', '#']).unwrap_or(after_type.len());
    Some(after_type[..end].to_string())
}

impl SbomAdapter for Cdx {
    fn components(&self) -> Vec<ComponentView> {
        self.all_components()
            .into_iter()
            .filter_map(|c| {
                let id = id_of(c)?;
                let mut scopes: BTreeSet<Scope> =
                    prop_values(c, "waybill:lifecycle-scope").iter().filter_map(|s| Scope::parse(s)).collect();
                if !prop_values(c, "waybill:optional-derivation").is_empty() {
                    scopes.insert(Scope::Optional);
                }
                if scopes.is_empty() && c.get("scope").and_then(Value::as_str) == Some("excluded") {
                    scopes.insert(Scope::Development);
                }
                let mut roles: BTreeSet<String> = prop_values(c, "waybill:component-role").into_iter().collect();
                if let Some(t) = c.get("type").and_then(Value::as_str) {
                    roles.insert(t.to_string());
                }
                Some(ComponentView {
                    id,
                    purl: c.get("purl").and_then(Value::as_str).map(str::to_string),
                    name: c.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    scopes,
                    tier: prop_values(c, "waybill:sbom-tier")
                        .into_iter()
                        .chain(prop_values(c, "waybill:component-tier"))
                        .next(),
                    roles,
                })
            })
            .collect()
    }

    fn root_ids(&self) -> BTreeSet<String> {
        self.doc.pointer("/metadata/component").and_then(id_of).into_iter().collect()
    }

    fn drop_components(&mut self, ids: &BTreeSet<String>) -> DropOutcome {
        if ids.is_empty() {
            return DropOutcome::default();
        }
        let edges = self.deps_edges();
        let (bridged, changed) = bridge(&edges, ids);
        let mut removed = 0;
        if let Some(arr) = self.doc.get_mut("components") {
            removed += retain_components(arr, ids);
        }
        if let Some(deps) = self.doc.get_mut("dependencies").and_then(Value::as_array_mut) {
            deps.retain(|d| d.get("ref").and_then(Value::as_str).is_none_or(|r| !ids.contains(r)));
            for d in deps.iter_mut() {
                let Some(r) = d.get("ref").and_then(Value::as_str).map(str::to_string) else { continue };
                if !changed.contains(&r) {
                    continue;
                }
                let was_sorted = d.get("dependsOn").and_then(Value::as_array).is_none_or(|a| is_sorted(a));
                let mut list: Vec<Value> = bridged
                    .get(&r)
                    .map(|ts| ts.iter().map(|(t, _)| Value::String(t.clone())).collect())
                    .unwrap_or_default();
                list.dedup();
                if was_sorted {
                    list.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                    list.dedup();
                }
                d["dependsOn"] = Value::Array(list);
            }
        }
        // Compositions: drop removed ids; a changed list is no longer complete.
        if let Some(comps) = self.doc.get_mut("compositions").and_then(Value::as_array_mut) {
            for c in comps.iter_mut() {
                strip_ids(c.get_mut("assemblies"), ids);
                strip_ids(c.get_mut("dependencies"), ids);
                if c.get("aggregate").and_then(Value::as_str) == Some("complete") {
                    strip_ids(c.get_mut("dependencies"), &changed);
                }
            }
            comps.retain(|c| {
                let empty = |k: &str| c.get(k).and_then(Value::as_array).is_none_or(Vec::is_empty);
                !(empty("assemblies") && empty("dependencies"))
            });
            if !changed.is_empty() {
                let list: Vec<Value> = changed.iter().cloned().map(Value::String).collect();
                if let Some(inc) = comps.iter_mut().find(|c| {
                    c.get("aggregate").and_then(Value::as_str) == Some("incomplete") && c.get("dependencies").is_some()
                }) {
                    if let Some(a) = inc.get_mut("dependencies").and_then(Value::as_array_mut) {
                        for v in list {
                            if !a.contains(&v) {
                                a.push(v);
                            }
                        }
                        a.sort_by(|x, y| x.as_str().cmp(&y.as_str()));
                    }
                } else {
                    comps.push(json!({ "aggregate": "incomplete", "dependencies": list }));
                }
            }
        }
        // Vulnerabilities: trim affects; a vulnerability that affects nothing goes.
        if let Some(vulns) = self.doc.get_mut("vulnerabilities").and_then(Value::as_array_mut) {
            vulns.retain_mut(|v| {
                let Some(affects) = v.get_mut("affects").and_then(Value::as_array_mut) else { return true };
                let had = !affects.is_empty();
                affects.retain(|a| a.get("ref").and_then(Value::as_str).is_none_or(|r| !ids.contains(r)));
                !(had && affects.is_empty())
            });
        }
        DropOutcome { removed, changed }
    }

    fn remove_annotations(&mut self, namespace: &str) -> usize {
        fn walk(arr: Option<&mut Value>, ns: &str) -> usize {
            let mut n = 0;
            for c in arr.and_then(Value::as_array_mut).map(|a| a.as_mut_slice()).unwrap_or(&mut []) {
                n += remove_props(c, ns);
                n += walk(c.get_mut("components"), ns);
            }
            n
        }
        let mut n = remove_props(&mut self.doc, namespace);
        if let Some(meta) = self.doc.get_mut("metadata") {
            n += remove_props(meta, namespace);
            if let Some(root) = meta.get_mut("component") {
                n += remove_props(root, namespace);
            }
        }
        n + walk(self.doc.get_mut("components"), namespace)
    }

    fn downgrade_graph_completeness(&mut self) {
        if let Some(ps) = self.doc.pointer_mut("/metadata/properties").and_then(Value::as_array_mut) {
            ps.retain(|p| p.get("name").and_then(Value::as_str) != Some("waybill:graph-completeness-reason"));
            for p in ps.iter_mut() {
                if p.get("name").and_then(Value::as_str) == Some("waybill:graph-completeness") {
                    p["value"] = json!("unknown");
                }
            }
        }
    }

    fn dangling_references(&self) -> Vec<String> {
        let known: BTreeSet<String> = self.all_components().into_iter().filter_map(id_of).collect();
        let mut out = Vec::new();
        let mut check = |loc: &str, v: &Value| {
            if let Some(s) = v.as_str() {
                if !known.contains(s) {
                    out.push(format!("{loc} -> {s}"));
                }
            }
        };
        for d in self.doc.get("dependencies").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(r) = d.get("ref") {
                check("dependencies[].ref", r);
            }
            for t in d.get("dependsOn").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                check("dependencies[].dependsOn", t);
            }
        }
        for c in self.doc.get("compositions").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
            for k in ["assemblies", "dependencies"] {
                for t in c.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                    check("compositions[]", t);
                }
            }
        }
        for v in self.doc.get("vulnerabilities").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
            for a in v.get("affects").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(r) = a.get("ref") {
                    check("vulnerabilities[].affects[].ref", r);
                }
            }
        }
        out
    }

    fn annotation_values(&self, subject: Option<&str>, field: &str) -> Vec<Value> {
        let holder = match subject {
            None => self.doc.get("metadata"),
            Some(id) => self.all_components().into_iter().find(|c| id_of(c).as_deref() == Some(id)),
        };
        holder
            .and_then(|h| h.get("properties"))
            .and_then(Value::as_array)
            .map(|ps| {
                ps.iter()
                    .filter(|p| p.get("name").and_then(Value::as_str) == Some(field))
                    .filter_map(|p| p.get("value").cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn native_paths(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in self.all_components() {
            let Some(id) = id_of(c) else { continue };
            for occ in c.pointer("/evidence/occurrences").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(l) = occ.get("location").and_then(Value::as_str) {
                    out.entry(id.clone()).or_default().push(l.to_string());
                }
            }
        }
        out
    }

    fn component_subtrees_mut(&mut self, id: &str) -> Vec<&mut Value> {
        let is_root = self.doc.pointer("/metadata/component").and_then(id_of).as_deref() == Some(id);
        if is_root {
            return self.doc.pointer_mut("/metadata/component").into_iter().collect();
        }
        find_component_mut(self.doc.get_mut("components"), id).into_iter().collect()
    }

    fn purl_of(&self, id: &str) -> Option<String> {
        self.all_components()
            .into_iter()
            .find(|c| id_of(c).as_deref() == Some(id))
            .and_then(|c| c.get("purl").and_then(Value::as_str))
            .and_then(purl_path)
    }

    fn derivation_record(&self) -> Option<Value> {
        prop_values(self.doc.get("metadata")?, "waybill:derivation")
            .first()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    fn attach_derivation(&mut self, record: &Value, original_sha256: &str) {
        // A modified BOM keeps its serial number and increments its version
        // (CycloneDX 1.6 `version`).
        let v = self.doc.get("version").and_then(Value::as_u64).unwrap_or(1);
        self.doc["version"] = json!(v + 1);
        let link = json!({
            "type": "bom",
            "url": format!("urn:sha256:{original_sha256}"),
            "hashes": [{ "alg": "SHA-256", "content": original_sha256 }],
            "comment": "waybill sbom edit: this BOM is derived from the BOM with this hash",
        });
        match self.doc.get_mut("externalReferences").and_then(Value::as_array_mut) {
            Some(refs) => refs.push(link),
            None => self.doc["externalReferences"] = json!([link]),
        }
        let prop = json!({ "name": "waybill:derivation", "value": super::derivation::canonical(record) });
        if self.doc.get("metadata").is_none() {
            self.doc["metadata"] = json!({});
        }
        let meta = &mut self.doc["metadata"];
        match meta.get_mut("properties").and_then(Value::as_array_mut) {
            Some(ps) => {
                ps.retain(|p| p.get("name").and_then(Value::as_str) != Some("waybill:derivation"));
                ps.push(prop);
            }
            None => meta["properties"] = json!([prop]),
        }
    }

    fn strip_original_signature(&mut self) {
        if let Some(root) = self.doc.as_object_mut() {
            root.remove("signature");
        }
        if let Some(meta) = self.doc.get_mut("metadata").and_then(Value::as_object_mut) {
            meta.remove("signature");
        }
        // A keyless original names its detached bundle (milestone 778).
        if let Some(refs) = self.doc.get_mut("externalReferences").and_then(Value::as_array_mut) {
            refs.retain(|r| {
                let url = r.get("url").and_then(Value::as_str).unwrap_or("");
                !(r.get("type").and_then(Value::as_str) == Some("attestation")
                    && !url.contains('/')
                    && (url.ends_with(".sig.bundle.json") || url.ends_with(".sig.json")))
            });
            if refs.is_empty() {
                if let Some(root) = self.doc.as_object_mut() {
                    root.remove("externalReferences");
                }
            }
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
            "bomFormat": "CycloneDX", "specVersion": "1.6", "version": 1,
            "metadata": {"component": {"bom-ref": "app", "name": "app", "type": "application"},
                         "properties": [{"name": "waybill:generation-context", "value": "filesystem-scan"},
                                        {"name": "waybill:graph-completeness", "value": "complete"}]},
            "components": [
                {"bom-ref": "b", "name": "b", "purl": "pkg:npm/b@1"},
                {"bom-ref": "c", "name": "c", "purl": "pkg:npm/c@1"},
                {"bom-ref": "d", "name": "d", "purl": "pkg:npm/d@1", "scope": "excluded",
                 "properties": [{"name": "waybill:lifecycle-scope", "value": "development"}]}
            ],
            "dependencies": [
                {"ref": "app", "dependsOn": ["b", "d"]},
                {"ref": "b", "dependsOn": ["c"]},
                {"ref": "c", "dependsOn": []},
                {"ref": "d", "dependsOn": []}
            ],
            "compositions": [{"aggregate": "complete", "assemblies": ["app", "b", "c", "d"], "dependencies": ["app", "b", "c", "d"]}],
            "vulnerabilities": [{"id": "CVE-1", "affects": [{"ref": "d"}]}, {"id": "CVE-2", "affects": [{"ref": "c"}]}]
        })
    }

    #[test]
    fn drop_bridges_cleans_and_downgrades() {
        let mut a = Cdx::new(doc());
        let out = a.drop_components(&BTreeSet::from(["b".to_string(), "d".to_string()]));
        assert_eq!(out.removed, 2);
        assert_eq!(out.changed, BTreeSet::from(["app".to_string()]));
        let d = a.doc();
        assert_eq!(d["dependencies"][0], json!({"ref": "app", "dependsOn": ["c"]}));
        assert_eq!(d["vulnerabilities"].as_array().unwrap().len(), 1);
        assert_eq!(d["compositions"][0]["dependencies"], json!(["c"]));
        assert_eq!(d["compositions"][1], json!({"aggregate": "incomplete", "dependencies": ["app"]}));
        assert!(a.dangling_references().is_empty());
    }

    #[test]
    fn scope_comes_from_the_annotation_or_excluded() {
        let a = Cdx::new(doc());
        let d = a.components().into_iter().find(|c| c.id == "d").unwrap();
        assert!(d.scopes.contains(&Scope::Development));
    }

    #[test]
    fn annotations_removed_except_protected() {
        let mut a = Cdx::new(doc());
        assert_eq!(a.remove_annotations("waybill:"), 2);
        assert_eq!(a.doc()["metadata"]["properties"], json!([{"name": "waybill:generation-context", "value": "filesystem-scan"}]));
    }

    #[test]
    fn purl_path_is_the_written_namespace_and_name() {
        assert_eq!(purl_path("pkg:npm/%40acme/internal-utils@2.0.1").as_deref(), Some("%40acme/internal-utils"));
        assert_eq!(purl_path("pkg:cargo/serde@1?x=y").as_deref(), Some("serde"));
    }
}
