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
// `FilterCategoryTag` variant) declared in waybill-ebpf/src/maps.rs.
// The kernel-side classifier increments the appropriate slot every
// time a file-open path matches a category prefix / substring.
//
// The `applied_categories()` accessor sorts and deduplicates the
// hit category names for emission into
// `TraceIntegrity.filter_categories_applied[]` per FR-006.

use waybill_common::events::FilterCategoryTag;
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

// ============================================================================
// #1252 — discarded-open counter reader.
// ============================================================================

/// Read `OPEN_FAILURES_DISCARDED`: open requests the kernel discarded
/// because the syscall failed.
///
/// A failure to read the map is reported as `(0, Some(reason))` rather than
/// as an error that aborts the trace, matching how the m212 counters behave:
/// the capture itself is unaffected, but a consumer must be able to tell "no
/// failed opens" from "the counter was unavailable". The reason reaches
/// `kprobe_attach_failures[]` through `counter_attach_failures`, which has
/// carried counter-map names since m212.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_open_failures_discarded(bpf: &mut aya::Ebpf) -> (u64, Option<String>) {
    match read_percpu_sum(bpf, "OPEN_FAILURES_DISCARDED") {
        Ok(n) => (n, None),
        Err(e) => {
            tracing::warn!(
                error = %truncate_error(&e.to_string()),
                "could not read OPEN_FAILURES_DISCARDED; the discarded-open \
                 count is unavailable, so zero here means unmeasured"
            );
            (0, Some("open_failures_discarded".to_string()))
        }
    }
}

/// Non-Linux stub. Matches the trace pipeline's no-op behaviour when
/// `ebpf-tracing` is off.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_open_failures_discarded<T>(_bpf: &mut T) -> (u64, Option<String>) {
    (0, None)
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

    // ---------------------------------------------------------------
    // #618 / #1185 — the attach-window baseline.
    // ---------------------------------------------------------------

    #[test]
    fn baseline_nets_out_what_the_attach_window_already_counted() {
        // The case this exists for: the open probes attach exit-first, so
        // on a busy host the slot is already nonzero before the build
        // starts. 1237 was the measured value on the x86-64 CI lane.
        let b = KernelEventDropBaseline::from_slots(vec![Some(0), Some(1237), Some(0)]);
        assert_eq!(b.net(1, 1237), 0, "a build that lost nothing reads zero");
        assert_eq!(b.net(1, 1240), 3, "only drops after attach are reported");
    }

    #[test]
    fn an_unreadable_baseline_slot_subtracts_nothing() {
        // A slot that would not read must not become a zero baseline,
        // which would silently inflate the delta by the attach window.
        // Reporting the full count is the conservative direction: it
        // over-reports loss rather than hiding it.
        let b = KernelEventDropBaseline::from_slots(vec![None]);
        assert_eq!(b.net(0, 99), 99);
    }

    #[test]
    fn a_slot_past_the_end_subtracts_nothing() {
        let b = KernelEventDropBaseline::from_slots(vec![Some(5)]);
        assert_eq!(b.net(7, 11), 11);
    }

    #[test]
    fn a_lower_later_reading_is_not_a_wrap() {
        // Per-CPU sums read at two points in time: a CPU coming online
        // between them can lower the total. Must read as "nothing since
        // attach", not as u64::MAX.
        let b = KernelEventDropBaseline::from_slots(vec![Some(500)]);
        assert_eq!(b.net(0, 100), 0);
    }

    #[test]
    fn a_default_baseline_subtracts_nothing() {
        // The non-Linux stub and the pre-attach state both produce this.
        let b = KernelEventDropBaseline::default();
        assert_eq!(b.net(0, 42), 42);
    }

}

// ============================================================================
// #618 / #1185 — named drop counters.
// ============================================================================

/// Observed events that never reached the recorded sets, by mechanism.
///
/// Userspace-side reasons are counted here as the drain loop runs;
/// kernel-side reasons are read out of `EVENT_DROPS` at trace end and
/// merged in. The result reaches
/// `TraceIntegrity.event_drop_reasons`.
///
/// Why named counts rather than one total: `events_dropped` has read `0`
/// since it was introduced, and a single zero cannot distinguish "nothing
/// was dropped" from "nothing was counted". #1185 is a capture losing
/// whole compiler invocations while reporting `completeness: complete`,
/// and narrowing it by elimination cost one CI cycle per hypothesis.
#[derive(Debug, Default, Clone)]
pub struct EventDropCounts {
    counts: std::collections::BTreeMap<&'static str, u64>,
}

