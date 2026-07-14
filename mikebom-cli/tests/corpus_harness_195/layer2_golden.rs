//! Layer 2 — full-SBOM byte-identity golden diff. Research §R5.
//!
//! Reuses the masking helpers from the existing `cdx_regression.rs` /
//! `spdx_regression.rs` / `spdx3_regression.rs` pattern (workspace-
//! path rewrite, HOME isolation, hash normalization, timestamp
//! masking, serial-number masking). When `MIKEBOM_UPDATE_PUBLIC_
//! CORPUS_GOLDENS=1` is set, comparison is replaced with a golden
//! file write.

use std::path::PathBuf;

use super::harness::{AssertionFailure, EmittedSboms, FailureFormat, update_goldens_gate};

/// Fixture root under the workspace: `mikebom-cli/tests/fixtures/public_corpus/`.
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
/// Under `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1`, writes the actual
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
    let masked = mask_nondeterministic(actual_value);
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
        suggested_action: "run `diff <golden> <actual>` to inspect drift; if drift is intended, regen via MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1",
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
            // SPDX 3 wraps everything under "@graph" with per-element
            // spdxIds that embed content hashes; mask them structurally.
            if let Some(spdxid) = map.get_mut("spdxId") {
                if let Some(s) = spdxid.as_str() {
                    if s.contains("/doc-") {
                        // Mask the doc- prefix (a per-scan random-ish
                        // identifier) while preserving the shape.
                        let masked = mask_doc_prefix(s);
                        *spdxid = serde_json::Value::String(masked);
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
        _ => {}
    }
}

fn mask_doc_prefix(s: &str) -> String {
    // Replace `/doc-<opaque>/` with `/doc-<masked>/` to survive per-scan
    // doc-ID rotation.
    if let Some(idx) = s.find("/doc-") {
        let rest = &s[idx + 5..];
        if let Some(slash) = rest.find('/') {
            let (_opaque, tail) = rest.split_at(slash);
            format!("{}/doc-<masked>{}", &s[..idx], tail)
        } else {
            format!("{}/doc-<masked>", &s[..idx])
        }
    } else {
        s.to_string()
    }
}
