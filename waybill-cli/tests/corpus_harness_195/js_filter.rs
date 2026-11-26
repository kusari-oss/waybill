//! Single-ecosystem golden filter — issue #760 option B, feature 675 FR-008.
//!
//! #1677 generalised it from `pkg:npm/*` to any PURL prefix: the hpack
//! targets' goldens are filtered to `pkg:hackage/*`, because most of their
//! components (ogmios's npm clients, hs-temporal-sdk's Rust crates) say
//! nothing about the Haskell readers they are there to measure. The JS
//! wrappers below keep `pants-example-javascript` byte-identical.
//!
//! Filters emitted SBOMs down to the `pkg:npm/*` surface only, before
//! layer 2 byte-identity comparison. Applied per-target (dispatched by
//! target name in `layer2_golden::compare_golden`) so that the six
//! pre-675 corpus targets remain byte-identical to their pre-feature
//! output.
//!
//! Contract at `specs/675-pants-js-corpus/contracts/js-golden-filter.md`.
//!
//! The pants-example-javascript corpus target's full CDX is ~570 KB
//! (302 pkg:npm/* components + a mix of doc-scope annotations), which
//! violates SC-004's 500 KB combined-goldens budget. Filtering to the
//! JS surface only lands under 200 KB across all three formats.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::HashSet;

/// Filter a CDX 1.6 JSON document to the `pkg:npm/*` surface only.
///
/// Retains: envelope fields, `.metadata`, `.components[]` entries with
/// PURL prefix `pkg:npm/`, `.dependencies[]` entries whose `.ref` is a
/// retained PURL **or the primary component's bom-ref**, with each
/// retained entry's `.dependsOn[]` filtered to only npm PURLs.
///
/// Idempotent: applying twice yields byte-identical output.
pub fn filter_cdx_to_prefix(v: &mut serde_json::Value, prefix: &str) {
    let Some(obj) = v.as_object_mut() else { return };

    // The primary component's bom-ref, retained unconditionally below.
    //
    // Its ref is the operator-supplied root identity (`<name>@<version>`,
    // or a `pkg:generic/*` PURL) — never an npm PURL, so the npm-prefix
    // test below drops it, and with it the one edge that says what the
    // project itself depends on. That is what made issue #881 look like a
    // CycloneDX emitter defect: the emitter had the edge all along
    // (measured: 1 root out-edge in all three formats on this target),
    // the CDX filter removed it, the SPDX 2.3 filter kept its equivalent
    // via `always_keep`, and the two goldens disagreed for a reason that
    // had nothing to do with either emitter.
    let root_ref: Option<String> = obj
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("bom-ref"))
        .and_then(|r| r.as_str())
        .map(str::to_string);

    // Filter components — drop entries whose .purl doesn't start with pkg:npm/
    // (or is missing).
    if let Some(components) = obj.get_mut("components").and_then(|c| c.as_array_mut()) {
        components.retain(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with(prefix))
        });
    }

    // Filter dependencies — drop entries whose .ref isn't an npm PURL; for
    // retained entries, prune .dependsOn to npm PURLs only.
    if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_array_mut()) {
        deps.retain_mut(|dep| {
            let dep_ref = dep.get("ref").and_then(|r| r.as_str());
            let is_npm_ref = dep_ref.is_some_and(|r| r.starts_with(prefix));
            let is_root = dep_ref.is_some() && dep_ref == root_ref.as_deref();
            if !is_npm_ref && !is_root {
                return false;
            }
            if let Some(depends_on) = dep.get_mut("dependsOn").and_then(|d| d.as_array_mut()) {
                depends_on.retain(|t| {
                    t.as_str().is_some_and(|s| s.starts_with(prefix))
                });
            }
            true
        });
    }
}

