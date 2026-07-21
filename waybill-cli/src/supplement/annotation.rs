// Milestone 119 — annotation stamping helpers for the three new
// waybill:* keys this feature introduces:
//
// - `waybill:source-tier = "declared"` (per-component, value extension
//   on the existing C5 key — supplement-only entries)
// - `waybill:assertion-conflict` (per-component, REPEATABLE conflicts
//   stored as a single JSON-array property — see annotation-shape.md
//   § "Cardinality + storage shape")
// - `waybill:supplement-cdx` (document-scope provenance — emitted via
//   `metadata.rs`, not via these helpers)

use std::collections::BTreeMap;

use super::conflict::ConflictRecord;

const SOURCE_TIER_KEY: &str = "waybill:source-tier";
const ASSERTION_CONFLICT_KEY: &str = "waybill:assertion-conflict";

/// Stamp `waybill:source-tier = "declared"` on a supplement-introduced
/// component's `extra_annotations` bag. Overwrites any pre-existing
/// value (per emission gating in contracts/annotation-shape.md: solo
/// entries only — collisions keep the scanner's pre-existing tier and
/// this helper is NOT called for them).
pub(crate) fn stamp_source_tier_declared(
    extra_annotations: &mut BTreeMap<String, serde_json::Value>,
) {
    extra_annotations.insert(
        SOURCE_TIER_KEY.to_string(),
        serde_json::Value::String("declared".to_string()),
    );
}

/// Append a single conflict record to a component's
/// `waybill:assertion-conflict` JSON array. Multiple conflicts on the
/// same component accumulate into the same array value — the in-process
/// storage channel is `BTreeMap<String, serde_json::Value>` which holds
/// one value per key, so REPEATABILITY is implemented by storing a
/// `serde_json::Value::Array` and pushing new records into it.
///
/// Per contracts/annotation-shape.md § "Cardinality + storage shape",
/// the CDX wire shape is ONE `properties[]` entry whose `value` is the
/// JSON-encoded string of the array. The existing emitter at
/// `generate/cyclonedx/builder.rs` per-component-property serialization
/// path already handles `Value::Array` by JSON-encoding it.
pub(crate) fn stamp_assertion_conflict(
    extra_annotations: &mut BTreeMap<String, serde_json::Value>,
    conflict: &ConflictRecord,
) {
    let record_obj = conflict.as_json();
    match extra_annotations.get_mut(ASSERTION_CONFLICT_KEY) {
        Some(existing) => match existing {
            serde_json::Value::Array(arr) => arr.push(record_obj),
            other => {
                // Defensive: if a non-array value somehow ended up
                // under this key (shouldn't happen in practice),
                // promote it to a single-element array preserving the
                // prior content and then append.
                let prior = std::mem::replace(other, serde_json::Value::Null);
                *other = serde_json::Value::Array(vec![prior, record_obj]);
            }
        },
        None => {
            extra_annotations.insert(
                ASSERTION_CONFLICT_KEY.to_string(),
                serde_json::Value::Array(vec![record_obj]),
            );
        }
    }
}

/// Build the document-scope `waybill:supplement-cdx` property value
/// per FR-012 / Decision 6. The shape is `<path>@sha256:<hex>` where
/// `<path>` is the verbatim string the operator passed and `<hex>` is
/// the lowercase 64-char SHA-256 over the supplement file's raw bytes.
pub(crate) fn build_supplement_cdx_provenance_string(
    source_path: &str,
    source_sha256: &str,
) -> String {
    format!("{source_path}@sha256:{source_sha256}")
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::supplement::conflict::{ConflictField, ConflictRecord};
    use std::collections::BTreeMap;

    #[test]
    fn stamps_source_tier_declared() {
        let mut bag: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        stamp_source_tier_declared(&mut bag);
        assert_eq!(bag.get(SOURCE_TIER_KEY).unwrap().as_str(), Some("declared"));
    }

    #[test]
    fn single_assertion_conflict_creates_one_element_array() {
        let mut bag: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let record = ConflictRecord {
            field: ConflictField::Licenses,
            scanner_value: serde_json::json!([]),
            supplement_value: serde_json::json!([{"license":{"id":"Apache-2.0"}}]),
        };
        stamp_assertion_conflict(&mut bag, &record);
        let arr = bag.get(ASSERTION_CONFLICT_KEY).unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("field").unwrap().as_str(), Some("licenses"));
        assert_eq!(arr[0].get("winner").unwrap().as_str(), Some("supplement"));
        assert_eq!(
            arr[0].get("justification").unwrap().as_str(),
            Some("developer-metadata-override")
        );
    }

    #[test]
    fn two_assertion_conflicts_append_to_same_array() {
        let mut bag: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        stamp_assertion_conflict(
            &mut bag,
            &ConflictRecord {
                field: ConflictField::Licenses,
                scanner_value: serde_json::json!([]),
                supplement_value: serde_json::json!([{"license":{"id":"MIT"}}]),
            },
        );
        stamp_assertion_conflict(
            &mut bag,
            &ConflictRecord {
                field: ConflictField::Hashes,
                scanner_value: serde_json::json!([{"alg":"SHA-256","content":"deadbeef"}]),
                supplement_value: serde_json::json!([{"alg":"SHA-256","content":"cafebabe"}]),
            },
        );
        let arr = bag.get(ASSERTION_CONFLICT_KEY).unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].get("field").unwrap().as_str(), Some("licenses"));
        assert_eq!(arr[0].get("winner").unwrap().as_str(), Some("supplement"));
        assert_eq!(arr[1].get("field").unwrap().as_str(), Some("hashes"));
        assert_eq!(arr[1].get("winner").unwrap().as_str(), Some("scanner"));
    }

    #[test]
    fn provenance_string_shape() {
        let s = build_supplement_cdx_provenance_string(
            "supplement.cdx.json",
            "5e884898da28047151d0e56f8dc6292773603d0d6aabbdd62a11ef721d1542d8",
        );
        assert_eq!(
            s,
            "supplement.cdx.json@sha256:5e884898da28047151d0e56f8dc6292773603d0d6aabbdd62a11ef721d1542d8"
        );
    }
}
