//! Milestone 212 (issue #615) — read the kernel-side per-CPU drop
//! counter maps at trace end.
//!
//! Each of the three ring buffers (`FILE_EVENTS`, `NETWORK_EVENTS`,
//! `COMPILER_EXEC_EVENTS`) has a companion `PerCpuArray<u64>` counter
//! map that eBPF programs increment on `reserve() → None`. This
//! module aggregates those per-CPU values into userspace-side totals,
//! which the `TraceIntegrity` builder sums into `ring_buffer_overflows`.
//!
//! Pre-m212 that field was hardcoded to `0` at every emission site,
//! hiding a real drop bug where cargo's fingerprint spam saturated
//! the ring buffer + rustc's file events were silently lost (see the
//! #614 investigation).
//!
//! Failure semantics per spec.md FR-005 + Q4: if a specific counter
//! map fails to attach (older kernel, unusual configuration), that
//! map's contribution is `0` (via `unwrap_or(0)` per-map). The
//! successful maps still contribute their real counts, so the returned
//! aggregate is a **partial sum** — a floor, not a total. The caller
//! reports the failing map name via `TraceIntegrity.kprobe_attach_failures[]`
//! per Q3 so downstream consumers can tell.
//!
//! Dead-code allowance rationale: on default-features / non-Linux
//! hosts (macOS dev, linux-x86_64 without --features ebpf-tracing),
//! the caller path in `scan.rs::execute_scan` is `#[cfg(all(
//! target_os = "linux", feature = "ebpf-tracing"))]`-gated. The
//! module's public types + functions compile everywhere (so the
//! unit tests run cross-platform) but are only actively called on
//! the eBPF-tracing build; hence the module-level dead_code allow.

#![cfg_attr(
    not(all(target_os = "linux", feature = "ebpf-tracing")),
    allow(dead_code)
)]

use std::collections::HashMap;

#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
use aya::maps::PerCpuArray;

/// The three counter map names we read at trace end. Each entry:
///   (short_name, kernel_map_name)
/// short_name is used both as the HashMap key returned to callers AND
/// as the identifier appended to `TraceIntegrity.kprobe_attach_failures[]`
/// when the map fails to attach (per spec.md FR-005 Q3).
pub const COUNTER_MAPS: &[(&str, &str)] = &[
    ("file_event_drops", "FILE_EVENT_DROPS"),
    ("network_event_drops", "NETWORK_EVENT_DROPS"),
    ("compiler_exec_drops", "COMPILER_EXEC_DROPS"),
];

/// Per-map drop-counter aggregate returned by `read_ring_buffer_drops`.
/// The `attach_failures` field carries the short names of any counter
/// maps that failed to attach (per Q3 disambiguation).
#[derive(Debug, Default, Clone)]
pub struct RingBufferDropsSummary {
    pub per_map: HashMap<&'static str, u64>,
    pub attach_failures: Vec<String>,
}

impl RingBufferDropsSummary {
    /// Aggregate across all three counters into a single u64. Failing
    /// maps contribute 0 (partial sum per FR-005 + Q4).
    pub fn total(&self) -> u64 {
        self.per_map.values().sum()
    }
}

/// Read all three per-CPU drop counter maps + sum across CPUs.
///
/// Called once per trace, at trace-end, after the settling drain
/// deadline expires. Returns per-map counts + a list of any maps
/// that failed to attach.
///
/// The `#[cfg(...)]` gate mirrors the rest of the trace pipeline —
/// on non-Linux hosts (macOS dev) the eBPF-tracing feature isn't
/// active, so this function returns an empty summary that the caller
/// treats as "no drops."
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_ring_buffer_drops(bpf: &mut aya::Ebpf) -> RingBufferDropsSummary {
    let mut per_map = HashMap::new();
    let mut attach_failures = Vec::new();
    for (short, map_name) in COUNTER_MAPS {
        match read_percpu_sum(bpf, map_name) {
            Ok(sum) => {
                per_map.insert(*short, sum);
            }
            Err(e) => {
                tracing::warn!(
                    map = %map_name,
                    error = %truncate_error(&e.to_string()),
                    "counter map not usable on this kernel; ring_buffer_overflows will be a partial sum"
                );
                per_map.insert(*short, 0);
                attach_failures.push((*short).to_string());
            }
        }
    }
    RingBufferDropsSummary {
        per_map,
        attach_failures,
    }
}

/// Non-Linux stub — returns an empty summary. Matches the trace
/// pipeline's overall no-op behavior when the `ebpf-tracing` feature
/// isn't enabled (default features + macOS dev builds).
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_ring_buffer_drops<T>(_bpf: &mut T) -> RingBufferDropsSummary {
    RingBufferDropsSummary::default()
}

/// Read one per-CPU counter map + sum across all online CPUs.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn read_percpu_sum(bpf: &mut aya::Ebpf, name: &str) -> anyhow::Result<u64> {
    let map = bpf
        .map_mut(name)
        .ok_or_else(|| anyhow::anyhow!("map `{name}` not found in loaded eBPF object"))?;
    let per_cpu: PerCpuArray<_, u64> = PerCpuArray::try_from(map)?;
    // `get(&0, 0)` returns Vec<u64>, one entry per online CPU.
    let values = per_cpu.get(&0, 0)?;
    Ok(values.iter().sum())
}

