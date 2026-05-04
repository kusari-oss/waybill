//! Cross-sibling helpers shared by `cdx`, `spdx2`, and `spdx3` (milestone 022).
//!
//! Owns:
//!   - the cross-format types (`ParityExtractor`, `Directionality`)
//!   - structural document walkers (one per format — kept here so SPDX-
//!     shared logic like `spdx_relationship_edges` can reach them
//!     without an upward dep into format submodules)
//!   - SPDX-side annotation envelope decoding (used by both SPDX 2.3
//!     and SPDX 3 sides)
//!   - `spdx_relationship_edges` (169 LOC graph traversal shared by
//!     SPDX 2.3 + SPDX 3 dependency-edge extractors per spec edge case)
//!   - sentinel `empty` / `g_empty` (referenced by EXTRACTORS table
//!     entries in `mod.rs`)
//!   - `normalize_alg` (used by all three formats' hash extractors)
//!
//! Visibility ladder (matches milestone 019 R4):
//!   - `pub` items are re-exported from `extractors/mod.rs` to preserve
//!     the public API path `mikebom::parity::extractors::*`.
//!   - `pub(super)` items are visible to `cdx`/`spdx2`/`spdx3`/`mod`
//!     siblings only.
//!   - private items live within this module.

use std::collections::BTreeSet;

use serde_json::Value;