impl EventDropCounts {
    pub fn record(&mut self, reason: waybill_common::events::EventDropReason) {
        *self.counts.entry(reason.name()).or_insert(0) += 1;
    }

    pub fn record_n(&mut self, reason: waybill_common::events::EventDropReason, n: u64) {
        if n > 0 {
            *self.counts.entry(reason.name()).or_insert(0) += n;
        }
    }

    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    /// The count for one reason, or zero if it did not fire.
    pub fn get(&self, reason: waybill_common::events::EventDropReason) -> u64 {
        self.counts.get(reason.name()).copied().unwrap_or(0)
    }

    /// The total that represents events actually lost.
    ///
    /// `kernel_outside_traced_subtree` is **not** loss and is excluded.
    /// With the gate armed those opens are dropped on purpose: they came
    /// from processes that are not the traced build, and removing them is
    /// the feature rather than a shortfall. Counting them as loss would
    /// make the "#618 the capture is a floor, not a total" warning fire on
    /// every healthy scoped trace — the false alarm that teaches people to
    /// ignore the real one, which this module already warns about for
    /// `user_no_invocation_mapping`.
    ///
    /// The distinction the attestation needs is between *the build's*
    /// events that went missing and *other processes'* events that were
    /// declined. This total is the first; the slot is reported separately
    /// as the second.
    pub fn total_lost(&self) -> u64 {
        self.total() - self.get(waybill_common::events::EventDropReason::KernelOutsideTracedSubtree)
    }

    /// Wire shape: zero-valued reasons are omitted, so a reason that did
    /// not fire is absent rather than present-and-zero. An empty map
    /// serializes away entirely, leaving a lossless trace byte-identical
    /// to one produced before this existed.
    pub fn into_wire(self) -> std::collections::BTreeMap<String, u64> {
        self.counts
            .into_iter()
            .filter(|(_, n)| *n > 0)
            .map(|(k, n)| (k.to_string(), n))
            .collect()
    }
}

/// The kernel drop slots as they read once attach has completed.
///
/// The open probes attach exit-before-entry on purpose: an entry program
/// live before its exit program would stash requests nothing can join,
/// losing successful opens outright. The cost is a window in which an
/// exit fires with no stored entry, and because the probes are
/// system-wide that window counts every open on the machine rather than
/// just the build's. On a quiet host it reads 0 and on a busy CI runner
/// it read 1237 (measurements/findings.md run 6) — the same healthy
/// capture either way.
///
/// So the attach window is subtracted rather than reported. What remains
/// is attributable: a nonzero delta means entry events were lost while
/// the probes were fully attached.
#[derive(Debug, Default, Clone)]
pub struct KernelEventDropBaseline {
    /// Per-slot values, indexed as `EventDropReason::KERNEL`. A slot that
    /// could not be read is `None`, which subtracts nothing — an
    /// unmeasured baseline must not silently become a zero one.
    slots: Vec<Option<u64>>,
}

impl KernelEventDropBaseline {
    /// What this slot read at attach time. A slot that could not be read
    /// contributes 0, so an unmeasured baseline subtracts nothing rather
    /// than quietly becoming a zero one.
    fn at(&self, slot: usize) -> u64 {
        self.slots.get(slot).copied().flatten().unwrap_or(0)
    }

    /// Drops attributable to the traced build: this slot now, less what
    /// it already held when the probes finished attaching.
    ///
    /// `saturating_sub` rather than an assertion, because these are
    /// per-CPU counters summed at two points in time and a CPU coming
    /// online between them can lower the sum. A negative delta means
    /// "nothing since attach", not a fault.
    pub fn net(&self, slot: usize, now: u64) -> u64 {
        now.saturating_sub(self.at(slot))
    }

    /// Construct from per-slot readings. `None` is a slot that would not
    /// read.
    pub fn from_slots(slots: Vec<Option<u64>>) -> Self {
        Self { slots }
    }
}

