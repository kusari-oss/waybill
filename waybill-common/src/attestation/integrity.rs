use serde::{Deserialize, Serialize};

use crate::types::timestamp::Timestamp;

/// Diagnostic information about the fidelity of the trace capture.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TraceIntegrity {
    pub ring_buffer_overflows: u64,
    /// Not measured: no producer in this repository sets it to anything
    /// but `0` (see `trace::aggregator::TraceStats::events_dropped` and
    /// #618). A consumer judging capture fidelity should treat `0` here as
    /// absence of evidence, and read `ring_buffer_overflows` -- which is
    /// measured since m212 -- for the loss the kernel does report.
    pub events_dropped: u64,
    pub uprobe_attach_failures: Vec<String>,
    /// Capture interfaces that did not become usable on this host.
    ///
    /// The name says "kprobe" and the contents have not been only kprobes
    /// for a long time: counter-map names since m212, `sched_*` tracepoint
    /// names since m210, and after #1252 no kprobe names at all, because
    /// capture attaches to syscall tracepoints. Read it as "whatever could
    /// not be used", whatever kind of interface that is — a reader who
    /// trusts the name will misread the contents.
    ///
    /// The name stays regardless. This is a stability surface: documents
    /// produced before and after #1252 have to stay comparable, and a
    /// rename would break every consumer for a cosmetic gain.
    ///
    /// A non-empty value means the recorded sets are a floor rather than a
    /// total, and `compiler_pipeline.completeness` must say so — the two
    /// disagreeing is what #1252 fixed.
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
    /// Milestone 1070: number of file operations recorded with
    /// `unresolved_relative: true`. Derived from those operations, so the
    /// two cannot disagree. Omitted when zero.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unresolved_relative_opens: u64,
    /// Milestone 1072 (#1144): what the close-time hashing pass covered.
    ///
    /// Absent when the trace captured zero compiler invocations, so a
    /// pre-m1072 attestation serializes byte-identically (SC-007). Field
    /// placement is LAST in the struct for the same reason, following the
    /// m1070 precedent two fields above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_hash_coverage: Option<CompilerHashCoverage>,
    /// #1252: open requests the kernel discarded because the syscall failed.
    ///
    /// Those requests used to reach the read set — capture observed the
    /// request, not its result — so this is the size of the overstatement
    /// this feature removes. It is also the signal that the kernel-side join
    /// is firing at all: a compiler attempts failed opens constantly across
    /// its header and library search paths, so a zero here on a real build
    /// is evidence about the capture rather than about the build.
    ///
    /// Omitted when zero, and placed LAST in the struct, so a pre-#1252
    /// attestation round-trips and serializes byte-identically — the m1070
    /// and m1072 precedent above.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub open_failures_discarded: u64,
    /// #618 / #1185: observed events that never reached the recorded
    /// sets, by the mechanism that consumed them.
    ///
    /// Every key is an `EventDropReason::name()`. Zero-valued reasons are
    /// omitted, and the whole map is omitted when nothing was dropped, so
    /// a trace that lost nothing serializes as it did before.
    ///
    /// This exists because `events_dropped` above reads `0` whether
    /// nothing was dropped or nothing was counted, and `completeness`
    /// could read `complete` with compiler invocations missing (#1185).
    /// A named count distinguishes "the mechanism did not fire" from "no
    /// mechanism was watching". Read it alongside
    /// `ring_buffer_overflows`, which covers the one drop path that was
    /// already measured (m212).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub event_drop_reasons: std::collections::BTreeMap<String, u64>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// What the milestone-1072 close-time hashing pass covered, in distinct
/// paths rather than set entries: a path named by three invocations counts
/// once (FR-013, `contracts/read-write-set-shape.md` C-1072-5).
///
/// `paths_hashed + paths_absent + paths_skipped == paths_total` is an
/// invariant, not a coincidence — the pass visits each distinct path exactly
/// once and every visit lands in exactly one of the three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilerHashCoverage {
    /// Distinct paths across every read and write set.
    pub paths_total: u64,
    /// Existed, was a regular file, fit the budget, and hashed cleanly.
    pub paths_hashed: u64,
    /// Did not exist when the trace ended.
    pub paths_absent: u64,
    /// Existed but was not hashed: unreadable, not a regular file, over the
    /// per-file cap, or over the remaining budget.
    pub paths_skipped: u64,
    /// The cumulative-bytes budget was exhausted. The attestation is still
    /// written and the command still exits zero (C-1072-10).
    pub truncated: bool,
    pub budget_source: BudgetSource,
}

