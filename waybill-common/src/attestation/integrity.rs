use serde::{Deserialize, Serialize};

use crate::types::timestamp::Timestamp;

/// Diagnostic information about the fidelity of the trace capture.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TraceIntegrity {
    pub ring_buffer_overflows: u64,
    pub events_dropped: u64,
    pub uprobe_attach_failures: Vec<String>,
    pub kprobe_attach_failures: Vec<String>,
    pub partial_captures: Vec<PartialCapture>,
    pub bloom_filter_capacity: u64,
    pub bloom_filter_false_positive_rate: f64,
    /// Milestone 213 (issue #616) — sorted-deduplicated set of
    /// noise-filter category names that fired during the trace. Values
    /// drawn from the closed set `{"System", "UserCache", "Ephemeral",
    /// "CargoFingerprint"}` matching `FilterCategoryTag::name()`
    /// verbatim (see contracts/filter-category-tag.md).
    ///
    /// Field placement: LAST in the struct so pre-m213 JSON prefix is
    /// byte-identical. Deserialization is back-compat via
    /// `#[serde(default)]` — pre-m213 attestations round-trip with
    /// `filter_categories_applied = vec![]`.
    ///
    /// Empty state MUST serialize as `[]` per FR-009 (never `null`,
    /// never absent) — the field's presence is the operator-visible
    /// signal that the filter ran.
    #[serde(default)]
    pub filter_categories_applied: Vec<String>,
}

