//! Builds an InTotoStatement from aggregated trace data.

// Builder is only invoked from `cli/scan.rs::execute_scan` (Linux-only
// trace flow); on macOS this file compiles but its functions are
// unreachable. Allow dead_code on non-Linux to keep cross-platform
// clippy clean.
#![allow(dead_code)]

use std::collections::BTreeMap;

use waybill_common::attestation::compiler_pipeline::CompilerPipelineData;
use waybill_common::attestation::metadata::{
    GenerationContext, HostInfo, ProcessInfo, ToolInfo, TraceMetadata,
};
use waybill_common::attestation::statement::{
    BuildTracePredicate, InTotoStatement, ResourceDescriptor,
};
use waybill_common::types::timestamp::Timestamp;

use crate::attestation::subject::SubjectResolver;
use crate::config;
use crate::trace::aggregator::AggregatedTrace;

/// Configuration for building an attestation.
pub struct AttestationConfig {
    pub target_pid: u32,
    pub target_command: String,
    pub cgroup_id: u64,
    /// Legacy hardcoded name, preserved for callers that still pass
    /// synthetic strings. Ignored when `subject_resolver` is `Some`.
    pub subject_name: String,
    /// Legacy hardcoded digest, preserved as above.
    pub subject_digest: Option<String>,
    /// Feature 006 — when present, drives real subject resolution via
    /// operator override → artifact-dir walk → magic-byte → synthetic
    /// fallback. Supersedes `subject_name` + `subject_digest`.
    pub subject_resolver: Option<SubjectResolver>,
}

/// Build an InTotoStatement from aggregated trace results.
///
/// Milestone 210: `compiler_pipeline` carries the compiler-invocation
/// DAG + per-invocation read/write sets when the trace observed
/// compiler execs (via `sched_process_exec` tracepoint). `None`
/// when the trace ran without eBPF compiler-pipeline data OR the
/// operator's command didn't invoke a whitelisted compiler.
pub fn build_attestation(
    trace: AggregatedTrace,
    cfg: &AttestationConfig,
    trace_start: Timestamp,
    trace_end: Timestamp,
    compiler_pipeline: Option<CompilerPipelineData>,
) -> anyhow::Result<InTotoStatement> {
    let host = detect_host_info();

    let metadata = TraceMetadata {
        tool: ToolInfo {
            name: config::TOOL_NAME.to_string(),
            version: config::TOOL_VERSION.to_string(),
        },
        trace_start,
        trace_end,
        target_process: ProcessInfo {
            pid: cfg.target_pid,
            command: cfg.target_command.clone(),
            cgroup_id: cfg.cgroup_id,
        },
        host,
        generation_context: GenerationContext::BuildTimeTrace,
    };

    let predicate = BuildTracePredicate {
        metadata,
        network_trace: trace.network_trace,
        file_access: trace.file_access,
        trace_integrity: trace.trace_integrity,
        // Milestone 210 — compiler-pipeline data threaded through from
        // scan.rs's `CompilerPipelineAggregator::finalize()`. `None`
        // when the trace didn't run under `--features ebpf-tracing`
        // OR the operator's command didn't invoke a whitelisted
        // compiler.
        compiler_pipeline,
    };

    // Subject array per FR-007 / FR-010. When a resolver is attached,
    // use the precedence ladder (operator override → artifact-dir → magic
    // → synthetic). Fall through to the legacy hardcoded pair otherwise
    // (kept for AttestationConfig callers that predate feature 006).
    let subject = if let Some(ref resolver) = cfg.subject_resolver {
        resolver
            .resolve()
            .iter()
            .map(|s| s.to_resource_descriptor())
            .collect()
    } else {
        let mut digest = BTreeMap::new();
        if let Some(ref hash) = cfg.subject_digest {
            digest.insert("sha256".to_string(), hash.clone());
        }
        vec![ResourceDescriptor {
            name: cfg.subject_name.clone(),
            digest,
        }]
    };

    Ok(InTotoStatement {
        statement_type: InTotoStatement::STATEMENT_TYPE.to_string(),
        subject,
        predicate_type: InTotoStatement::PREDICATE_TYPE.to_string(),
        predicate,
    })
}

fn detect_host_info() -> HostInfo {
    HostInfo {
        os: std::env::consts::OS.to_string(),
        kernel_version: detect_kernel_version(),
        arch: std::env::consts::ARCH.to_string(),
        distro_codename: detect_distro_codename(),
    }
}

#[cfg(target_os = "linux")]
fn detect_kernel_version() -> String {
    std::fs::read_to_string("/proc/version")
        .ok()
        .and_then(|v| v.split_whitespace().nth(2).map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(not(target_os = "linux"))]
fn detect_kernel_version() -> String {
    "unknown".to_string()
}

/// Read the canonical distro tag from the trace host's own
/// `/etc/os-release`. Delegates to the shared scan_fs helper so
/// scan-mode and build-time paths produce the same
/// `<ID>-<VERSION_ID>` shape (with VERSION_CODENAME fallback).
/// The attestation struct field is still named `distro_codename` for
/// backwards compat; the value it carries is the canonical tag.
fn detect_distro_codename() -> Option<String> {
    crate::scan_fs::os_release::detect_host_distro_tag()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::attestation::file::{FileAccess, FileAccessSummary};
    use waybill_common::attestation::integrity::TraceIntegrity;
    use waybill_common::attestation::network::{NetworkSummary, NetworkTrace};
    use crate::trace::aggregator::AggregatedTrace;

    #[test]
    fn builds_valid_attestation() {
        let trace = AggregatedTrace {
            network_trace: NetworkTrace {
                connections: vec![],
                summary: NetworkSummary {
                    total_connections: 0,
                    unique_hosts: vec![],
                    unique_ips: vec![],
                    protocol_counts: BTreeMap::new(),
                    total_bytes_received: 0,
                },
            },
            file_access: FileAccess {
                operations: vec![],
                summary: FileAccessSummary {
                    total_operations: 0,
                    unique_paths: 0,
                    operations_by_type: BTreeMap::new(),
                },
            },
            trace_integrity: TraceIntegrity {
                ring_buffer_overflows: 0,
                events_dropped: 0,
                uprobe_attach_failures: vec![],
                kprobe_attach_failures: vec![],
                partial_captures: vec![],
                bloom_filter_capacity: 65536,
                bloom_filter_false_positive_rate: 0.01,
                filter_categories_applied: vec![],
            },
        };

        let cfg = AttestationConfig {
            target_pid: 1234,
            target_command: "cargo build".to_string(),
            cgroup_id: 999,
            subject_name: "test-output".to_string(),
            subject_digest: None,
            subject_resolver: None,
        };

        let stmt = build_attestation(
            trace,
            &cfg,
            Timestamp::now(),
            Timestamp::now(),
            None, // m210 compiler-pipeline — test doesn't exercise it
        )
        .expect("should build attestation");

        assert_eq!(stmt.statement_type, InTotoStatement::STATEMENT_TYPE);
        assert_eq!(stmt.predicate_type, InTotoStatement::PREDICATE_TYPE);
        assert_eq!(stmt.predicate.metadata.tool.name, "mikebom");
        assert_eq!(
            stmt.predicate.metadata.generation_context,
            GenerationContext::BuildTimeTrace
        );
    }
}
