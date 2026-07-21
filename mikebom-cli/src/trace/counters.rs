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
}
