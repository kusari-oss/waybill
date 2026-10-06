//! `waybill sbom edit`: derive a distributable SBOM from an emitted one
//! (milestone 1071, #1129).
//!
//! The model is "generate everything once, derive what you distribute".
//! An edit reads one CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 document,
//! applies ordered operations and writes the same format. It never goes
//! through a format-neutral model: each format's adapter edits the
//! document's own JSON, so content the edit doesn't target is unchanged.
//! waybill writes CycloneDX and SPDX 3 with sorted keys and 2-space
//! indentation, so for those "unchanged" means byte-identical; SPDX 2.3,
//! written in field order, comes back as the same JSON with sorted keys
//! (research R2).
//!
//! Every edit is fail-closed: after the operations, the pipeline checks that
//! no dropped identifier remains, that every reference resolves, and that
//! no redacted value remains. If any check fails, nothing is written.

pub mod cdx;
pub mod derivation;
pub mod redact;
pub mod select;
pub mod spdx23;
pub mod spdx3;

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::redact::{RedactClass, RedactMode, Redactor};
use self::select::{ComponentView, Scope, Selector};

/// Annotations no edit can remove. `waybill:generation-context` is C21:
/// the constitution's Operating Modes require every document to state which
/// mode produced it. `waybill:derivation` is C194, the edit's own record.
pub const PROTECTED_ANNOTATIONS: &[&str] = &["waybill:generation-context", "waybill:derivation"];

pub(crate) fn is_protected(field: &str) -> bool {
    PROTECTED_ANNOTATIONS.contains(&field)
}

/// One edit operation. This is the vocabulary a policy file will list in
/// the next milestone (FR-014): the command line parses into it, and so will
/// the policy file. Re-identification (`ReIdentify`) is reserved for that
/// milestone (FR-015).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum EditOp {
    DropComponents { selector: Selector },
    DropAnnotations { namespace: String },
    Redact {
        class: RedactClass,
        mode: RedactMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
}

impl EditOp {
    /// The derivation record's category for this operation.
    pub fn category(&self) -> &'static str {
        match self {
            Self::DropComponents { .. } => "drop-components",
            Self::DropAnnotations { .. } => "drop-annotations",
            Self::Redact { class, .. } => class.category(),
        }
    }
}

/// What one operation matched and changed, in format-independent units
/// (data-model.md): components selected and removed; annotation entries
/// removed; distinct values matched and replaced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpReport {
    pub category: String,
    pub matched: usize,
    pub changed: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditReport {
    pub ops: Vec<OpReport>,
    /// Components whose dependency list changed (bridging or removal).
    pub dependency_lists_changed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    CycloneDx16,
    Spdx23,
    Spdx301,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::CycloneDx16 => "cyclonedx-1.6",
            Self::Spdx23 => "spdx-2.3",
            Self::Spdx301 => "spdx-3.0.1",
        }
    }
}

/// Detect the input format (research R10). Mirrors the detection in
/// `binding/verify.rs`, and refuses anything this command can't edit
/// faithfully.
pub fn detect(doc: &Value) -> Result<Format> {
    if doc.get("@graph").is_some() {
        return Ok(Format::Spdx301);
    }
    if doc.get("spdxVersion").and_then(Value::as_str) == Some("SPDX-2.3") {
        return Ok(Format::Spdx23);
    }
    if doc.get("bomFormat").and_then(Value::as_str) == Some("CycloneDX") {
        return match doc.get("specVersion").and_then(Value::as_str) {
            Some("1.6") => Ok(Format::CycloneDx16),
            other => bail!(
                "unsupported CycloneDX specVersion {}: sbom edit supports 1.6",
                other.unwrap_or("(none)")
            ),
        };
    }
    if let Some(v) = doc.get("spdxVersion").and_then(Value::as_str) {
        bail!("unsupported SPDX version {v}: sbom edit supports SPDX-2.3 and SPDX 3.0.1");
    }
    bail!("input is not a CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 JSON document")
}