/// Read every kernel drop slot immediately after attach completes.
///
/// Called before the traced child is spawned, so anything counted here
/// predates the build entirely.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_kernel_event_drop_baseline(bpf: &mut aya::Ebpf) -> KernelEventDropBaseline {
    use waybill_common::events::EventDropReason;
    let mut slots = Vec::with_capacity(EventDropReason::KERNEL.len());
    for slot in 0..EventDropReason::KERNEL.len() {
        match read_percpu_slot_sum(bpf, "EVENT_DROPS", slot as u32) {
            Ok(n) => slots.push(Some(n)),
            Err(e) => {
                // Said out loud rather than swallowed. An unreadable
                // baseline degrades this slot to the pre-baseline
                // behaviour — the attach window counted as loss — and
                // without this line that is indistinguishable from the
                // baseline working, which is the ambiguity #618 exists
                // to remove.
                tracing::warn!(
                    error = %truncate_error(&e.to_string()),
                    slot,
                    "could not baseline EVENT_DROPS; this slot will include \
                     opens that exited during the attach window, so a nonzero \
                     count for it is not attributable to the build"
                );
                slots.push(None);
            }
        }
    }
    KernelEventDropBaseline::from_slots(slots)
}

/// Non-Linux stub.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_kernel_event_drop_baseline<T>(_bpf: &mut T) -> KernelEventDropBaseline {
    KernelEventDropBaseline::default()
}

/// Read the kernel-side `EVENT_DROPS` slots and merge them in, net of the
/// attach-window baseline.
///
/// A map that will not read is itself reported, through
/// `counter_attach_failures`, for the same reason the m212 counters are:
/// a zero from an unreadable counter is not a measurement.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn merge_kernel_event_drops(
    bpf: &mut aya::Ebpf,
    baseline: &KernelEventDropBaseline,
    counts: &mut EventDropCounts,
) -> Option<String> {
    use waybill_common::events::EventDropReason;
    for (slot, reason) in EventDropReason::KERNEL.iter().enumerate() {
        // #1185 — the attach-window baseline is right for some slots and
        // wrong for this one.
        //
        // Baselining exists because the probes are machine-wide and the
        // window between attaching them counts syscalls the trace did not
        // cause. For `kernel_exit_without_request` those counts are an
        // artifact and subtracting them is correct.
        //
        // `kernel_outside_traced_subtree` is different: what it counts
        // during that window is the tracer's own load-time reads — aya
        // probing `/proc/version_signature` and friends — and those are
        // real opens from a real process outside the subtree, which the
        // gate will drop. Subtracting them makes the figure understate the
        // gate's effect by exactly the amount the tracer reads at startup.
        //
        // Measured: a single-process probe captured 26 `waybill` reads and
        // reported `kernel_outside_traced_subtree = 0`, because all 26 were
        // counted before the baseline was taken and then subtracted away.
        let raw = *reason == EventDropReason::KernelOutsideTracedSubtree;
        match read_percpu_slot_sum(bpf, "EVENT_DROPS", slot as u32)
            .map(|n| if raw { n } else { baseline.net(slot, n) })
        {
            Ok(n) => counts.record_n(*reason, n),
            Err(e) => {
                tracing::warn!(
                    error = %truncate_error(&e.to_string()),
                    slot,
                    "could not read EVENT_DROPS; kernel-side drop reasons are \
                     unavailable, so their absence here means unmeasured"
                );
                return Some("event_drops".to_string());
            }
        }
    }
    None
}

/// Non-Linux stub.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn merge_kernel_event_drops<T>(
    _bpf: &mut T,
    _baseline: &KernelEventDropBaseline,
    _counts: &mut EventDropCounts,
) -> Option<String> {
    None
}