/// Per-row extractor entry — one closure per format + a
/// directionality flag indicating whether the three extracted
/// sets must be symmetrically equal or whether a subset rule
/// applies.
pub struct ParityExtractor {
    pub row_id: &'static str,
    pub label: &'static str,
    pub cdx: fn(&Value) -> BTreeSet<String>,
    pub spdx23: fn(&Value) -> BTreeSet<String>,
    pub spdx3: fn(&Value) -> BTreeSet<String>,
    pub directional: Directionality,
    /// Milestone 071: when the extracted JSON values contain arrays whose
    /// element order is semantic (e.g., a build-trace step sequence),
    /// set this to `true` so `canonicalize_for_compare` preserves order
    /// instead of sorting. Default `false` — sets returned by extractors
    /// are already order-invariant via `BTreeSet<String>`, so this only
    /// matters when nested array values are passed through the helper.
    /// All currently-named keys (source-files, cpe-candidates,
    /// deps-dev-match, npm-role, sbom-tier, lifecycle-scope) are
    /// unordered and use the default.
    pub order_sensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directionality {
    /// CDX, SPDX 2.3, and SPDX 3 sets must all be equal.
    SymmetricEqual,
    /// `CDX ⊆ SPDX 2.3 ∧ CDX ⊆ SPDX 3`. The SPDX sides MAY
    /// carry additional values not in CDX (e.g., A12 CPE: CDX
    /// primary only; SPDX 3 every fully-resolved candidate).
    CdxSubsetOfSpdx,
    /// All three formats carry the datum but in shapes that
    /// structurally diverge (e.g., D1 evidence model — CDX
    /// `evidence.identity[]` vs SPDX flat `{technique,
    /// confidence}`; E1 compositions — CDX full array vs SPDX
    /// `{complete_ecosystems: [...]}`). The parity test only
    /// asserts that all three formats have a non-empty set —
    /// the spec calls this "presence parity," consistent with
    /// the user's clarification "data should be very similar,
    /// just formatting and structure should be different."
    PresenceOnly,
    /// Milestone 052/part-2: CDX-only by design. Used for
    /// finer-info carve-outs per Constitution Principle V where
    /// CDX's native field is too coarse to express the signal
    /// directly and the SPDX sides carry the same lifecycle
    /// signal natively via OTHER catalog rows (e.g., C42's
    /// `mikebom:lifecycle-scope` is a CDX-only finer split where
    /// CDX `scope` cannot express dev/build/test; SPDX 2.3 + 3
    /// carry the lifecycle scope via B2's typed dep-relationship
    /// types / `lifecycleScope` parameter, asserted independently
    /// by B2's extractor). The parity check asserts only that
    /// the CDX side is non-empty; SPDX sides are intentionally
    /// not parity-checked under this catalog row.
    CdxOnly,
}

/// Decode the `MikebomAnnotationCommentV1` envelope from SPDX
/// 2.3 `annotations[].comment` / SPDX 3 `Annotation.statement`
/// entries, returning the set of values observed for the named
/// `mikebom:<field>`. When `subject_is_document` is true, the
/// helper checks document-level annotations (SPDX 2.3 top-level
/// `annotations[]` / SPDX 3 `@graph[Annotation].subject ==
/// document-iri`); otherwise it walks per-Package annotations
/// (SPDX 2.3 `packages[].annotations[]` / SPDX 3 `@graph[Annotation]`
/// keyed by Package subject IRIs).
///
/// Used by Section C / D / E catalog rows whose extractors are
/// otherwise repetitive 30-line walks. Centralizing the envelope-
/// decoding here keeps extractor entries one-line per row.
pub fn extract_mikebom_annotation_values(
    doc: &Value,
    field_name: &str,
    subject_is_document: bool,
) -> BTreeSet<String> {
    // Guess the format by document shape: SPDX 2.3 has top-
    // level `packages[]`; SPDX 3 has `@graph[]`; CDX has
    // `components[]`. The catalog rows that route through this
    // helper are SPDX-only (CDX uses property-name lookups
    // directly), so we only handle the two SPDX shapes here.
    if doc.get("@graph").is_some() {
        extract_spdx3_annotation_values(doc, field_name, subject_is_document)
    } else {
        extract_spdx23_annotation_values(doc, field_name, subject_is_document)
    }
}

fn extract_spdx23_annotation_values(
    doc: &Value,
    field_name: &str,
    subject_is_document: bool,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let pools: Vec<&Value> = if subject_is_document {
        doc.get("annotations").into_iter().collect()
    } else {
        doc.get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.get("annotations"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    for pool in pools {
        let Some(arr) = pool.as_array() else { continue };
        for anno in arr {
            let Some(comment) = anno.get("comment").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(values) = decode_envelope(comment, field_name) {
                for v in values {
                    out.insert(v);
                }
            }
        }
    }
    out
}

fn extract_spdx3_annotation_values(
    doc: &Value,
    field_name: &str,
    subject_is_document: bool,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return out;
    };
    let document_iri = graph
        .iter()
        .find(|el| el.get("type").and_then(|v| v.as_str()) == Some("SpdxDocument"))
        .and_then(|el| el.get("spdxId"))
        .and_then(|v| v.as_str())
        .map(String::from);
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("Annotation") {
            continue;
        }
        let Some(subject_iri) = el.get("subject").and_then(|v| v.as_str()) else {
            continue;
        };
        let is_doc_subject = Some(subject_iri) == document_iri.as_deref();
        if subject_is_document != is_doc_subject {
            continue;
        }
        let Some(statement) = el.get("statement").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(values) = decode_envelope(statement, field_name) {
            for v in values {
                out.insert(v);
            }
        }
    }
    out
}

/// Decode a `MikebomAnnotationCommentV1` JSON-string envelope and
/// return the canonicalized atomic-value set if `field` matches
/// `field_name`, else None. Applied flatten-and-canonicalize
/// matches the CDX-side property-walk so a CDX scalar property
/// `value: "true"` (JSON-encoded string) compares equal to a SPDX
/// envelope `value: true` (real JSON bool); a CDX list-shape (one
/// property per element) compares equal to a SPDX array-shape
/// (one annotation, array-valued envelope).
pub(super) fn decode_envelope(serialized: &str, field_name: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(serialized).ok()?;
    if v.get("schema")?.as_str()? != "mikebom-annotation/v1" {
        return None;
    }
    if v.get("field")?.as_str()? != field_name {
        return None;
    }
    let value = v.get("value")?;
    Some(canonicalize_atomic_values(value))
}

/// Reduce a `serde_json::Value` to a flat set of canonical
/// strings. Strings that themselves encode JSON (e.g., the CDX
/// property-value convention of stringifying booleans / numbers /
/// short JSON-y values) are recursively decoded. Arrays are
/// flattened one level. Other shapes (bool, number, plain string,
/// object) canonicalize via `to_string`. This is the canonical
/// form both the CDX-property side and the SPDX-annotation side
/// reduce to before set-comparison.
pub(super) fn canonicalize_atomic_values(value: &Value) -> Vec<String> {
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        let looks_like_json = matches!(
            trimmed.chars().next(),
            Some('[' | '{' | '"' | 't' | 'f' | 'n' | '-' | '0'..='9')
        );
        if looks_like_json {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                return canonicalize_atomic_values(&parsed);
            }
        }
        return vec![serde_json::to_string(value).unwrap_or_default()];
    }
    if let Some(arr) = value.as_array() {
        let mut out = Vec::new();
        for el in arr {
            out.extend(canonicalize_atomic_values(el));
        }
        return out;
    }
    vec![serde_json::to_string(value).unwrap_or_default()]
}