/// What removing components did.
#[derive(Debug, Default)]
pub struct DropOutcome {
    pub removed: usize,
    /// Components whose dependency list gained bridged edges or lost
    /// members; their completeness claims are downgraded.
    pub changed: BTreeSet<String>,
}

/// One format's editor over the document's own JSON.
pub trait SbomAdapter {
    fn components(&self) -> Vec<ComponentView>;
    /// The document's root or subject: never droppable (FR-016).
    fn root_ids(&self) -> BTreeSet<String>;
    fn drop_components(&mut self, ids: &BTreeSet<String>) -> DropOutcome;
    /// Remove annotations whose field starts with `namespace`, sparing the
    /// protected set. Returns the number of annotation entries removed.
    fn remove_annotations(&mut self, namespace: &str) -> usize;
    /// Set `waybill:graph-completeness` (C104) to `unknown` and drop its
    /// reason (C105). Used when an edit changed dependency lists: they're no
    /// longer known complete, and `partial` needs a reason code the closed
    /// vocabulary doesn't have.
    fn downgrade_graph_completeness(&mut self);
    /// References that point at nothing, as `location -> target` strings.
    fn dangling_references(&self) -> Vec<String>;
    /// The raw values of a `waybill`-convention annotation (property or
    /// envelope) on a component, or on the document when `subject` is
    /// `None`.
    fn annotation_values(&self, subject: Option<&str>, field: &str) -> Vec<Value>;
    /// Paths a format carries natively rather than in an annotation
    /// (CycloneDX `evidence.occurrences`, SPDX 2.3 file names), per
    /// component.
    fn native_paths(&self) -> BTreeMap<String, Vec<String>>;
    /// Every path-bearing value, per component (the document's own under
    /// `""`), for path redaction. One definition for all three formats, so
    /// they collect the same paths.
    fn path_values(&self) -> BTreeMap<String, Vec<String>> {
        let mut out = self.native_paths();
        let mut subjects: Vec<Option<String>> = vec![None];
        subjects.extend(self.components().into_iter().map(|c| Some(c.id)));
        for subject in subjects {
            for field in PATH_FIELDS {
                for v in self.annotation_values(subject.as_deref(), field) {
                    let mut paths = Vec::new();
                    path_strings(field, &v, &mut paths);
                    out.entry(subject.clone().unwrap_or_default()).or_default().extend(paths);
                }
            }
        }
        out.retain(|_, v| {
            v.sort();
            v.dedup();
            !v.is_empty()
        });
        out
    }
    /// The JSON subtrees that belong to one component (its object, plus
    /// annotations about it in formats that keep them apart), for
    /// component-scoped redaction.
    fn component_subtrees_mut(&mut self, id: &str) -> Vec<&mut Value>;
    /// The component's PURL path segment as written (`%40acme/internal-utils`).
    fn purl_of(&self, id: &str) -> Option<String>;
    /// The original's own derivation record, if it was itself derived.
    fn derivation_record(&self) -> Option<Value>;
    fn attach_derivation(&mut self, record: &Value, original_sha256: &str);
    /// Remove the original's own signature, or the reference to it, from
    /// the document: it signs bytes that no longer exist, and its material
    /// moves into the derivation record. Only CycloneDX carries one.
    fn strip_original_signature(&mut self) {}
    fn doc(&self) -> &Value;
    fn doc_mut(&mut self) -> &mut Value;
    fn into_doc(self: Box<Self>) -> Value;
}

/// Annotation fields whose values are, or contain, paths (catalogue rows
/// C18, C25, C31, C63, C66, C76, C92, C120, C121, C130, C136, and D2 where a
/// format carries evidence as an annotation).
pub(crate) const PATH_FIELDS: &[&str] = &[
    "waybill:source-files",
    "waybill:file-paths",
    "waybill:elf-runpath",
    "waybill:macho-rpath",
    "waybill:exclude-path",
    "waybill:supplement-cdx",
    "waybill:bbappend-applied",
    "waybill:workspace-member",
    "waybill:workspaces-detected",
    "waybill:source-read-set",
    "waybill:go-toolchain-detected",
    "evidence.occurrences",
];

