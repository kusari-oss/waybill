//! Layer 2 — full-SBOM byte-identity golden diff. Research §R5.
//!
//! Reuses the masking helpers from the existing `cdx_regression.rs` /
//! `spdx_regression.rs` / `spdx3_regression.rs` pattern (workspace-
//! path rewrite, HOME isolation, hash normalization, timestamp
//! masking, serial-number masking). When `WAYBILL_UPDATE_PUBLIC_
//! CORPUS_GOLDENS=1` is set, comparison is replaced with a golden
//! file write.

use std::path::PathBuf;

use super::harness::{AssertionFailure, EmittedSboms, FailureFormat, update_goldens_gate};
use super::js_filter;

/// Fixture root under the workspace: `waybill-cli/tests/fixtures/public_corpus/`.
fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("public_corpus")
}

/// Golden file path for a given target + format.
fn golden_path(target: &str, format: FailureFormat) -> PathBuf {
    let filename = match format {
        FailureFormat::Cdx => "cdx.json",
        FailureFormat::Spdx23 => "spdx-2.3.json",
        FailureFormat::Spdx3 => "spdx-3.json",
        FailureFormat::All => unreachable!("Layer 2 is per-format"),
    };
    fixtures_root().join(target).join(filename)
}

/// Compares an emitted SBOM against its golden. On drift, writes an
/// `.actual.json` sibling next to the golden so `diff` is copy-
/// pasteable per contracts/corpus-harness.md.
///
/// Under `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1`, writes the actual
/// as the new golden (regen mode).
pub fn compare_golden(
    target: &str,
    format: FailureFormat,
    sboms: &EmittedSboms,
) -> Result<(), AssertionFailure> {
    let (actual_path, actual_value) = match format {
        FailureFormat::Cdx => (&sboms.paths.cdx, &sboms.cdx),
        FailureFormat::Spdx23 => (&sboms.paths.spdx_2_3, &sboms.spdx_2_3),
        FailureFormat::Spdx3 => (&sboms.paths.spdx_3, &sboms.spdx_3),
        FailureFormat::All => unreachable!(),
    };
    let mut masked = mask_nondeterministic(actual_value);
    // Feature 675 — per-target JS-only filter for pants-example-javascript
    // per FR-008 clarification (Session 2026-09-02). Every other target
    // stays byte-identical to pre-675 output.
    if target == "pants-example-javascript" {
        match format {
            FailureFormat::Cdx => js_filter::filter_cdx_to_js(&mut masked),
            FailureFormat::Spdx23 => js_filter::filter_spdx23_to_js(&mut masked),
            FailureFormat::Spdx3 => js_filter::filter_spdx3_to_js(&mut masked),
            FailureFormat::All => unreachable!("layer 2 is per-format"),
        }
    }
    let masked_bytes = serde_json::to_vec_pretty(&masked).expect("serialize masked");
    let golden = golden_path(target, match format {
        FailureFormat::Cdx => FailureFormat::Cdx,
        FailureFormat::Spdx23 => FailureFormat::Spdx23,
        FailureFormat::Spdx3 => FailureFormat::Spdx3,
        FailureFormat::All => unreachable!(),
    });

    if update_goldens_gate() || !golden.exists() {
        if let Some(parent) = golden.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&golden, &masked_bytes).unwrap_or_else(|e| {
            panic!("m195 T014: failed to write golden {}: {e}", golden.display())
        });
        return Ok(());
    }
    let golden_bytes = std::fs::read(&golden).unwrap_or_else(|e| {
        panic!("m195 T014: failed to read golden {}: {e}", golden.display())
    });
    if golden_bytes == masked_bytes {
        return Ok(());
    }
    // Drift — write sibling `.actual.json` for copy-paste diffing.
    let actual_sibling = actual_path.with_extension(format!(
        "{}.actual",
        actual_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("json")
    ));
    std::fs::write(&actual_sibling, &masked_bytes).ok();
    let fmt_kind = match format {
        FailureFormat::Cdx => FailureFormat::Cdx,
        FailureFormat::Spdx23 => FailureFormat::Spdx23,
        FailureFormat::Spdx3 => FailureFormat::Spdx3,
        FailureFormat::All => unreachable!(),
    };
    Err(AssertionFailure {
        invariant_name: "layer2-golden-drift",
        format: fmt_kind,
        observed: format!("emitted (masked): {}", actual_sibling.display()),
        expected: format!("golden: {}", golden.display()),
        suggested_action: "run `diff <golden> <actual>` to inspect drift; if drift is intended, regen via WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1",
    })
}

/// Structural mask of non-deterministic fields per memory
/// `feedback_cross_host_goldens`: masks known volatile fields to
/// stable placeholders so byte-identity compares across hosts, dates,
/// and workspace paths.
fn mask_nondeterministic(v: &serde_json::Value) -> serde_json::Value {
    let mut cloned = v.clone();
    walk_mask(&mut cloned);
    cloned
}