/// Record of an event that was only partially captured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartialCapture {
    pub event_type: String,
    pub reason: String,
    pub timestamp: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_integrity_serde_round_trip() {
        let integrity = TraceIntegrity {
            ring_buffer_overflows: 0,
            events_dropped: 2,
            uprobe_attach_failures: vec!["libssl.so:SSL_write".to_string()],
            kprobe_attach_failures: vec![],
            partial_captures: vec![PartialCapture {
                event_type: "tls_handshake".to_string(),
                reason: "buffer too small".to_string(),
                timestamp: Timestamp::now(),
            }],
            bloom_filter_capacity: 100_000,
            bloom_filter_false_positive_rate: 0.01,
            filter_categories_applied: vec![],
        };

        let json = serde_json::to_string(&integrity).expect("serialize integrity");
        let back: TraceIntegrity = serde_json::from_str(&json).expect("deserialize integrity");
        assert_eq!(integrity.events_dropped, back.events_dropped);
        assert_eq!(integrity.uprobe_attach_failures, back.uprobe_attach_failures);
    }

    /// Milestone 212 (issue #615) — wire-shape regression guard.
    ///
    /// Post-m212 `ring_buffer_overflows` carries real u64 drop counts
    /// (previously always `0`) and `kprobe_attach_failures[]` may
    /// carry counter-map names (e.g. `"file_event_drops"`) alongside
    /// real kprobe attach failure names per Q3. This test asserts:
    /// (a) the serialized JSON round-trips value-identically for BOTH
    ///     fields populated with realistic post-m212 values, AND
    /// (b) the deserialized struct is byte-equal to the input via
    ///     `serde_json::to_value` equality (JSON-value equivalence,
    ///     robust to serde version drift per research R4).
    #[test]
    fn trace_integrity_serde_populated_counter_and_attach_failures() {
        let integrity = TraceIntegrity {
            ring_buffer_overflows: 13636,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![
                "file_event_drops".to_string(),
                "vfs_open".to_string(),
            ],
            partial_captures: vec![],
            bloom_filter_capacity: 65536,
            bloom_filter_false_positive_rate: 0.01,
            filter_categories_applied: vec![],
        };

        let json = serde_json::to_string(&integrity).expect("serialize integrity");
        let back: TraceIntegrity =
            serde_json::from_str(&json).expect("deserialize integrity");
        assert_eq!(integrity.ring_buffer_overflows, back.ring_buffer_overflows);
        assert_eq!(integrity.kprobe_attach_failures, back.kprobe_attach_failures);
        assert_eq!(integrity, back);

        // Cross-check via serde_json::Value equality — catches
        // structural drift (field renames, type changes) that
        // struct-equality can't.
        let val = serde_json::to_value(&integrity).unwrap();
        let val_back = serde_json::to_value(&back).unwrap();
        assert_eq!(val, val_back);

        // Field-presence assertions on the emitted JSON — pinned so
        // consumer contracts (per contracts/counter-semantics.md) can
        // rely on stable field names.
        assert!(val.get("ring_buffer_overflows").unwrap().is_u64());
        assert_eq!(val["ring_buffer_overflows"].as_u64(), Some(13636));
        assert!(val.get("kprobe_attach_failures").unwrap().is_array());
    }

    /// Milestone 213 T016 (issue #616) — wire-shape round-trip test for
    /// the new `filter_categories_applied` field. Asserts:
    /// (a) populated field round-trips byte-identically alongside the
    ///     m212 counter fields (FR-006, FR-007)
    /// (b) empty state serializes as `[]` — never `null`, never absent
    ///     (FR-009 — the presence of an empty array is the operator-
    ///     visible signal that the filter ran but no category fired)
    /// (c) `serde_json::to_value` equality per R4 pattern (structural
    ///     drift resistance)
    /// (d) values pass through unmodified — no sort/dedup happens at
    ///     the wire-shape layer (that's the aggregator's job upstream)
    #[test]
    fn trace_integrity_serde_populated_filter_categories_applied() {
        let integrity = TraceIntegrity {
            ring_buffer_overflows: 8,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![],
            partial_captures: vec![],
            bloom_filter_capacity: 65536,
            bloom_filter_false_positive_rate: 0.01,
            filter_categories_applied: vec![
                "CargoFingerprint".to_string(),
                "Ephemeral".to_string(),
                "System".to_string(),
            ],
        };

        let json = serde_json::to_string(&integrity).expect("serialize integrity");
        let back: TraceIntegrity =
            serde_json::from_str(&json).expect("deserialize integrity");
        assert_eq!(integrity, back);
        assert_eq!(
            serde_json::to_value(&integrity).unwrap(),
            serde_json::to_value(&back).unwrap(),
        );

        // The field MUST appear as a JSON array in the emitted JSON so
        // downstream `jq` consumers can rely on it (per FR-009 the
        // field is always present, even when empty).
        let val = serde_json::to_value(&integrity).unwrap();
        assert!(val.get("filter_categories_applied").unwrap().is_array());
        assert_eq!(
            val["filter_categories_applied"].as_array().unwrap().len(),
            3
        );
    }

    /// Milestone 213 FR-009 — empty state MUST serialize as `[]` (never
    /// null, never absent). This is the operator-visible signal that
    /// the filter ran.
    #[test]
    fn trace_integrity_empty_filter_categories_applied_serializes_as_empty_array() {
        let integrity = TraceIntegrity::default();
        let val = serde_json::to_value(&integrity).unwrap();

        // Field MUST be present.
        assert!(
            val.get("filter_categories_applied").is_some(),
            "filter_categories_applied field MUST be present in the emitted JSON per FR-009"
        );
        // Field MUST be a JSON array.
        assert!(
            val["filter_categories_applied"].is_array(),
            "filter_categories_applied MUST be a JSON array, got: {:?}",
            val["filter_categories_applied"]
        );
        // Array MUST be empty (default value).
        assert_eq!(
            val["filter_categories_applied"].as_array().unwrap().len(),
            0
        );

        // Pre-m213 attestations (missing the field) MUST deserialize
        // successfully via serde(default) — back-compat contract.
        let pre_m213_json = r#"{
            "ring_buffer_overflows": 0,
            "events_dropped": 0,
            "uprobe_attach_failures": [],
            "kprobe_attach_failures": [],
            "partial_captures": [],
            "bloom_filter_capacity": 0,
            "bloom_filter_false_positive_rate": 0.0
        }"#;
        let back: TraceIntegrity =
            serde_json::from_str(pre_m213_json).expect("pre-m213 JSON must round-trip");
        assert!(back.filter_categories_applied.is_empty());
    }
}