/// The paths in one annotation value: JSON-encoded strings are decoded,
/// arrays walked, objects read at their `path`/`location`/`read_set` keys.
fn path_strings(field: &str, v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            let t = s.trim_start();
            if t.starts_with('[') || t.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<Value>(t) {
                    return path_strings(field, &parsed, out);
                }
            }
            match field {
                "waybill:exclude-path" => out.extend(s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty())),
                "waybill:supplement-cdx" => out.push(s.split("@sha256:").next().unwrap_or(s).to_string()),
                _ => out.push(s.clone()),
            }
        }
        Value::Array(a) => a.iter().for_each(|x| path_strings(field, x, out)),
        Value::Object(m) => {
            for k in ["path", "location", "read_set"] {
                if let Some(x) = m.get(k) {
                    path_strings(field, x, out);
                }
            }
        }
        _ => {}
    }
}

pub fn adapter_for(format: Format, doc: Value) -> Box<dyn SbomAdapter> {
    match format {
        Format::CycloneDx16 => Box::new(cdx::Cdx::new(doc)),
        Format::Spdx23 => Box::new(spdx23::Spdx23::new(doc)),
        Format::Spdx301 => Box::new(spdx3::Spdx3::new(doc)),
    }
}

/// Options that aren't operations.
#[derive(Default)]
pub struct EditOptions {
    /// HMAC key for pseudonymisation; required if any operation uses it.
    pub redact_key: Option<Vec<u8>>,
    /// The original's signature material, found by the caller (sidecar,
    /// override, or the CycloneDX embedded signature).
    pub original_signature: derivation::OriginalSignature,
}

pub struct EditOutcome {
    pub format: Format,
    pub bytes: Vec<u8>,
    pub report: EditReport,
}