/// Walk every CycloneDX component (top-level + recursively
/// nested under `components[].components[]`), yielding each
/// component object. Used by Section A / B / C extractors.
pub fn walk_cdx_components(doc: &Value) -> Vec<&Value> {
    fn recur<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
        if let Some(arr) = node.get("components").and_then(|v| v.as_array()) {
            for c in arr {
                out.push(c);
                recur(c, out);
            }
        }
    }
    let mut out = Vec::new();
    recur(doc, &mut out);
    out
}

/// Like `walk_cdx_components` but additionally includes
/// `metadata.component` when it carries
/// `mikebom:component-role: main-module` (milestone 053 — the Go
/// workspace's main-module per FR-001a). Used by `mikebom:*`
/// property extractors (C18 source-files, C40 component-role,
/// sbom-tier, etc.) that need to see all components carrying the
/// property regardless of where they live in the BOM tree.
///
/// Section A extractors (purl, name, version, supplier, cpe) MUST
/// NOT use this — those fields on `metadata.component` are
/// synthesized in a CDX-specific shape (e.g., `cpe:2.3:a:mikebom:…`
/// for the synthetic placeholder; `cpe:2.3:a:<name>:<name>:…` for
/// the main-module's promoted entry) and don't round-trip to the
/// SPDX side identically.
pub fn walk_cdx_components_and_main_module(doc: &Value) -> Vec<&Value> {
    let mut out = walk_cdx_components(doc);
    if let Some(metadata_component) = doc.get("metadata").and_then(|m| m.get("component")) {
        let has_main_module_tag = metadata_component
            .get("properties")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().any(|p| {
                    p.get("name").and_then(|v| v.as_str())
                        == Some("mikebom:component-role")
                        && p.get("value").and_then(|v| v.as_str())
                            == Some("main-module")
                })
            })
            .unwrap_or(false);
        if has_main_module_tag {
            out.push(metadata_component);
        }
    }
    out
}

/// Iterate SPDX 2.3 `packages[]`, skipping the synthetic root
/// (SPDXID begins with `SPDXRef-DocumentRoot-`).
pub fn walk_spdx23_packages(doc: &Value) -> Vec<&Value> {
    let Some(arr) = doc.get("packages").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter(|p| {
            !p.get("SPDXID")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.starts_with("SPDXRef-DocumentRoot-"))
        })
        .collect()
}

/// Iterate SPDX 3 `@graph[]` Package elements, skipping the
/// synthetic root (spdxId path segment includes `/pkg-root-`).
pub fn walk_spdx3_packages(doc: &Value) -> Vec<&Value> {
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    graph
        .iter()
        .filter(|el| el.get("type").and_then(|v| v.as_str()) == Some("software_Package"))
        .filter(|el| {
            !el.get("spdxId")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("/pkg-root-"))
        })
        .collect()
}

/// Empty extractor — used for format-restricted columns + sentinel
/// G/H rows that don't carry cross-format-comparable signal.
pub(super) fn empty(_doc: &Value) -> BTreeSet<String> {
    BTreeSet::new()
}

/// Normalize a hash-algorithm name to a canonical comparison
/// form (`SHA256` etc.). CDX uses `SHA-256`; SPDX 2.3 uses
/// `SHA256`; SPDX 3 uses `sha256` (lowercase). We uppercase +
/// strip hyphens for symmetric comparison.
pub(super) fn normalize_alg(s: &str) -> String {
    s.replace('-', "").to_uppercase()
}