/// Truncate the aya error message for the WARN log body per FR-005.
/// Verifier-dump-style errors can span 20+ KB otherwise (same class
/// of overflow that m211 addressed for the vfs_open WARN pre-retire).
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn truncate_error(msg: &str) -> String {
    const MAX: usize = 300;
    if msg.len() <= MAX {
        msg.to_string()
    } else {
        format!("{}... (truncated; {} bytes)", &msg[..MAX], msg.len())
    }
}

// ============================================================================
// Milestone 213 (issue #616) — filter-category hits reader.
// ============================================================================
//
// Parallel to `read_ring_buffer_drops` above, but reads the
// `FILTER_CATEGORY_HITS` per-CPU array (4 slots — one per
// `FilterCategoryTag` variant) declared in mikebom-ebpf/src/maps.rs.
// The kernel-side classifier increments the appropriate slot every
// time a file-open path matches a category prefix / substring.
//
// The `applied_categories()` accessor sorts and deduplicates the
// hit category names for emission into
// `TraceIntegrity.filter_categories_applied[]` per FR-006.

use mikebom_common::events::FilterCategoryTag;
use std::collections::BTreeMap;

/// Aggregated per-category hit counts + attach-failure record.
/// Populated by `read_filter_category_hits` at trace end.
#[derive(Debug, Default, Clone)]
pub struct FilterCategoryHitsSummary {
    /// Per-category sum across all online CPUs. Categories with count == 0
    /// are omitted from `applied_categories()`.
    pub per_category: BTreeMap<FilterCategoryTag, u64>,
    /// Names of counter maps that failed to attach on this kernel.
    /// Per m213 R9, `"filter_category_hits"` is added here as a single
    /// entry if the map is missing (not per-slot). Surfaced via
    /// `TraceIntegrity.kprobe_attach_failures[]` at trace-end wire-up.
    pub attach_failures: Vec<String>,
}

impl FilterCategoryHitsSummary {
    /// Names of categories with count > 0, sorted lexicographically and
    /// deduplicated for wire-shape stability per FR-006.
    ///
    /// Empty when no category fired — the caller MUST still emit the
    /// empty vec into `TraceIntegrity.filter_categories_applied` per
    /// FR-009 so consumers see `[]` (never `null`, never absent).
    pub fn applied_categories(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .per_category
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(cat, _)| cat.name().to_string())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// Read the 4-slot `FILTER_CATEGORY_HITS` per-CPU map from the loaded
/// eBPF object + sum across online CPUs for each slot. Returns a
/// summary with per-category counts and any attach failures.
///
/// If the map itself fails to attach (older kernel, unusual config),
/// `"filter_category_hits"` is added to `attach_failures` and the
/// per-category map is populated with zeros — the aggregate then reads
/// as "no categories fired" (equivalent to filter-disabled). Operators
/// see the attach failure via `TraceIntegrity.kprobe_attach_failures[]`
/// per m213 R9.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_filter_category_hits(bpf: &mut aya::Ebpf) -> FilterCategoryHitsSummary {
    let mut per_category: BTreeMap<FilterCategoryTag, u64> = BTreeMap::new();
    let mut attach_failures = Vec::new();
    let mut all_slots_failed = true;

    for cat in FilterCategoryTag::ALL {
        match read_percpu_slot_sum(bpf, "FILTER_CATEGORY_HITS", cat as u32) {
            Ok(sum) => {
                per_category.insert(cat, sum);
                all_slots_failed = false;
            }
            Err(e) => {
                tracing::warn!(
                    slot = ?cat,
                    error = %truncate_error(&e.to_string()),
                    "filter-category hits slot not readable; treating as 0"
                );
                per_category.insert(cat, 0);
            }
        }
    }

    // Per R9, surface a SINGLE attach-failure entry if the map is
    // completely unusable (all slots failed) rather than one entry per
    // slot. Downstream consumers can then treat the whole aggregate as
    // "no categories reported" without needing to parse 4 separate names.
    if all_slots_failed {
        attach_failures.push("filter_category_hits".to_string());
    }

    FilterCategoryHitsSummary {
        per_category,
        attach_failures,
    }
}

/// Non-Linux stub — returns an empty summary. Matches the trace
/// pipeline's overall no-op behavior when `ebpf-tracing` is off.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_filter_category_hits<T>(_bpf: &mut T) -> FilterCategoryHitsSummary {
    FilterCategoryHitsSummary::default()
}