fn walk_mask(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            let volatile_keys: &[&str] = &[
                "serialNumber",
                "timestamp",
                "created",
                "createdAt",
                "creationInfo",
                "documentNamespace",
                "creators",
                // SPDX 2.3 per-annotation timestamp — rotates per scan
                // regardless of pinned input (Utc::now() at emit time).
                "annotationDate",
            ];
            for k in volatile_keys {
                if map.contains_key(*k) {
                    map.insert((*k).to_string(), serde_json::Value::String("<masked>".to_string()));
                }
            }
            // m196: mask SHA256 / MD5 content hashes embedded inside
            // annotation `statement:` / `comment:` / `value:` JSON-in-
            // string values (chiefly the `evidence.occurrences[]` shape
            // in image-tier scans). These hashes are (a) noisy in golden
            // diffs — they rotate with any upstream re-publish of the
            // pinned image, adding drift with no regression-detection
            // signal, and (b) false-positive fodder for secret-scanners
            // (Kusari Inspector flagged `/etc/protocols` SHA256 as a
            // ProtocolsIO API key on PR #576). Masking eliminates both
            // classes of noise while preserving the shape and file paths
            // that make the annotation useful for regression detection.
            for key in ["statement", "comment", "value", "additionalContext"] {
                if let Some(v) = map.get_mut(key) {
                    if let Some(s) = v.as_str() {
                        let masked = mask_content_hashes_in_string(s);
                        if masked != s {
                            *v = serde_json::Value::String(masked);
                        }
                    }
                }
            }
            for (_, child) in map.iter_mut() {
                walk_mask(child);
            }
        }
        serde_json::Value::Array(arr) => {
            for child in arr.iter_mut() {
                walk_mask(child);
            }
        }
        // Issue #865: mask the document IRI wherever it appears, at the
        // string leaf, so this does not depend on enumerating the keys
        // that happen to carry one. The previous version keyed on
        // `spdxId` alone and left every relationship endpoint, annotation
        // subject and rootElement holding the real hash.
        serde_json::Value::String(s) if s.contains("/spdx3/doc-") => {
            *v = serde_json::Value::String(mask_doc_prefix(s));
        }
        _ => {}
    }
}