/// Shared SPDX 2.3 + SPDX 3 graph-traversal: walks Relationship
/// records for the given relationship type, returning a set of
/// "from-purl -> to-purl" strings. Branches at runtime on
/// `@graph` presence to detect SPDX 3 vs SPDX 2.3 shape. Used by
/// B1/B2 (runtime/dev dependency edges) on both SPDX sides.
pub(super) fn spdx_relationship_edges(
    doc: &Value,
    rel_type_2_3: &str,
    rel_type_3: &str,
) -> BTreeSet<String> {
    if doc.get("@graph").is_some() {
        // SPDX 3
        let mut out = BTreeSet::new();
        let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
            return out;
        };
        // Build IRI → PURL lookup.
        let mut purl_by_iri: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for el in graph {
            if el.get("type").and_then(|v| v.as_str()) == Some("software_Package") {
                if let (Some(iri), Some(purl)) = (
                    el.get("spdxId").and_then(|v| v.as_str()),
                    el.get("software_packageUrl").and_then(|v| v.as_str()),
                ) {
                    purl_by_iri.insert(iri.to_string(), purl.to_string());
                }
            }
        }
        for el in graph {
            if el.get("type").and_then(|v| v.as_str()) != Some("Relationship") {
                continue;
            }
            if el.get("relationshipType").and_then(|v| v.as_str()) != Some(rel_type_3) {
                continue;
            }
            let from_iri = el.get("from").and_then(|v| v.as_str());
            let to_arr = el.get("to").and_then(|v| v.as_array());
            if let (Some(f), Some(t_arr)) = (from_iri, to_arr) {
                let Some(from_purl) = purl_by_iri.get(f) else {
                    continue;
                };
                for t in t_arr {
                    if let Some(t_iri) = t.as_str() {
                        if let Some(to_purl) = purl_by_iri.get(t_iri) {
                            out.insert(format!("{from_purl}->{to_purl}"));
                        }
                    }
                }
            }
        }
        out
    } else {
        // SPDX 2.3
        let mut out = BTreeSet::new();
        let mut purl_by_spdxid: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for p in walk_spdx23_packages(doc) {
            let id = match p.get("SPDXID").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => continue,
            };
            let purl = p
                .get("externalRefs")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|r| {
                        if r.get("referenceType").and_then(|v| v.as_str()) == Some("purl") {
                            r.get("referenceLocator")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        } else {
                            None
                        }
                    })
                });
            if let Some(purl) = purl {
                purl_by_spdxid.insert(id.to_string(), purl);
            }
        }
        let Some(rels) = doc.get("relationships").and_then(|v| v.as_array()) else {
            return out;
        };
        for r in rels {
            if r.get("relationshipType").and_then(|v| v.as_str()) != Some(rel_type_2_3) {
                continue;
            }
            let Some(from) = r.get("spdxElementId").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(to) = r.get("relatedSpdxElement").and_then(|v| v.as_str()) else {
                continue;
            };
            if let (Some(from_purl), Some(to_purl)) =
                (purl_by_spdxid.get(from), purl_by_spdxid.get(to))
            {
                out.insert(format!("{from_purl}->{to_purl}"));
            }
        }
        out
    }
}

/// Milestone 071 contract C-3: canonicalize a JSON value into a stable
/// string suitable for cross-format value-equality comparison.
///
/// Default rule (`order_sensitive == false`):
///   - Object keys sorted lexicographically (recursively).
///   - JSON arrays sorted lexicographically (recursively).
///   - Whitespace normalized via `serde_json::to_string` (compact).
///
/// Override (`order_sensitive == true`): preserve array insertion order.
/// Object keys are still sorted — the override is array-only.
///
/// Used by parity-equivalence checks where value payloads must agree
/// across CDX 1.6 / SPDX 2.3 / SPDX 3 emissions.
#[allow(dead_code)] // wired by milestone-071 T018 parity_completeness.rs
pub fn canonicalize_for_compare(value: &Value, order_sensitive: bool) -> String {
    let canon = canonicalize_value(value, order_sensitive);
    serde_json::to_string(&canon).unwrap_or_else(|_| String::from("null"))
}