/// Run an edit. Returns the output bytes, or an error with nothing written.
pub fn run(input: &[u8], ops: &[EditOp], opts: &EditOptions) -> Result<EditOutcome> {
    let doc: Value = serde_json::from_slice(input).context("input is not valid JSON")?;
    let format = detect(&doc)?;
    let ancestors = {
        let probe = adapter_for(format, doc.clone());
        probe.derivation_record()
    };
    let mut adapter = adapter_for(format, doc);
    adapter.strip_original_signature();
    let roots = adapter.root_ids();
    let mut report = EditReport::default();
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    let mut redactor = Redactor::new(opts.redact_key.clone());
    let mut changed_lists: BTreeSet<String> = BTreeSet::new();

    for op in ops {
        let op_report = match op {
            EditOp::DropComponents { selector } => {
                let selected: BTreeSet<String> = adapter
                    .components()
                    .into_iter()
                    .filter(|c| selector.matches(c))
                    .map(|c| c.id)
                    .collect();
                if let Some(root) = selected.intersection(&roots).next() {
                    bail!(
                        "selector `{selector}` matches the document's root/subject `{root}`; \
                         dropping it would leave a document describing nothing"
                    );
                }
                let outcome = adapter.drop_components(&selected);
                changed_lists.extend(outcome.changed);
                dropped.extend(selected.iter().cloned());
                OpReport {
                    category: op.category().to_string(),
                    matched: selected.len(),
                    changed: outcome.removed,
                }
            }
            EditOp::DropAnnotations { namespace } => {
                let removed = adapter.remove_annotations(namespace);
                OpReport {
                    category: op.category().to_string(),
                    matched: removed,
                    changed: removed,
                }
            }
            EditOp::Redact { class, mode, pattern } => {
                if *mode == RedactMode::Pseudonymise && opts.redact_key.is_none() {
                    bail!("--redact {}:pseudonymise needs a key: pass --redact-key-file", class.as_str());
                }
                let (matched, changed) =
                    redactor.apply(adapter.as_mut(), *class, *mode, pattern.as_deref())?;
                OpReport {
                    category: op.category().to_string(),
                    matched,
                    changed,
                }
            }
        };
        report.ops.push(op_report);
    }

    // Dropped ids are gone from bridged lists already; any list that changed
    // is no longer known complete.
    changed_lists.retain(|id| !dropped.contains(id));
    report.dependency_lists_changed = changed_lists.len();
    if !changed_lists.is_empty() {
        adapter.downgrade_graph_completeness();
    }

    // Post-conditions, before anything is added or written.
    let dangling = adapter.dangling_references();
    if let Some(first) = dangling.first() {
        bail!(
            "edit left {} reference(s) that resolve to nothing (first: {first}); nothing written",
            dangling.len()
        );
    }
    let flat = serde_json::to_string(adapter.doc()).context("serialising edited document")?;
    for id in &dropped {
        if flat.contains(&serde_json::to_string(id)?) {
            bail!("a dropped component's identifier is still referenced in the output; nothing written");
        }
    }
    redactor.check_no_leaks(adapter.as_mut())?;

    // Derivation record (US3).
    let original_sha256 = derivation::sha256_hex(input);
    let signature = opts.original_signature.for_record(&redactor);
    let record = derivation::DerivationRecord::new(
        &original_sha256,
        format,
        signature,
        report.ops.clone(),
        ancestors.map(|a| derivation::scrub_ancestor(a, &redactor)),
    );
    adapter.attach_derivation(&record.to_value(), &original_sha256);
    // The record is the only thing added after the first check.
    redactor.check_no_leaks(adapter.as_mut())?;

    let out = adapter.into_doc();
    let mut bytes = serde_json::to_string_pretty(&out).context("serialising edited document")?.into_bytes();
    if input.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    Ok(EditOutcome { format, bytes, report })
}

/// Bridge dependency edges across dropped components (FR-005, research R4).
///
/// `edges` maps each component to its dependencies, in document order, with
/// the edge's scope. Returns the new edges of every component that isn't
/// dropped, and the set of components whose list changed.
pub(crate) type Edges = BTreeMap<String, Vec<(String, Scope)>>;

pub(crate) fn bridge(edges: &Edges, dropped: &BTreeSet<String>) -> (Edges, BTreeSet<String>) {
    let mut out = BTreeMap::new();
    let mut changed = BTreeSet::new();
    for (from, targets) in edges {
        if dropped.contains(from) {
            continue;
        }
        let mut list: Vec<(String, Scope)> = Vec::new();
        let push = |list: &mut Vec<(String, Scope)>, to: String, scope: Scope| {
            if &to != from && !list.iter().any(|(t, s)| *t == to && *s == scope) {
                list.push((to, scope));
            }
        };
        for (to, scope) in targets {
            if !dropped.contains(to) {
                push(&mut list, to.clone(), *scope);
                continue;
            }
            // Walk through consecutive dropped components.
            let mut stack = vec![(to.clone(), *scope)];
            let mut seen = BTreeSet::new();
            while let Some((node, into)) = stack.pop() {
                if !seen.insert(node.clone()) {
                    continue;
                }
                for (next, out_scope) in edges.get(&node).map(Vec::as_slice).unwrap_or(&[]) {
                    let s = Scope::bridge(into, *out_scope);
                    if dropped.contains(next) {
                        stack.push((next.clone(), s));
                    } else {
                        push(&mut list, next.clone(), s);
                    }
                }
            }
        }
        if &list != targets {
            changed.insert(from.clone());
        }
        out.insert(from.clone(), list);
    }
    (out, changed)
}

