//! Ecosystem-agnostic envelope construction for the milestone-134
//! `waybill:duplicate-purl-divergent` (per-component) and
//! `waybill:purl-collisions-detected` (document-scope) annotations.
//!
//! The actual per-format emission code lives in `cyclonedx/`,
//! `spdx/annotations.rs`, and `spdx/v3_annotations.rs`. Per-component
//! emission rides on the existing `extra_annotations` channel; the
//! document-scope summary is threaded through
//! `ScanArtifacts::collisions_summary`. The helpers here exist as
//! the schema-stable construction site so future
//! cross-CLI-process callers (parity extractors, fuzzers, the
//! supplement-merge code path) have a canonical entry point that
//! cannot drift from the [`DivergenceRecord`] and [`CollisionsSummary`]
//! definitions in `waybill_common::divergence`.
//!
//! Wire format specs:
//! - `specs/134-divergent-purl-detection/contracts/per-component-property.md`
//! - `specs/134-divergent-purl-detection/contracts/document-scope-annotation.md`

#![allow(dead_code)]

use waybill_common::divergence::{CollisionsSummary, DivergenceRecord};

/// Property/field identifier for the per-component property.
/// Lives on the deduped `pkg:<ecosystem>/<name>@<version>` component.
pub const PER_COMPONENT_PROPERTY_NAME: &str = "waybill:duplicate-purl-divergent";

/// Property/field identifier for the document-scope summary annotation.
/// Lives on the SBOM document (CDX `metadata.properties`, SPDX
/// top-level `annotations`, SPDX 3 `SpdxDocument.extension`).
pub const DOCUMENT_SCOPE_PROPERTY_NAME: &str = "waybill:purl-collisions-detected";

/// Build the per-component property's JSON value. CDX emitters
/// place this inside `components[].properties[].value` as a JSON-
/// encoded string; SPDX emitters wrap it inside the
/// `MikebomAnnotationCommentV1.value` slot. Either way, the
/// underlying JSON shape is identical — that's the byte-equivalence
/// guarantee per FR-003.
pub fn per_component_value(record: &DivergenceRecord) -> serde_json::Value {
    serde_json::to_value(record)
        .expect("DivergenceRecord serializes infallibly (concrete types, no Map<K, V>)")
}

/// Build the document-scope summary's JSON value. Same emission
/// rules as `per_component_value`.
pub fn document_scope_value(summary: &CollisionsSummary) -> serde_json::Value {
    serde_json::to_value(summary)
        .expect("CollisionsSummary serializes infallibly (concrete types, no Map<K, V>)")
}

/// CDX `properties[].value` is a string — JSON-encode the structured
/// payload so consumers `fromjson | ...` it back.
pub fn per_component_value_string(record: &DivergenceRecord) -> String {
    serde_json::to_string(record)
        .expect("DivergenceRecord serializes infallibly (concrete types, no Map<K, V>)")
}

/// Same as `per_component_value_string` for the document-scope
/// summary.
pub fn document_scope_value_string(summary: &CollisionsSummary) -> String {
    serde_json::to_string(summary)
        .expect("CollisionsSummary serializes infallibly (concrete types, no Map<K, V>)")
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::collections::BTreeMap;

    use waybill_common::divergence::{
        DivergenceReason, DivergenceRecord, DIVERGENCE_SCHEMA_VERSION,
    };
    use waybill_common::types::purl::Purl;

    use super::*;

    fn deps_differ_record() -> DivergenceRecord {
        let mut deps = BTreeMap::new();
        deps.insert(
            "crates/foo/Cargo.toml".to_string(),
            vec!["serde".to_string(), "tokio".to_string()],
        );
        deps.insert(
            "vendor/foo/Cargo.toml".to_string(),
            vec![
                "anyhow".to_string(),
                "serde".to_string(),
                "tokio".to_string(),
            ],
        );
        DivergenceRecord {
            v: DIVERGENCE_SCHEMA_VERSION,
            purl: Purl::new("pkg:cargo/foo@1.2.3").unwrap(),
            reason: DivergenceReason::DepsDiffer,
            paths: vec![
                "crates/foo/Cargo.toml".to_string(),
                "vendor/foo/Cargo.toml".to_string(),
            ],
            dep_sets_by_path: Some(deps),
            hashes_by_path: None,
        }
    }

    #[test]
    fn per_component_value_round_trips() {
        let record = deps_differ_record();
        let value = per_component_value(&record);
        let decoded: DivergenceRecord = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, record);
    }

    #[test]
    fn per_component_value_string_round_trips() {
        let record = deps_differ_record();
        let s = per_component_value_string(&record);
        let decoded: DivergenceRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(decoded, record);
    }

    #[test]
    fn reason_serialises_as_kebab_case() {
        let record = deps_differ_record();
        let s = per_component_value_string(&record);
        assert!(
            s.contains("\"reason\":\"deps-differ\""),
            "expected kebab-case reason: {s}"
        );
    }

    #[test]
    fn hashes_by_path_skipped_when_absent() {
        let record = deps_differ_record();
        let s = per_component_value_string(&record);
        assert!(
            !s.contains("hashes_by_path"),
            "absent hashes_by_path should not appear in serialized form: {s}"
        );
    }

    #[test]
    fn document_scope_value_round_trips() {
        let summary = CollisionsSummary::from_records(vec![deps_differ_record()]);
        let value = document_scope_value(&summary);
        let decoded: CollisionsSummary = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, summary);
    }
}