/// Where the hashing budget came from.
///
/// An enum rather than a bool so the wire shape reads
/// `"budget_source": "operator"` instead of `"budget_was_overridden": true`
/// — Principle IV, no raw primitives for domain values across boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetSource {
    /// The compiled-in default, measured under
    /// `specs/1072-compiler-set-close-time/measurements/`.
    Default,
    /// Supplied by the operator via `--compiler-hash-budget-bytes`.
    Operator,
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

    /// Milestone 1070 (SC-005): `unresolved_relative_opens` is omitted at
    /// zero and present otherwise.
    #[test]
    fn unresolved_relative_opens_omitted_at_zero() {
        let mut integrity = TraceIntegrity::default();
        let json = serde_json::to_string(&integrity).expect("serialize");
        assert!(!json.contains("unresolved_relative_opens"));
        integrity.unresolved_relative_opens = 3;
        let json = serde_json::to_string(&integrity).expect("serialize");
        assert!(json.contains("\"unresolved_relative_opens\":3"));
    }

    /// #1252 (T009): a document produced before this feature must
    /// deserialize, and a trace with no failed opens must serialize exactly
    /// as it did before the field existed.
    ///
    /// Both halves matter. The first is the compatibility claim; the second
    /// is why the field is omitted at zero rather than written as `0` — an
    /// attestation whose bytes change for a trace that observed nothing new
    /// would make every committed expectation churn for no information.
    #[test]
    fn open_failures_discarded_is_absent_before_and_at_zero() {
        // A pre-#1252 document: every field this build knows except the new
        // one. It must deserialize, and the missing field must read as 0.
        let before = r#"{
            "ring_buffer_overflows": 0,
            "events_dropped": 0,
            "uprobe_attach_failures": [],
            "kprobe_attach_failures": [],
            "partial_captures": [],
            "bloom_filter_capacity": 65536,
            "bloom_filter_false_positive_rate": 0.01,
            "filter_categories_applied": []
        }"#;
        let parsed: TraceIntegrity =
            serde_json::from_str(before).expect("a pre-#1252 document must still parse");
        assert_eq!(parsed.open_failures_discarded, 0);

        // Re-serializing it must not introduce the field. Asserted on the
        // whole string rather than on a substring search alone, so a field
        // written as `"open_failures_discarded":0` cannot pass by sitting
        // somewhere the search did not look.
        let json = serde_json::to_string(&parsed).expect("serialize");
        assert!(
            !json.contains("open_failures_discarded"),
            "a zero count must not appear in the document: {json}"
        );

        // And the converse, so the test cannot pass by the field never
        // serializing at all.
        let mut nonzero = parsed;
        nonzero.open_failures_discarded = 51;
        let json = serde_json::to_string(&nonzero).expect("serialize");
        assert!(
            json.contains("\"open_failures_discarded\":51"),
            "a non-zero count must appear: {json}"
        );
    }

    /// #618: a trace that dropped nothing must serialize exactly as it
    /// did before this field existed, and a reason that did not fire must
    /// be absent rather than present-and-zero.
    ///
    /// Both halves matter. Present-and-zero is the shape that made
    /// `events_dropped` unreadable: a consumer cannot tell a measured
    /// zero from an unmeasured one, which is the whole of #618.
    #[test]
    fn event_drop_reasons_is_absent_when_nothing_was_dropped() {
        let mut integrity = TraceIntegrity::default();
        let json = serde_json::to_string(&integrity).expect("serialize");
        assert!(
            !json.contains("event_drop_reasons"),
            "a lossless trace must not carry the field at all: {json}"
        );

        integrity
            .event_drop_reasons
            .insert("kernel_path_unreadable".to_string(), 7);
        let json = serde_json::to_string(&integrity).expect("serialize");
        assert!(
            json.contains("\"kernel_path_unreadable\":7"),
            "a reason that fired must be named with its count: {json}"
        );

        // And it round-trips, so a consumer reading it back gets the same
        // map rather than an empty default.
        let back: TraceIntegrity = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.event_drop_reasons.get("kernel_path_unreadable"), Some(&7));
    }

    /// #618: every reason name is distinct, and none is empty. The names
    /// are the wire contract a consumer joins on, so a copy-paste
    /// collision between two variants would silently merge two mechanisms
    /// into one count.
    #[test]
    fn every_drop_reason_has_a_distinct_name() {
        use crate::events::EventDropReason;
        let all: Vec<&str> = EventDropReason::KERNEL
            .iter()
            .chain(EventDropReason::USER.iter())
            .map(|r| r.name())
            .collect();
        let mut sorted = all.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "two reasons share a name: {all:?}");
        assert!(all.iter().all(|n| !n.is_empty()));
        assert_eq!(
            EventDropReason::KERNEL.len() as u32,
            EventDropReason::KERNEL_SLOTS,
            "KERNEL_SLOTS must match the array the kernel map is sized for"
        );
    }

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
            unresolved_relative_opens: 0,
            ..Default::default()
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
            unresolved_relative_opens: 0,
            ..Default::default()
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
            unresolved_relative_opens: 0,
            ..Default::default()
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