/// The `field` of a `waybill-annotation/v1` envelope, if `s` is one.
pub(crate) fn envelope_field(s: &str) -> Option<String> {
    let v: Value = serde_json::from_str(s).ok()?;
    if v.get("schema").and_then(Value::as_str) != Some("waybill-annotation/v1") {
        return None;
    }
    v.get("field").and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn envelope_value(s: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(s).ok()?;
    if v.get("schema").and_then(Value::as_str) != Some("waybill-annotation/v1") {
        return None;
    }
    v.get("value").cloned()
}

pub(crate) fn envelope(field: &str, value: Value) -> String {
    let mut m = serde_json::Map::new();
    m.insert("field".into(), Value::String(field.into()));
    m.insert("schema".into(), Value::String("waybill-annotation/v1".into()));
    m.insert("value".into(), value);
    Value::Object(m).to_string()
}

/// String values of an annotation value: a plain string, or a JSON array of
/// strings encoded in a string (`["src/a.rs"]`), as waybill writes them.
pub(crate) fn string_values(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => vec![s.clone()],
        },
        Value::Array(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    fn edges(list: &[(&str, &[(&str, Scope)])]) -> BTreeMap<String, Vec<(String, Scope)>> {
        list.iter()
            .map(|(f, ts)| (f.to_string(), ts.iter().map(|(t, s)| (t.to_string(), *s)).collect()))
            .collect()
    }

    #[test]
    fn detect_each_format_and_refusals() {
        assert_eq!(detect(&json!({"bomFormat":"CycloneDX","specVersion":"1.6"})).unwrap(), Format::CycloneDx16);
        assert_eq!(detect(&json!({"spdxVersion":"SPDX-2.3","packages":[]})).unwrap(), Format::Spdx23);
        assert_eq!(detect(&json!({"@graph":[]})).unwrap(), Format::Spdx301);
        assert!(detect(&json!({"bomFormat":"CycloneDX","specVersion":"1.4"})).unwrap_err().to_string().contains("1.6"));
        assert!(detect(&json!({"spdxVersion":"SPDX-2.2"})).unwrap_err().to_string().contains("SPDX-2.3"));
        assert!(detect(&json!({"hello":1})).is_err());
    }

    #[test]
    fn bridge_connects_through_one_and_several_dropped() {
        use Scope::*;
        let e = edges(&[
            ("app", &[("b", Runtime), ("x", Runtime)]),
            ("b", &[("c", Runtime)]),
            ("c", &[("d", Development)]),
            ("x", &[]),
            ("d", &[]),
        ]);
        let (out, changed) = bridge(&e, &BTreeSet::from(["b".to_string(), "c".to_string()]));
        // Bridged edges take the dropped edge's place in document order.
        assert_eq!(out["app"], vec![("d".to_string(), Development), ("x".to_string(), Runtime)]);
        assert!(changed.contains("app") && !out.contains_key("b"));
    }

    #[test]
    fn bridge_never_self_loops_or_duplicates() {
        use Scope::*;
        let e = edges(&[("a", &[("b", Runtime), ("c", Runtime)]), ("b", &[("a", Runtime), ("c", Runtime)]), ("c", &[])]);
        let (out, _) = bridge(&e, &BTreeSet::from(["b".to_string()]));
        assert_eq!(out["a"], vec![("c".to_string(), Runtime)]);
    }

    #[test]
    fn op_serde_is_the_policy_vocabulary() {
        let op: EditOp = serde_json::from_value(json!({"action":"drop-components","selector":"scope=development"})).unwrap();
        assert_eq!(op.category(), "drop-components");
        let op: EditOp = serde_json::from_value(json!({"action":"redact","class":"names","mode":"pseudonymise","pattern":"@acme/*"})).unwrap();
        assert_eq!(op.category(), "redact-names");
    }

    #[test]
    fn protected_set_holds_generation_context_and_derivation() {
        assert!(is_protected("waybill:generation-context"));
        assert!(is_protected("waybill:derivation"));
        assert!(!is_protected("waybill:graph-completeness"));
    }
}