/// Read one slot from a per-CPU u64 map + sum across all online CPUs.
/// Parallel to `read_percpu_sum` above but takes an explicit slot index
/// (for maps with `max_entries > 1` like `FILTER_CATEGORY_HITS`).
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn read_percpu_slot_sum(bpf: &mut aya::Ebpf, name: &str, idx: u32) -> anyhow::Result<u64> {
    let map = bpf
        .map_mut(name)
        .ok_or_else(|| anyhow::anyhow!("map `{name}` not found in loaded eBPF object"))?;
    let per_cpu: PerCpuArray<_, u64> = PerCpuArray::try_from(map)?;
    let values = per_cpu.get(&idx, 0)?;
    Ok(values.iter().sum())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn summary_total_sums_per_map_values() {
        let mut summary = RingBufferDropsSummary::default();
        summary.per_map.insert("file_event_drops", 100);
        summary.per_map.insert("network_event_drops", 50);
        summary.per_map.insert("compiler_exec_drops", 25);
        assert_eq!(summary.total(), 175);
    }

    #[test]
    fn summary_total_handles_partial_failure() {
        // Partial-sum semantics per spec.md FR-005 + Q4: failing maps
        // contribute 0 (via unwrap_or(0)), successful maps contribute
        // their real counts. The aggregate is a floor, not a total.
        let mut summary = RingBufferDropsSummary::default();
        summary.per_map.insert("file_event_drops", 12345);
        summary.per_map.insert("network_event_drops", 0); // "failed to attach"
        summary.per_map.insert("compiler_exec_drops", 42);
        summary.attach_failures.push("network_event_drops".to_string());
        assert_eq!(summary.total(), 12387);
        assert_eq!(summary.attach_failures.len(), 1);
    }

    #[test]
    fn counter_maps_const_matches_data_model() {
        // Data-model E2 declares exactly three counter maps. If someone
        // adds a fourth ring buffer without updating this const, the
        // aggregation drops the new map's drops silently. This test
        // pins the count so a future PR must explicitly extend the
        // COUNTER_MAPS list.
        assert_eq!(COUNTER_MAPS.len(), 3);
        let short_names: Vec<&str> = COUNTER_MAPS.iter().map(|(s, _)| *s).collect();
        assert!(short_names.contains(&"file_event_drops"));
        assert!(short_names.contains(&"network_event_drops"));
        assert!(short_names.contains(&"compiler_exec_drops"));
    }

    // ========================================================================
    // Milestone 213 T017 (issue #616) — FilterCategoryHitsSummary tests.
    // ========================================================================

    #[test]
    fn t017_applied_categories_sorts_and_dedups() {
        // FR-006: emitted names MUST be sorted-deduplicated.
        let mut summary = FilterCategoryHitsSummary::default();
        summary.per_category.insert(FilterCategoryTag::System, 42);
        summary
            .per_category
            .insert(FilterCategoryTag::CargoFingerprint, 167_000);
        summary.per_category.insert(FilterCategoryTag::Ephemeral, 8);
        let applied = summary.applied_categories();
        // Alphabetical order: CargoFingerprint < Ephemeral < System.
        assert_eq!(
            applied,
            vec![
                "CargoFingerprint".to_string(),
                "Ephemeral".to_string(),
                "System".to_string(),
            ]
        );
    }

    #[test]
    fn t017_applied_categories_omits_zero_counts() {
        // Categories with count == 0 MUST NOT appear (they never fired).
        let mut summary = FilterCategoryHitsSummary::default();
        summary.per_category.insert(FilterCategoryTag::System, 0);
        summary.per_category.insert(FilterCategoryTag::UserCache, 5);
        summary.per_category.insert(FilterCategoryTag::Ephemeral, 0);
        summary
            .per_category
            .insert(FilterCategoryTag::CargoFingerprint, 3);
        let applied = summary.applied_categories();
        assert_eq!(
            applied,
            vec!["CargoFingerprint".to_string(), "UserCache".to_string()]
        );
    }

    #[test]
    fn t017_applied_categories_empty_when_no_hits() {
        // FR-009: empty state MUST return `vec![]` (never null).
        let summary = FilterCategoryHitsSummary::default();
        assert_eq!(summary.applied_categories(), Vec::<String>::new());
        assert!(summary.applied_categories().is_empty());
    }

    #[test]
    fn t017_applied_categories_empty_when_all_zero() {
        // Every category present but every count == 0 → still empty vec.
        let mut summary = FilterCategoryHitsSummary::default();
        for cat in FilterCategoryTag::ALL {
            summary.per_category.insert(cat, 0);
        }
        assert!(summary.applied_categories().is_empty());
    }

    #[test]
    fn t017_attach_failures_propagate_through_summary() {
        // Per R9: a failing FILTER_CATEGORY_HITS map surfaces as one
        // entry `"filter_category_hits"` in `attach_failures` — not
        // one entry per slot. The wire-up layer (scan.rs at trace-end)
        // appends this to `TraceIntegrity.kprobe_attach_failures[]` per
        // the m212 disambiguation convention.
        let mut summary = FilterCategoryHitsSummary::default();
        summary
            .attach_failures
            .push("filter_category_hits".to_string());
        assert_eq!(summary.attach_failures.len(), 1);
        assert_eq!(summary.attach_failures[0], "filter_category_hits");
        // Zero counts + attach_failures populated = "filter didn't run
        // reliably" state. applied_categories() correctly returns empty.
        assert!(summary.applied_categories().is_empty());
    }
}