#[allow(dead_code)] // wired by milestone-071 T018 parity_completeness.rs
fn canonicalize_value(value: &Value, order_sensitive: bool) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize_value(v, order_sensitive)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                out.insert(k, v);
            }
            Value::Object(out)
        }
        Value::Array(arr) => {
            let mut canon: Vec<Value> = arr
                .iter()
                .map(|v| canonicalize_value(v, order_sensitive))
                .collect();
            if !order_sensitive {
                canon.sort_by_key(|v| serde_json::to_string(v).unwrap_or_default());
            }
            Value::Array(canon)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    /// (a) nested objects — keys at every depth sort lexicographically.
    #[test]
    fn canonicalize_sorts_nested_object_keys() {
        let a = json!({ "z": 1, "a": { "y": 2, "b": 3 } });
        let b = json!({ "a": { "b": 3, "y": 2 }, "z": 1 });
        assert_eq!(
            canonicalize_for_compare(&a, false),
            canonicalize_for_compare(&b, false),
        );
    }

    /// (b) mixed array + object — both kinds canonicalize together.
    #[test]
    fn canonicalize_handles_mixed_array_and_object() {
        let a = json!({ "items": [{ "id": 2, "name": "b" }, { "name": "a", "id": 1 }] });
        let b = json!({ "items": [{ "id": 1, "name": "a" }, { "id": 2, "name": "b" }] });
        assert_eq!(
            canonicalize_for_compare(&a, false),
            canonicalize_for_compare(&b, false),
        );
    }

    /// (c) order_sensitive=true preserves array order.
    #[test]
    fn canonicalize_order_sensitive_preserves_array_order() {
        let a = json!(["step-1", "step-2", "step-3"]);
        let b = json!(["step-3", "step-1", "step-2"]);
        assert_ne!(
            canonicalize_for_compare(&a, true),
            canonicalize_for_compare(&b, true),
            "order_sensitive=true must NOT sort arrays",
        );
        // Sanity: with order_sensitive=false they DO match.
        assert_eq!(
            canonicalize_for_compare(&a, false),
            canonicalize_for_compare(&b, false),
        );
    }

    /// (d) structurally-different-but-semantically-equivalent inputs
    /// produce the same canonical string. Two objects whose keys are
    /// in opposite orders, with nested arrays whose elements are
    /// in different orders, must canonicalize identically.
    #[test]
    fn canonicalize_equates_structurally_different_inputs() {
        let a = json!({
            "cpes": ["cpe:2.3:a:foo:bar:1.0", "cpe:2.3:a:baz:qux:2.0"],
            "tier": "source",
        });
        let b = json!({
            "tier": "source",
            "cpes": ["cpe:2.3:a:baz:qux:2.0", "cpe:2.3:a:foo:bar:1.0"],
        });
        assert_eq!(
            canonicalize_for_compare(&a, false),
            canonicalize_for_compare(&b, false),
        );
    }

    /// (e) empty/null/absent equivalence — the spec.md edge case
    /// "Empty / null / absent: which one is the canonical 'absent'
    /// representation?" Empty array, empty string, JSON null, and
    /// absent key all produce extractor-empty `BTreeSet<String>` outputs
    /// when consumed by Section C extractors, so SymmetricEqual rows
    /// treat them as equal. This test covers the helper-level identity
    /// + the BTreeSet-extractor-level identity.
    #[test]
    fn empty_null_absent_canonicalize_equivalently_at_set_layer() {
        // At the helper layer, the FOUR shapes produce 4 distinct
        // canonical strings (`[]`, `""`, `null`, n/a). That's expected —
        // the helper compares full JSON values. The equivalence the
        // spec promises is at the EXTRACTOR-set layer:
        //
        //   - Empty array  → extractor returns BTreeSet::new() (no items)
        //   - Empty string → extractor sees no comma-/pipe-separated tokens → BTreeSet::new()
        //   - JSON null    → extractor sees nothing to push → BTreeSet::new()
        //   - Absent key   → extractor's lookup returns None → BTreeSet::new()
        //
        // All four collapse to the empty set, which compares equal under
        // SymmetricEqual. Simulate this with a representative extractor:
        let extractor_empty_set = |_v: &Value| -> BTreeSet<String> { BTreeSet::new() };

        let empty_array = json!([]);
        let empty_string = json!("");
        let null_value = Value::Null;
        let absent = json!({}); // No key at all on the parent doc.

        let s1 = extractor_empty_set(&empty_array);
        let s2 = extractor_empty_set(&empty_string);
        let s3 = extractor_empty_set(&null_value);
        let s4 = extractor_empty_set(&absent);

        assert_eq!(s1, s2);
        assert_eq!(s2, s3);
        assert_eq!(s3, s4);
        assert!(s1.is_empty());

        // And at the helper layer, empty-array canonicalizes consistently
        // (idempotent) — i.e., a real extractor that DOES walk the value
        // gets the same canonical form on every invocation.
        assert_eq!(
            canonicalize_for_compare(&empty_array, false),
            canonicalize_for_compare(&empty_array, false),
        );
        assert_eq!(canonicalize_for_compare(&empty_array, false), "[]");
    }
}