/// #1185 — how many in-flight-open entries the kernel map still held when
/// the trace ended.
///
/// Counted by walking the keys, because a BPF hash map does not publish
/// its own occupancy. The walk is bounded by the map's capacity, runs once
/// per trace after the drain loop, and reads nothing the probes are still
/// writing into — by this point the programs are detached.
///
/// `None` is "not measured", never zero: a map that will not read is the
/// case this whole counter family exists to distinguish from a mechanism
/// that did not fire. The caller records the name in
/// `counter_attach_failures` so the absence is visible in the attestation.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_pending_open_entries(bpf: &mut aya::Ebpf) -> (Option<u64>, Option<String>) {
    use waybill_common::maps::PendingOpen;

    // `map_mut` rather than `map`, matching `read_percpu_slot_sum` above:
    // the same borrow shape every other reader in this file uses, so the
    // trait resolution is one that is already known to compile for the
    // eBPF target.
    let Some(map) = bpf.map_mut("PENDING_OPEN") else {
        tracing::warn!(
            "PENDING_OPEN map not found; its occupancy is unmeasured, so a \
             nonzero kernel_pending_insert_failed cannot be read against how \
             full the map was"
        );
        return (None, Some("pending_open".to_string()));
    };
    let typed = match aya::maps::HashMap::<_, u32, PendingOpen>::try_from(map) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(
                error = %truncate_error(&e.to_string()),
                "PENDING_OPEN is not a usable hash map; its occupancy is unmeasured"
            );
            return (None, Some("pending_open".to_string()));
        }
    };
    // A key that will not read is counted, not skipped: it occupies a slot
    // either way, and the number being measured is how many slots are gone.
    let held = typed.keys().count() as u64;
    (Some(held), None)
}

/// Non-Linux stub.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_pending_open_entries<T>(_bpf: &mut T) -> (Option<u64>, Option<String>) {
    (None, None)
}

/// #1185 — how many pids the kernel currently counts as inside the traced
/// subtree.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_traced_pid_count(bpf: &mut aya::Ebpf) -> anyhow::Result<u64> {
    let map = bpf
        .map_mut("TRACED_PIDS")
        .ok_or_else(|| anyhow::anyhow!("TRACED_PIDS map not found in loaded eBPF object"))?;
    let typed = aya::maps::HashMap::<_, u32, u8>::try_from(map)
        .map_err(|e| anyhow::anyhow!("TRACED_PIDS is not a usable hash map: {e}"))?;
    Ok(typed.keys().count() as u64)
}

/// #1185 — opens observed from inside the traced subtree.
///
/// The positive half of the dry run. With the gate off, capture is
/// identical whether the subtree holds one process or ninety, so this is
/// the only signal that fork propagation works.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub fn read_traced_subtree_hits(bpf: &mut aya::Ebpf) -> (u64, Option<String>) {
    match read_percpu_sum(bpf, "TRACED_SUBTREE_HITS") {
        Ok(n) => (n, None),
        Err(e) => {
            tracing::warn!(
                error = %truncate_error(&e.to_string()),
                "could not read TRACED_SUBTREE_HITS; whether the traced \
                 subtree propagated past its root is unmeasured, so a zero \
                 here would not mean propagation failed"
            );
            (0, Some("traced_subtree_hits".to_string()))
        }
    }
}

/// Non-Linux stub.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_traced_subtree_hits<T>(_bpf: &mut T) -> (u64, Option<String>) {
    (0, None)
}

