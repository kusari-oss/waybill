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
///
/// `waybill:retiered-from` is C243 (spec 1651): what a component's tier was
/// before `--retier` changed it. Removable, it would let a later edit erase
/// the only per-component trace that a tier is not the scan's own.
pub const PROTECTED_ANNOTATIONS: &[&str] =
    &["waybill:generation-context", "waybill:derivation", RETIERED_FROM];

/// C5, the per-component tier `--retier` relabels.
pub(crate) const SBOM_TIER: &str = "waybill:sbom-tier";

/// C243: a relabelled component's tier before its first relabel.
pub(crate) const RETIERED_FROM: &str = "waybill:retiered-from";

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
    /// Spec 1651: relabel `waybill:sbom-tier` from `from` to `to`. `from` is
    /// matched exactly and may be any value; `to` is one of the CISA six.
    Retier { from: String, to: crate::generate::lifecycle_phases::SbomType },
}

impl EditOp {
    /// The derivation record's category for this operation.
    pub fn category(&self) -> &'static str {
        match self {
            Self::DropComponents { .. } => "drop-components",
            Self::DropAnnotations { .. } => "drop-annotations",
            Self::Redact { class, .. } => class.category(),
            Self::Retier { .. } => "retier",
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
    /// Spec 1651, for `retier` only: the mapping. Absent for every other
    /// category, so their records are byte-identical to before. Tier names
    /// are a closed technical vocabulary, not a value from the original, so
    /// recording them keeps C194's "never the values" rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditReport {
    pub ops: Vec<OpReport>,
    /// Spec 1651 (FR-006): a retier changed components, but the document's
    /// type did not match what its own tiers gave, so it was kept as an
    /// assertion (`--sbom-type`, or another tool's).
    pub document_type_kept: bool,
    /// Components whose dependency list changed (bridging or removal).
    pub dependency_lists_changed: usize,
    /// Vulnerability statements removed because every component they
    /// assessed was dropped.
    pub vulnerability_statements_removed: usize,
    /// The OpenVEX sidecars whose reference the edit removed (#1454), as the
    /// document named them.
    pub openvex_references_removed: Vec<String>,
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
    /// Vulnerability statements removed because every component they
    /// assessed was dropped (CycloneDX `vulnerabilities[]` entries, SPDX 3
    /// `security_*VulnAssessmentRelationship` elements).
    pub vulnerability_statements_removed: usize,
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
    /// Spec 1651: set a component's `waybill:sbom-tier` to `to`, returning
    /// the tier it had. Writes C243 with that tier unless the component
    /// already carries C243, whose first value is kept.
    fn set_tier(&mut self, id: &str, to: &str) -> Option<String>;
    /// Spec 1651 (FR-006): whether the document's type, in this format's
    /// carrier, equals the given aggregate (CycloneDX phases, SPDX 3 types).
    fn document_type_is(&self, phases: &[&str], spdx3_types: &[&str]) -> bool;
    /// Spec 1651 (FR-006): replace the document's type in this format's
    /// carriers.
    fn set_document_type(&mut self, phases: &[&str], spdx3_types: &[&str]);
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
    /// Remove the document's references to an OpenVEX sidecar, returning
    /// what they named (#1454). Only SPDX has one: CycloneDX carries its VEX
    /// in the document, where dropping a component prunes it.
    fn remove_openvex_references(&mut self) -> Vec<String> {
        Vec::new()
    }
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
    let mut renamed_components = false;
    // Spec 1651 (R2, R5): the input's tiers, before any operation, and the
    // components a `--retier` has already relabelled in this edit.
    let input_tiers: Vec<String> = adapter.components().into_iter().filter_map(|c| c.tier).collect();
    let mut relabelled: BTreeSet<String> = BTreeSet::new();
    let mut retiered = 0usize;

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
                report.vulnerability_statements_removed += outcome.vulnerability_statements_removed;
                dropped.extend(selected.iter().cloned());
                OpReport {
                    category: op.category().to_string(),
                    matched: selected.len(),
                    changed: outcome.removed,
                    from: None,
                    to: None,
                }
            }
            EditOp::DropAnnotations { namespace } => {
                let removed = adapter.remove_annotations(namespace);
                OpReport {
                    category: op.category().to_string(),
                    matched: removed,
                    changed: removed,
                    from: None,
                    to: None,
                }
            }
            EditOp::Redact { class, mode, pattern } => {
                if *mode == RedactMode::Pseudonymise && opts.redact_key.is_none() {
                    bail!("--redact {}:pseudonymise needs a key: pass --redact-key-file", class.as_str());
                }
                let (matched, changed) =
                    redactor.apply(adapter.as_mut(), *class, *mode, pattern.as_deref())?;
                renamed_components |=
                    changed > 0 && matches!(class, RedactClass::Names | RedactClass::Hosts);
                OpReport {
                    category: op.category().to_string(),
                    matched,
                    changed,
                    from: None,
                    to: None,
                }
            }
            // Spec 1651 (FR-001): each component at most once, from its tier
            // in the input, so `design=source` then `source=build` does not
            // carry an original `design` on to `build`.
            EditOp::Retier { from, to } => {
                let matching: Vec<String> = adapter
                    .components()
                    .into_iter()
                    .filter(|c| c.tier.as_deref() == Some(from.as_str()))
                    .map(|c| c.id)
                    .collect();
                let mut changed = 0usize;
                for id in &matching {
                    if relabelled.contains(id) {
                        continue;
                    }
                    if adapter.set_tier(id, to.as_str()).is_some() {
                        relabelled.insert(id.clone());
                        changed += 1;
                    }
                }
                retiered += changed;
                OpReport {
                    category: op.category().to_string(),
                    matched: matching.len(),
                    changed,
                    from: Some(from.clone()),
                    to: Some(to.as_str().to_string()),
                }
            }
        };
        report.ops.push(op_report);
    }

    // #1454 — an OpenVEX sidecar names components by PURL, and this command
    // edits one document, never the sidecar. Once components are dropped, or
    // a name or host they are identified by is replaced, the sidecar may
    // assess components the document no longer holds, so the reference goes:
    // kept, it would point a consumer at statements about them, and SPDX
    // 2.3's checksum would still verify. The CycloneDX side drops the same
    // statements from the document.
    if !dropped.is_empty() || renamed_components {
        report.openvex_references_removed = adapter.remove_openvex_references();
    }

    // Spec 1651 (FR-006): recompute the document's type from the output's
    // tiers, only when it equals what the input's own tiers give, which is
    // the only evidence that it was derived rather than asserted. A retier
    // never touches completeness (FR-007): it resolves nothing.
    if retiered > 0 {
        use crate::generate::lifecycle_phases::{aggregate_phases_from_tiers, aggregate_spdx3_sbom_types_from_tiers};
        let before_phases = aggregate_phases_from_tiers(input_tiers.iter().map(String::as_str), None);
        let before_types = aggregate_spdx3_sbom_types_from_tiers(input_tiers.iter().map(String::as_str), None);
        if adapter.document_type_is(&before_phases, &before_types) {
            let output_tiers: Vec<String> = adapter.components().into_iter().filter_map(|c| c.tier).collect();
            let phases = aggregate_phases_from_tiers(output_tiers.iter().map(String::as_str), None);
            let types = aggregate_spdx3_sbom_types_from_tiers(output_tiers.iter().map(String::as_str), None);
            adapter.set_document_type(&phases, &types);
        } else {
            report.document_type_kept = true;
        }
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
    for id in &dropped {
        if let Some(path) = find_string(adapter.doc(), id) {
            bail!("dropped component `{id}` is still referenced at {path}; nothing written");
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

/// The JSON Pointer of a string value or object key equal to `needle`: the
/// same matches the serialised-text search this replaced, with a location.
pub(crate) fn find_string(v: &Value, needle: &str) -> Option<String> {
    fn escape(token: &str) -> String {
        token.replace('~', "~0").replace('/', "~1")
    }
    match v {
        Value::String(s) if s == needle => Some(String::new()),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .find_map(|(i, x)| find_string(x, needle).map(|rest| format!("/{i}{rest}"))),
        Value::Object(m) => m.iter().find_map(|(k, x)| {
            if k == needle {
                return Some(format!("/{}", escape(k)));
            }
            find_string(x, needle).map(|rest| format!("/{}{rest}", escape(k)))
        }),
        _ => None,
    }
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

/// Spec 1651: the phases clause of waybill's document comment, as the
/// emitter writes it (`generate/spdx/document.rs`).
const PHASES_KEY: &str = "Observed lifecycle phases: ";

pub(crate) fn phases_text(phases: &[&str]) -> String {
    if phases.is_empty() {
        "no lifecycle phases observed".to_string()
    } else {
        phases.join(", ")
    }
}

/// The phases clause's text in a document comment, if it has one.
pub(crate) fn comment_phases(comment: &str) -> Option<&str> {
    let start = comment.find(PHASES_KEY)? + PHASES_KEY.len();
    let len = comment[start..].find('.')?;
    Some(&comment[start..start + len])
}

/// The comment with its phases clause replaced, if it has one.
pub(crate) fn with_comment_phases(comment: &str, phases: &[&str]) -> Option<String> {
    let start = comment.find(PHASES_KEY)? + PHASES_KEY.len();
    let len = comment[start..].find('.')?;
    Some(format!("{}{}{}", &comment[..start], phases_text(phases), &comment[start + len..]))
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

    /// Spec 1651 (R3): every existing category serialises exactly as before.
    #[test]
    fn s1651_op_report_without_a_mapping_is_unchanged() {
        let r = OpReport { category: "drop-components".into(), matched: 2, changed: 1, from: None, to: None };
        assert_eq!(serde_json::to_value(&r).unwrap(), json!({"category": "drop-components", "matched": 2, "changed": 1}));
    }

    /// Spec 1651 (R4): `set_tier` writes C243 once, with the first tier, in
    /// every format.
    #[test]
    fn s1651_set_tier_keeps_the_first_marker_in_every_format() {
        let env = |f: &str, v: &str| envelope(f, json!(v));
        let docs = [
            (Format::CycloneDx16, json!({"bomFormat": "CycloneDX", "specVersion": "1.6", "components": [
                {"bom-ref": "c", "name": "c", "properties": [{"name": SBOM_TIER, "value": "design"}]}]})),
            (Format::Spdx23, json!({"spdxVersion": "SPDX-2.3", "packages": [
                {"SPDXID": "c", "name": "c", "annotations": [{"annotationType": "OTHER", "comment": env(SBOM_TIER, "design")}]}]})),
            (Format::Spdx301, json!({"@graph": [
                {"type": "software_Package", "spdxId": "c", "name": "c"},
                {"type": "Annotation", "spdxId": "c/anno-tier", "subject": "c", "statement": env(SBOM_TIER, "design")}]})),
        ];
        for (format, doc) in docs {
            let mut a = adapter_for(format, doc);
            assert_eq!(a.set_tier("c", "source").as_deref(), Some("design"), "{format:?}");
            assert_eq!(a.set_tier("c", "build").as_deref(), Some("source"), "{format:?}");
            assert_eq!(a.annotation_values(Some("c"), SBOM_TIER), vec![json!("build")], "{format:?}");
            assert_eq!(a.annotation_values(Some("c"), RETIERED_FROM), vec![json!("design")], "{format:?}");
            assert_eq!(a.set_tier("missing", "build"), None, "{format:?}");
        }
    }

    #[test]
    fn s1651_comment_phases_round_trip() {
        let c = "Scope: manifest (declared transitives included). Observed lifecycle phases: design, pre-build. Per-component scope detail.";
        assert_eq!(comment_phases(c), Some("design, pre-build"));
        assert_eq!(comment_phases(&with_comment_phases(c, &["pre-build"]).unwrap()), Some("pre-build"));
        assert_eq!(comment_phases(&with_comment_phases(c, &[]).unwrap()), Some("no lifecycle phases observed"));
        assert_eq!(comment_phases("no phases clause"), None);
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

    fn cdx_doc(extra_meta_props: Value, vulnerabilities: Value) -> Vec<u8> {
        let mut props = vec![json!({"name": "waybill:generation-context", "value": "filesystem-scan"})];
        props.extend(extra_meta_props.as_array().cloned().unwrap_or_default());
        serde_json::to_vec(&json!({
            "bomFormat": "CycloneDX", "specVersion": "1.6", "version": 1,
            "metadata": {"component": {"bom-ref": "waybill-fixture-app", "name": "waybill-fixture-app", "type": "application"},
                         "properties": props},
            "components": [
                {"bom-ref": "pkg:generic/waybill-fixture-lib@1", "name": "waybill-fixture-lib", "purl": "pkg:generic/waybill-fixture-lib@1"},
                {"bom-ref": "pkg:generic/waybill-fixture-tool@1", "name": "waybill-fixture-tool", "purl": "pkg:generic/waybill-fixture-tool@1"}
            ],
            "dependencies": [
                {"ref": "waybill-fixture-app", "dependsOn": ["pkg:generic/waybill-fixture-lib@1"]},
                {"ref": "pkg:generic/waybill-fixture-lib@1", "dependsOn": []},
                {"ref": "pkg:generic/waybill-fixture-tool@1", "dependsOn": []}
            ],
            "vulnerabilities": vulnerabilities
        }))
        .unwrap()
    }

    fn drop_tool() -> Vec<EditOp> {
        vec![EditOp::DropComponents { selector: "name=waybill-fixture-tool".parse().unwrap() }]
    }

    /// #1422: a document whose VEX names a dropped component is editable,
    /// and the report says how many statements went with it.
    #[test]
    fn vex_on_a_dropped_component_is_pruned_and_counted() {
        let input = cdx_doc(
            json!([]),
            json!([{"id": "CVE-2026-0001", "affects": [{"ref": "waybill-fixture-app"}],
                    "properties": [{"name": "waybill:vex-subcomponent", "value": "pkg:generic/waybill-fixture-tool@1"}]}]),
        );
        let out = run(&input, &drop_tool(), &EditOptions::default()).unwrap();
        assert_eq!(out.report.vulnerability_statements_removed, 1);
        let doc: Value = serde_json::from_slice(&out.bytes).unwrap();
        assert_eq!(doc["vulnerabilities"], json!([]));
    }

    fn spdx23_with_openvex() -> Vec<u8> {
        let tool_purl = "pkg:generic/waybill-fixture-tool@1";
        serde_json::to_vec(&json!({
            "spdxVersion": "SPDX-2.3", "SPDXID": "SPDXRef-DOCUMENT",
            "documentNamespace": "https://example.invalid/waybill-fixture-doc",
            "documentDescribes": ["SPDXRef-app"],
            "externalDocumentRefs": [{
                "externalDocumentId": "DocumentRef-OpenVEX",
                "spdxDocument": "waybill-fixture.openvex.json",
                "checksum": {"algorithm": "SHA256", "checksumValue": "0".repeat(64)}
            }],
            "annotations": [{"comment": envelope("waybill:generation-context", json!("filesystem-scan"))}],
            "packages": [
                {"SPDXID": "SPDXRef-app", "name": "waybill-fixture-app"},
                {"SPDXID": "SPDXRef-tool", "name": "waybill-fixture-tool",
                 "externalRefs": [{"referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": tool_purl}],
                 "annotations": [{"comment": envelope("waybill:source-files", json!("[\"src/waybill-fixture/a.rs\"]"))}]}
            ],
            "relationships": [
                {"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": "SPDXRef-app"},
                {"spdxElementId": "SPDXRef-app", "relationshipType": "DEPENDS_ON", "relatedSpdxElement": "SPDXRef-tool"}
            ]
        }))
        .unwrap()
    }

    /// #1454: which edits invalidate the OpenVEX sidecar reference. The
    /// sidecar names components by PURL, so dropping one, or replacing the
    /// name it is identified by, does; dropping annotations or redacting
    /// paths does not.
    #[test]
    fn the_openvex_reference_goes_when_components_are_dropped_or_renamed() {
        let names = EditOp::Redact { class: RedactClass::Names, mode: RedactMode::Remove, pattern: Some("waybill-fixture-tool".into()) };
        let paths = EditOp::Redact { class: RedactClass::Paths, mode: RedactMode::Remove, pattern: None };
        let annotations = EditOp::DropAnnotations { namespace: "waybill:source-".into() };
        let cases: [(&str, Vec<EditOp>, bool); 4] = [
            ("drop", drop_tool(), true),
            ("redact names", vec![names], true),
            ("redact paths", vec![paths], false),
            ("drop annotations", vec![annotations], false),
        ];
        for (label, ops, removes) in cases {
            let out = run(&spdx23_with_openvex(), &ops, &EditOptions::default()).unwrap();
            // CONTROL: the operation changed something, so a kept reference
            // is a decision and not an edit that did nothing.
            assert!(out.report.ops[0].changed > 0, "{label}: {:?}", out.report.ops);
            let doc: Value = serde_json::from_slice(&out.bytes).unwrap();
            let ids: Vec<&str> = doc["externalDocumentRefs"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|r| r["externalDocumentId"].as_str())
                .collect();
            assert_eq!(!ids.contains(&"DocumentRef-OpenVEX"), removes, "{label}: {ids:?}");
            let reported = if removes { vec!["waybill-fixture.openvex.json".to_string()] } else { Vec::new() };
            assert_eq!(out.report.openvex_references_removed, reported, "{label}");
            // The derivation's own reference is unaffected either way.
            assert!(ids.iter().any(|i| i.starts_with("DocumentRef-original")), "{label}: {ids:?}");
        }
    }

    /// #1454, SPDX 3: the sidecar is an `externalRef` on the `SpdxDocument`.
    #[test]
    fn the_spdx3_openvex_reference_goes_when_components_are_dropped() {
        let input = serde_json::to_vec(&json!({
            "@context": "https://spdx.org/rdf/3.0.1/spdx-context.jsonld",
            "@graph": [
                {"type": "SpdxDocument", "spdxId": "https://example.invalid/waybill-fixture-doc",
                 "rootElement": ["urn:app"],
                 "externalRef": [{"type": "ExternalRef", "externalRefType": "vulnerabilityExploitabilityAssessment",
                                  "contentType": "application/openvex+json", "locator": ["waybill-fixture.openvex.json"]}]},
                {"type": "software_Package", "spdxId": "urn:app", "name": "waybill-fixture-app"},
                {"type": "software_Package", "spdxId": "urn:tool", "name": "waybill-fixture-tool",
                 "software_packageUrl": "pkg:generic/waybill-fixture-tool@1"},
                {"type": "Relationship", "spdxId": "urn:rel", "from": "urn:app", "relationshipType": "dependsOn", "to": ["urn:tool"]},
                {"type": "Annotation", "spdxId": "urn:anno", "subject": "https://example.invalid/waybill-fixture-doc",
                 "statement": envelope("waybill:generation-context", json!("filesystem-scan"))}
            ]
        }))
        .unwrap();
        let out = run(&input, &drop_tool(), &EditOptions::default()).unwrap();
        // CONTROL: the tool really went.
        assert_eq!(out.report.ops[0].changed, 1, "{:?}", out.report.ops);
        assert_eq!(out.report.openvex_references_removed, vec!["waybill-fixture.openvex.json".to_string()]);
        let doc: Value = serde_json::from_slice(&out.bytes).unwrap();
        let spdx_doc = doc["@graph"].as_array().unwrap().iter().find(|e| e["type"] == "SpdxDocument").unwrap();
        assert!(spdx_doc.get("externalRef").is_none(), "{spdx_doc}");
    }

    /// #1422: the fail-closed check names what it found and where.
    #[test]
    fn a_leftover_reference_is_named_with_its_path() {
        let input = cdx_doc(
            json!([{"name": "waybill:fixture-pointer", "value": "pkg:generic/waybill-fixture-tool@1"}]),
            json!([]),
        );
        let err = run(&input, &drop_tool(), &EditOptions::default()).err().unwrap().to_string();
        assert!(err.contains("`pkg:generic/waybill-fixture-tool@1`"), "{err}");
        assert!(err.contains("/metadata/properties/1/value"), "{err}");
    }

    #[test]
    fn reference_paths_are_json_pointers_and_find_keys_too() {
        let v = json!({"a": [{"x": "id"}], "m": {"id": 1}, "s": "id/x~y"});
        assert_eq!(find_string(&v, "id").as_deref(), Some("/a/0/x"));
        assert_eq!(find_string(&json!({"m": {"id": 1}}), "id").as_deref(), Some("/m/id"));
        assert_eq!(find_string(&json!({"a/b~": "id"}), "id").as_deref(), Some("/a~1b~0"));
        assert_eq!(find_string(&v, "absent"), None);
    }

    #[test]
    fn protected_set_holds_generation_context_and_derivation() {
        assert!(is_protected("waybill:generation-context"));
        assert!(is_protected("waybill:derivation"));
        assert!(!is_protected("waybill:graph-completeness"));
    }
}