/// Filter a SPDX 2.3 JSON document to the `pkg:npm/*` surface only.
///
/// Retains: envelope fields, `.creationInfo`, `.documentDescribes` (kept
/// unmodified — root document reference), `.packages[]` entries that
/// either (a) have any `.externalRefs[]` entry with a `pkg:npm/*`
/// referenceLocator, or (b) are the root document package (SPDXID matches
/// `SPDXRef-DOCUMENT` or appears in `documentDescribes`).
/// `.relationships[]` entries are retained only when both endpoint
/// SPDXIDs are in the retained set.
///
/// Retained packages keep ALL their externalRefs (not filtered per-entry).
pub fn filter_spdx23_to_prefix(v: &mut serde_json::Value, prefix: &str) {
    let Some(obj) = v.as_object_mut() else { return };

    // Compute the "always keep" set: documentDescribes references + the
    // canonical root SPDXID.
    let mut always_keep: HashSet<String> = HashSet::new();
    always_keep.insert("SPDXRef-DOCUMENT".to_string());
    if let Some(desc) = obj.get("documentDescribes").and_then(|d| d.as_array()) {
        for id in desc {
            if let Some(s) = id.as_str() {
                always_keep.insert(s.to_string());
            }
        }
    }

    // Build the kept-SPDXID set: always_keep ∪ any package with npm
    // externalRef.
    let mut kept_spdxids: HashSet<String> = always_keep.clone();
    if let Some(packages) = obj.get("packages").and_then(|p| p.as_array()) {
        for pkg in packages {
            let Some(spdxid) = pkg.get("SPDXID").and_then(|s| s.as_str()) else {
                continue;
            };
            if always_keep.contains(spdxid) {
                continue;
            }
            let has_npm = pkg
                .get("externalRefs")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter().any(|r| {
                        r.get("referenceLocator")
                            .and_then(|l| l.as_str())
                            .is_some_and(|l| l.starts_with(prefix))
                    })
                })
                .unwrap_or(false);
            if has_npm {
                kept_spdxids.insert(spdxid.to_string());
            }
        }
    }

    // Filter packages by the kept set.
    if let Some(packages) = obj.get_mut("packages").and_then(|p| p.as_array_mut()) {
        packages.retain(|pkg| {
            pkg.get("SPDXID")
                .and_then(|s| s.as_str())
                .is_some_and(|s| kept_spdxids.contains(s))
        });
    }

    // Filter relationships — both endpoints must be in the kept set.
    if let Some(rels) = obj.get_mut("relationships").and_then(|r| r.as_array_mut()) {
        rels.retain(|rel| {
            let a = rel.get("spdxElementId").and_then(|s| s.as_str());
            let b = rel.get("relatedSpdxElement").and_then(|s| s.as_str());
            matches!((a, b), (Some(a), Some(b)) if kept_spdxids.contains(a) && kept_spdxids.contains(b))
        });
    }
}