/// #1185 — block until the traced root has announced itself, or fail.
///
/// The root performs its handshake between fork and exec, so `spawn()` can
/// return before the kernel has seen it. This waits for the seed rather
/// than sampling once and guessing.
///
/// Nothing is drained here, and nothing needs to be: the handshake is
/// recorded by the kernel writing a map, not by an event crossing the ring
/// buffer. That is the reason for seeding through a map — a seed that
/// depended on an event would be lost by exactly the mechanisms this work
/// is investigating.
///
/// **This returns an error rather than warning.** Once the subtree gate
/// lands, a trace that runs without a seeded root drops every open and
/// reports itself complete, which is #1185's own failure rebuilt
/// deliberately. The verification is therefore added before the gate, so
/// that state is unreachable rather than merely unlikely.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
/// #1185 — localise a handshake that never arrived.
///
/// Reads `HANDSHAKE_DIAG` and the nonce slot and says which link in the
/// chain broke, rather than leaving the reader with "TRACED_PIDS is
/// empty" and no way to narrow it. Each slot rules something in or out;
/// see `waybill-ebpf/src/maps.rs::HANDSHAKE_DIAG`.
///
/// Every read here can fail, and a failed read must not read as a zero:
/// a zero in slot 0 is a strong claim (the entry program does not run on
/// this kernel) and an unreadable map is no claim at all.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn describe_handshake_failure(bpf: &mut aya::Ebpf) -> String {
    fn slot(bpf: &mut aya::Ebpf, i: u32) -> String {
        match read_percpu_slot_sum(bpf, "HANDSHAKE_DIAG", i) {
            Ok(n) => n.to_string(),
            Err(e) => format!("unreadable({})", truncate_error(&e.to_string())),
        }
    }
    let entered = slot(bpf, 0);
    let sentinel = slot(bpf, 1);
    let rejected = slot(bpf, 2);
    let recognised = slot(bpf, 3);
    let insert_failed = match read_percpu_slot_sum(bpf, "EVENT_DROPS", 5) {
        Ok(n) => n.to_string(),
        Err(e) => format!("unreadable({})", truncate_error(&e.to_string())),
    };
    let nonce_readback = match bpf
        .map_mut("TRACE_HANDSHAKE_NONCE")
        .ok_or_else(|| anyhow::anyhow!("map not found"))
        .and_then(|m| {
            aya::maps::Array::<_, u32>::try_from(m)
                .map_err(|e| anyhow::anyhow!("not a usable array: {e}"))
        })
        .and_then(|a| {
            a.get(&waybill_common::maps::TRACE_HANDSHAKE_NONCE_SLOT, 0)
                .map_err(|e| anyhow::anyhow!("{e}"))
        }) {
        Ok(n) => n.to_string(),
        Err(e) => format!("unreadable({})", truncate_error(&e.to_string())),
    };

    // The reading, stated rather than left to be re-derived. Phrased as
    // what each shape rules out, because the point of the counters is
    // that two plausible mechanisms were already wrong.
    let reading = if entered == "0" {
        "slot 0 is zero: `sys_enter_openat` did not fire at all, which is \
         larger than the handshake and would explain a short capture too"
    } else if sentinel == "0" {
        "the entry program ran and no open carried the sentinel `dfd`: \
         either the marker syscall never reached this tracepoint, or the \
         `dfd` does not arrive as written"
    } else if recognised == "0" {
        "the sentinel `dfd` arrived and the nonce did not match: compare \
         the read-back below against what the loader wrote"
    } else {
        "the handshake was recognised and seeding was attempted, so the \
         break is at the insert or in what userspace reads"
    };

    format!(
        "#1185 handshake diagnosis -- openat_entered={entered} \
         sentinel_dfd_seen={sentinel} nonce_rejected={rejected} \
         recognised={recognised} traced_insert_failed={insert_failed} \
         nonce_readback={nonce_readback}; {reading}"
    )
}

pub fn wait_for_traced_root(bpf: &mut aya::Ebpf, timeout: std::time::Duration) -> anyhow::Result<u64> {
    let start = std::time::Instant::now();
    loop {
        let n = read_traced_pid_count(bpf)?;
        if n > 0 {
            tracing::info!(
                traced_pids = n,
                waited_ms = start.elapsed().as_millis() as u64,
                "#1185 traced subtree seeded: the root announced itself"
            );
            return Ok(n);
        }
        if start.elapsed() >= timeout {
            // #1185 — say where it died before refusing.
            //
            // This guard destroyed its own evidence: aborting here means
            // no attestation is written, so every drop counter was
            // unreadable -- including `kernel_traced_insert_failed`,
            // which covers the seeding path itself. Diagnosing the 7.0
            // x86_64 failure took a separate probe and two refuted
            // hypotheses because of it.
            let d = describe_handshake_failure(bpf);
            tracing::error!("{d}");
            anyhow::bail!(
                "the traced root never announced itself within {:?}: TRACED_PIDS is empty, \
                 so the kernel has no traced subtree. The capture would admit every process \
                 on the machine, and once the subtree gate is enabled it would instead admit \
                 none while still reporting itself complete (#1185). Refusing to trace on an \
                 unestablished root rather than producing a figure that cannot be attributed. \
                 {d}",
                timeout
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Non-Linux stubs.
#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn read_traced_pid_count<T>(_bpf: &mut T) -> anyhow::Result<u64> {
    Ok(0)
}

#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
#[allow(dead_code)]
pub fn wait_for_traced_root<T>(_bpf: &mut T, _timeout: std::time::Duration) -> anyhow::Result<u64> {
    Ok(0)
}