/// Replace 64-hex sha256 and 32-hex md5 values inside JSON-in-string
/// annotation payloads with a stable placeholder. Preserves the
/// enclosing structure (field names, paths, JSON braces) — only the
/// hex payload itself rotates.
fn mask_content_hashes_in_string(s: &str) -> String {
    // Only bother if the string plausibly contains `sha256` or `md5`
    // as a JSON field. Cheap prefilter avoids regex work on 99% of
    // annotations.
    if !s.contains("sha256") && !s.contains("md5") {
        return s.to_string();
    }
    // Field-scoped mask: `"sha256":"<64-hex>"` → `"sha256":"<masked-sha256>"`
    // (same for md5). Handles both inside JSON-in-string annotation
    // payloads and top-level JSON fields when the walker recurses.
    use std::sync::OnceLock;
    static SHA256_RE: OnceLock<regex::Regex> = OnceLock::new();
    static MD5_RE: OnceLock<regex::Regex> = OnceLock::new();
    let sha256_re = SHA256_RE.get_or_init(|| {
        regex::Regex::new(r#""sha256":"[0-9a-fA-F]{64}""#).expect("valid regex")
    });
    let md5_re = MD5_RE.get_or_init(|| {
        regex::Regex::new(r#""md5":"[0-9a-fA-F]{32}""#).expect("valid regex")
    });
    let a = sha256_re.replace_all(s, r#""sha256":"<masked-sha256>""#);
    let b = md5_re.replace_all(&a, r#""md5":"<masked-md5>""#);
    b.into_owned()
}

/// Replace every `/spdx3/doc-<opaque>` segment with
/// `/spdx3/doc-<masked>` so a stored golden survives per-scan
/// document-ID rotation.
///
/// Anchored on the full `/spdx3/doc-` namespace rather than a bare
/// `/doc-`: image scans contain real filesystem paths such as
/// `/usr/share/doc-base/findutils.findutils`, and a looser match
/// rewrites those into the golden as corrupted data. Caught by the
/// regeneration diff on `image-postgres16` — CDX and SPDX 2.3 moved
/// when only SPDX 3 should have.
///
/// Issue #865: this used to mask only the FIRST occurrence, and was only
/// ever called on values under the `spdxId` key. Relationship endpoints
/// (`from`, `to`), annotation `subject`s, `rootElement` and `suppliedBy`
/// all carry the same IRI and were left holding the real hash — so a
/// stored SPDX 3 golden referenced identifiers that appeared nowhere in
/// its own document, and any structural check on one was meaningless.
/// It also inflated every SPDX 3 diff, because an unmasked
/// content-addressed hash cascades on any content change.
fn mask_doc_prefix(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find("/spdx3/doc-") {
        out.push_str(&rest[..idx]);
        out.push_str("/spdx3/doc-<masked>");
        let after = &rest[idx + 11..];
        // The opaque segment runs to the next `/`, or to a character
        // that cannot appear in it (quote, whitespace) when the IRI is
        // embedded in a larger string such as a JSON-in-string value.
        match after.find(|c: char| c == '/' || c == '"' || c.is_whitespace()) {
            Some(end) => rest = &after[end..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m865_masking_tests {
    use super::*;
    use serde_json::json;

    /// The defect: masking keyed on `spdxId`, so every other IRI-bearing
    /// field kept the real document hash and the stored golden referenced
    /// identifiers absent from its own document.
    #[test]
    fn every_iri_bearing_field_is_masked_not_just_spdx_id() {
        let doc = json!({
            "@graph": [
                { "type": "software_Package", "spdxId": "https://w.dev/spdx3/doc-ABC/pkg-1" },
                { "type": "Relationship",
                  "spdxId": "https://w.dev/spdx3/doc-ABC/rel-1",
                  "from": "https://w.dev/spdx3/doc-ABC/pkg-1",
                  "to": ["https://w.dev/spdx3/doc-ABC/pkg-2"] },
                { "type": "Annotation",
                  "spdxId": "https://w.dev/spdx3/doc-ABC/anno-1",
                  "subject": "https://w.dev/spdx3/doc-ABC/pkg-1" },
                { "type": "SpdxDocument",
                  "spdxId": "https://w.dev/spdx3/doc-ABC",
                  "rootElement": ["https://w.dev/spdx3/doc-ABC/pkg-1"] }
            ]
        });
        let masked = mask_nondeterministic(&doc);
        let text = serde_json::to_string(&masked).unwrap();
        assert!(
            !text.contains("doc-ABC"),
            "no field may retain the real document id; got {text}",
        );
        assert!(text.contains("doc-<masked>"), "the shape must be preserved");
    }

    /// The property the goldens were violating: after masking, every
    /// relationship endpoint must still resolve to an element of the
    /// same document. Without this, no structural check on a stored
    /// golden means anything.
    #[test]
    fn masked_document_stays_internally_coherent() {
        let doc = json!({
            "@graph": [
                { "type": "software_Package", "spdxId": "https://w.dev/spdx3/doc-XYZ/pkg-1" },
                { "type": "software_Package", "spdxId": "https://w.dev/spdx3/doc-XYZ/pkg-2" },
                { "type": "Relationship",
                  "spdxId": "https://w.dev/spdx3/doc-XYZ/rel-1",
                  "from": "https://w.dev/spdx3/doc-XYZ/pkg-1",
                  "to": ["https://w.dev/spdx3/doc-XYZ/pkg-2"] }
            ]
        });
        let masked = mask_nondeterministic(&doc);
        let graph = masked["@graph"].as_array().unwrap();
        let ids: std::collections::BTreeSet<&str> = graph
            .iter()
            .filter_map(|e| e["spdxId"].as_str())
            .collect();
        for e in graph.iter().filter(|e| e["type"] == "Relationship") {
            let from = e["from"].as_str().unwrap();
            assert!(ids.contains(from), "dangling `from` after masking: {from}");
            for t in e["to"].as_array().unwrap() {
                let t = t.as_str().unwrap();
                assert!(ids.contains(t), "dangling `to` after masking: {t}");
            }
        }
    }

    /// The helper masked only the first occurrence, which silently left
    /// later ones intact inside JSON-in-string values.
    #[test]
    fn every_occurrence_in_one_string_is_masked() {
        let s = "a https://w.dev/spdx3/doc-AAA/pkg-1 b https://w.dev/spdx3/doc-AAA/pkg-2 c";
        let out = mask_doc_prefix(s);
        assert!(!out.contains("doc-AAA"), "got {out}");
        assert_eq!(out.matches("doc-<masked>").count(), 2);
    }

    /// A bare document IRI with no trailing segment must still mask.
    #[test]
    fn trailing_document_iri_is_masked() {
        assert_eq!(
            mask_doc_prefix("https://w.dev/spdx3/doc-ZZZ"),
            "https://w.dev/spdx3/doc-<masked>",
        );
    }

    /// A real filesystem path containing `doc-` must survive intact.
    /// The first version of this fix matched a bare `/doc-` and rewrote
    /// `/usr/share/doc-base/...` inside the image-postgres16 golden,
    /// corrupting real data. The regeneration diff caught it: CDX and
    /// SPDX 2.3 moved when a masking-only change should have touched
    /// SPDX 3 alone.
    #[test]
    fn real_paths_containing_doc_are_not_masked() {
        for p in [
            "/usr/share/doc-base/findutils.findutils",
            "/usr/share/doc-base/base-passwd.users-and-groups",
            "/etc/doc-something",
        ] {
            assert_eq!(mask_doc_prefix(p), p, "must not rewrite the real path {p}");
        }
    }

    /// Strings with no document IRI must pass through untouched — the
    /// leaf rule runs on every string in the document.
    #[test]
    fn unrelated_strings_are_untouched() {
        for s in ["pkg:cargo/serde@1.0.0", "MIT", "", "no slashes here"] {
            assert_eq!(mask_doc_prefix(s), s, "must not alter {s:?}");
        }
    }
}