/// Filter a SPDX 3.0.1 JSON-LD document to the `pkg:npm/*` surface only.
///
/// Retains: `@context`; doc-scope typed nodes (`SpdxDocument`, `CreationInfo`,
/// `Person`, `Organization`, `Tool`); the document's `rootElement`; component
/// nodes carrying an npm PURL (via `software_packageUrl`, or an
/// `externalIdentifier` of type `packageUrl`/`purl`); `Relationship` and
/// `LifecycleScopedRelationship` nodes where both `from` and (filtered) `to`
/// reference retained spdxIds.
///
/// `Annotation` nodes are retained iff their `subject` is a retained spdxId.
///
/// `to` may be an array — its members are filtered to retained spdxIds
/// (drop the relationship if `to` becomes empty). `to` may also be a
/// scalar string — drop the relationship if it references a removed node.
pub fn filter_spdx3_to_prefix(v: &mut serde_json::Value, prefix: &str) {
    let Some(obj) = v.as_object_mut() else { return };

    // SPDX 3 uses "type" (per the JPEWdev validator we gate on); support
    // "@type" as a fallback for JSON-LD variants.
    fn node_type(n: &serde_json::Value) -> Option<&str> {
        n.get("type")
            .and_then(|t| t.as_str())
            .or_else(|| n.get("@type").and_then(|t| t.as_str()))
    }

    fn node_spdxid(n: &serde_json::Value) -> Option<&str> {
        n.get("spdxId").and_then(|s| s.as_str())
    }

    const DOC_SCOPE_TYPES: &[&str] = &[
        "SpdxDocument",
        "CreationInfo",
        "Person",
        "Organization",
        "Tool",
    ];

    let Some(graph) = obj.get_mut("@graph").and_then(|g| g.as_array_mut()) else {
        return;
    };

    // Pass 1: collect the set of retained spdxIds. Doc-scope nodes and the
    // document's own rootElement are always retained; component nodes are
    // retained iff they carry an npm PURL.
    //
    // The PURL test reads `software_packageUrl` and accepts either
    // `packageUrl` or `purl` as the externalIdentifierType. `packageUrl` is
    // the value waybill emits and the one the SPDX 3.0.1 vocabulary
    // defines (`ExternalIdentifierType/packageUrl`); the original filter
    // tested only for `purl`, which matched nothing, so every
    // software_Package in the document was dropped. The committed golden
    // was 14 nodes — 12 Organizations, a Tool and the SpdxDocument — for a
    // target whose whole purpose is to regression-lock 304 npm packages.
    // It asserted nothing, and satisfied the size budget by asserting
    // nothing (SC-004 recorded "SPDX 3 ~4 KB" against ~550 KB and ~976 KB
    // for the other two formats, a 250x gap that was measured and not
    // questioned). The unit test below could not catch it: its fixture was
    // hand-built with the same wrong value the filter looked for.
    let mut kept_ids: HashSet<String> = HashSet::new();
    for node in graph.iter() {
        if node_type(node) == Some("SpdxDocument") {
            for key in ["rootElement", "software_rootElement"] {
                for id in node.get(key).and_then(|r| r.as_array()).into_iter().flatten() {
                    if let Some(s) = id.as_str() {
                        kept_ids.insert(s.to_string());
                    }
                }
            }
        }
    }
    for node in graph.iter() {
        let Some(ty) = node_type(node) else { continue };
        let Some(id) = node_spdxid(node) else { continue };
        if DOC_SCOPE_TYPES.contains(&ty) {
            kept_ids.insert(id.to_string());
            continue;
        }
        // Component-like nodes: software_Package, software_File, software_Sbom
        let has_npm_purl = node
            .get("software_packageUrl")
            .and_then(|u| u.as_str())
            .is_some_and(|u| u.starts_with(prefix))
            || node
                .get("externalIdentifier")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter().any(|r| {
                        let ty_is_purl = r
                            .get("externalIdentifierType")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t == "packageUrl" || t == "purl");
                        let ident_is_npm = r
                            .get("identifier")
                            .and_then(|i| i.as_str())
                            .is_some_and(|i| i.starts_with(prefix));
                        ty_is_purl && ident_is_npm
                    })
                })
                .unwrap_or(false);
        if has_npm_purl {
            kept_ids.insert(id.to_string());
        }
    }

    // Pass 2: filter. Component / file / SBOM nodes: retain iff in kept_ids.
    // Relationship nodes: retain iff both endpoints are in kept_ids
    // (with `to` array pruning).
    graph.retain_mut(|node| {
        let Some(ty) = node_type(node).map(str::to_string) else {
            return true; // untyped nodes pass through (defensive)
        };
        // Both spellings are edges. waybill emits `LifecycleScopedRelationship`
        // for every scoped dependency edge — 640 of the 641 on this target —
        // and the original filter recognised only `Relationship`, so the
        // scoped edges fell through to the non-relationship branch below and
        // were dropped for not being in the kept set.
        if ty == "Relationship" || ty == "LifecycleScopedRelationship" {
            let from_ok = node
                .get("from")
                .and_then(|f| f.as_str())
                .is_some_and(|f| kept_ids.contains(f));
            if !from_ok {
                return false;
            }
            // .to may be a string or an array.
            let to_ref = node.get_mut("to");
            match to_ref {
                Some(serde_json::Value::String(s)) => {
                    return kept_ids.contains(s.as_str());
                }
                Some(serde_json::Value::Array(arr)) => {
                    arr.retain(|item| {
                        item.as_str().is_some_and(|s| kept_ids.contains(s))
                    });
                    return !arr.is_empty();
                }
                _ => return false,
            }
        }
        // Annotations: retain iff their subject is kept, as relationships
        // are retained by their endpoints. Without this every annotation
        // was dropped, so this target's SPDX 3 golden locked none of them
        // while the CDX and SPDX 2.3 goldens, which carry annotations inside
        // the component, locked all of them (#1150).
        if ty == "Annotation" {
            return node
                .get("subject")
                .and_then(|s| s.as_str())
                .is_some_and(|s| kept_ids.contains(s));
        }
        // Non-relationship: retain iff in kept set (which includes both
        // doc-scope and component nodes).
        node_spdxid(node)
            .map(|id| kept_ids.contains(id))
            .unwrap_or(false)
    });
}

/// The JS surface (feature 675), for `pants-example-javascript`.
pub fn filter_cdx_to_js(v: &mut serde_json::Value) {
    filter_cdx_to_prefix(v, "pkg:npm/");
}

/// The JS surface (feature 675), for `pants-example-javascript`.
pub fn filter_spdx23_to_js(v: &mut serde_json::Value) {
    filter_spdx23_to_prefix(v, "pkg:npm/");
}

/// The JS surface (feature 675), for `pants-example-javascript`.
pub fn filter_spdx3_to_js(v: &mut serde_json::Value) {
    filter_spdx3_to_prefix(v, "pkg:npm/");
}

/// #1677: the Haskell surface, `pkg:hackage/*`, for the hpack targets. The
/// document-scope records (C200 `waybill:hpack-read-outcome` among them)
/// stay: CycloneDX `metadata` is never filtered, SPDX 2.3 keeps document
/// fields, and SPDX 3 keeps annotations whose subject is the document.
pub const HASKELL_PREFIX: &str = "pkg:hackage/";

// ------------------------------------------------------------------
// Unit tests — per contracts/js-golden-filter.md testing section
// ------------------------------------------------------------------
