//! Milestone 210 (compiler-pipeline eBPF tracing) — data-model
//! entities E1–E6 + E14 supporting types.
//!
//! Public types populated by the user-space aggregator at
//! `waybill-cli/src/trace/compiler_pipeline.rs` (m210 tasks
//! T022–T024) and consumed by:
//!
//! - `BuildTracePredicate.compiler_pipeline` (statement.rs E7) —
//!   native attestation shape
//! - `witness::build_witness_collection` (m210 T039) — emits the
//!   `compiler-invocation/v0.1` attestor entry per
//!   `contracts/attestor-predicate.md` C-1
//! - The three per-format emitters at
//!   `waybill-cli/src/generate/{cyclonedx,spdx}/*.rs` — emit the
//!   `waybill:source-read-set` per-component annotation (C130)
//!   per Clarifications Q1 mapping
//!
//! Design decisions locked in `specs/210-compiler-pipeline-trace/research.md`:
//!
//! - **R1**: `sched_process_exec` tracepoint (populates
//!   `CompilerExecEvent` in-kernel; user-space consumes via
//!   `COMPILER_EXEC_EVENTS` ring buffer).
//! - **R6**: `CompilerPipelineData` rides an ADDITIVE `Option<>`
//!   field on `BuildTracePredicate` (E7) — pre-m210 consumers
//!   see it as `None`, preserving JSON backward compatibility.
//! - **R8**: deterministic ordering — `invocations` sorted by
//!   `(start_timestamp, pid)`; `read_set` + `write_set` sorted
//!   by path lex; `dag_edges` sorted by `(parent, child)`;
//!   `filter_categories_applied` sorted lex.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::hash::ContentHash;
use crate::types::timestamp::Timestamp;

/// The compiler family a `CompilerInvocation` was matched to. See
/// data-model E3. Emitted verbatim in the wire shape per
/// `contracts/attestor-predicate.md` C-2.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilerFamily {
    Rustc,
    Gcc,
    Clang,
    /// `g++`
    Gpp,
    /// `go build` OR `go tool compile`
    Go,
    Ld,
    Mold,
    /// GCC's `cc1` internal
    Cc1,
    /// C preprocessor
    Cpp,
    As,
    /// Basename matched the comm-field prefilter but argv[0] didn't
    /// map to a known family. User-space heuristic + logs a warning.
    Unknown,
}

/// Classifies a `ReadSetEntry` per data-model E4 + FR-018.
/// Most reads are `File`; `StdinInput` handles the `gcc -x c -`
/// pattern where a compiler consumes source from stdin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadKind {
    File,
    /// FR-018 — bytes-read counter substitutes for path + content
    /// hash when the input came from stdin.
    StdinInput { bytes_read: u64 },
}

/// One entry in a `CompilerInvocation.read_set`. See data-model E4.
///
/// For `kind == StdinInput`, the `path` is a synthetic `"<stdin>"`
/// marker and the `sha256` is a sentinel zero-value hash (real bytes
/// don't get captured in MVP per FR-018).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadSetEntry {
    pub path: PathBuf,
    pub sha256: ContentHash,
    pub kind: ReadKind,
}

/// One entry in a `CompilerInvocation.write_set`. See data-model E5.
///
/// `sha256` is `None` when the file was deleted between close-time
/// and trace-end (typical for build intermediates like `*.o` files
/// that get consumed by the linker then removed). `survived_trace_window`
/// captures the same distinction more directly and is preserved
/// alongside `sha256` for downstream consumers that want the boolean
/// flag without null-check gymnastics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WriteSetEntry {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<ContentHash>,
    pub survived_trace_window: bool,
}

/// A single traced compiler invocation. See data-model E2.
///
/// One instance per `execve` of a whitelisted compiler binary.
/// Uniquely identified by `invocation_id` (monotonically-increasing
/// per-scan). `parent_invocation_id.is_none()` iff this is a root of
/// the compiler-invocation DAG (e.g., `cargo` before it spawns
/// `rustc` children).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompilerInvocation {
    pub invocation_id: u64,
    pub compiler: CompilerFamily,
    pub pid: u32,
    pub ppid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_invocation_id: Option<u64>,
    pub cgroup_id: u64,
    pub start_timestamp: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_timestamp: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argv_full_path: Option<PathBuf>,
    pub argv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub read_set: Vec<ReadSetEntry>,
    pub write_set: Vec<WriteSetEntry>,
    pub events_dropped: u64,
}

/// Parent-child linkage in the compiler-invocation DAG.
/// See data-model E6. Redundant with `CompilerInvocation.parent_invocation_id`
/// (either alone suffices); we emit both for consumer convenience per
/// `contracts/attestor-predicate.md` C-5.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationDagEdge {
    pub parent_invocation_id: u64,
    pub child_invocation_id: u64,
}

/// Per-scan pipeline-completeness signal. Drives the doc-scope
/// `waybill:compiler-pipeline-completeness` annotation (C132) per
/// FR-008 + FR-017 + contracts/annotations.md A-3.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CompletenessState {
    Complete,
    Degraded {
        dropped: u64,
        affected_component_count: usize,
    },
    Partial {
        reason: PartialReason,
    },
}

/// Why the trace is `Partial`. Currently one variant; extended
/// via serde-additive semantics if future partial-completion
/// modes emerge.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartialReason {
    /// FR-017 — trace attached AFTER one or more compilers had
    /// started; early events preceding attach are missing.
    AttachLate,
}

/// Categorizes which FR-016 filter groups produced drops during
/// this scan. Emitted in `CompilerPipelineData.filter_categories_applied`
/// for auditability (operator can see WHICH filters actually fired).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCategory {
    /// `/etc/`, `/proc/`, `/sys/`, `/dev/`
    System,
    /// `~/.cache/`, `~/.local/share/`
    UserCache,
    /// `/tmp/`, `/var/tmp/`, `$TMPDIR/`
    Ephemeral,
    /// Per Clarifications Q2 + FR-016 secrets category:
    /// `/var/run/secrets/`, `/run/keys/`, `~/.ssh/`, `~/.aws/`,
    /// `~/.gnupg/`, `~/.docker/config.json`, `~/.netrc`, `~/.kube/config`,
    /// plus `.pem` / `.key` / `.crt` / `_rsa` / `_ed25519` basename
    /// heuristic.
    SecretsAdjacent,
}

/// Root record for the compiler-pipeline data captured during a
/// build trace. See data-model E6. Threads into
/// `BuildTracePredicate.compiler_pipeline` as an additive
/// `Option<>` field per research R6.
///
/// `invocations` MUST be sorted by `(start_timestamp, pid)` per R8.
/// `dag_edges` MUST be sorted by `(parent, child)` per R8.
/// `filter_categories_applied` MUST be sorted lex per R8.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompilerPipelineData {
    pub invocations: Vec<CompilerInvocation>,
    pub dag_edges: Vec<InvocationDagEdge>,
    pub completeness: CompletenessState,
    pub secrets_read_filtered: u64,
    pub include_system_reads_flag: bool,
    pub filter_categories_applied: Vec<FilterCategory>,
}

impl CompilerPipelineData {
    /// The waybill-owned URI for the `compiler-invocation/v0.1`
    /// witness attestor entry. Locked per Clarifications Q3 +
    /// `contracts/attestor-predicate.md` C-1. Future version bumps
    /// (v0.2, v1, etc.) MUST retain the `/compiler-invocation/` path
    /// prefix.
    pub const PREDICATE_TYPE: &'static str =
        "https://waybill.dev/attestation/compiler-invocation/v0.1";
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::types::hash::HashAlgorithm;

    fn zero_sha256() -> ContentHash {
        ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: crate::types::hash::HexString::new(&"0".repeat(64)).unwrap(),
        }
    }

    #[test]
    fn compiler_family_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(CompilerFamily::Rustc).unwrap(),
            serde_json::json!("rustc")
        );
        assert_eq!(
            serde_json::to_value(CompilerFamily::Gpp).unwrap(),
            serde_json::json!("gpp")
        );
        assert_eq!(
            serde_json::to_value(CompilerFamily::Unknown).unwrap(),
            serde_json::json!("unknown")
        );
    }

    #[test]
    fn read_kind_stdin_input_carries_bytes_read() {
        let k = ReadKind::StdinInput { bytes_read: 1234 };
        let v = serde_json::to_value(&k).unwrap();
        assert_eq!(v, serde_json::json!({ "stdin_input": { "bytes_read": 1234 } }));
    }

    #[test]
    fn read_kind_file_is_bare_snake_case_variant() {
        let v = serde_json::to_value(ReadKind::File).unwrap();
        assert_eq!(v, serde_json::json!("file"));
    }

    #[test]
    fn completeness_state_serde_tag() {
        let v = serde_json::to_value(CompletenessState::Complete).unwrap();
        assert_eq!(v, serde_json::json!({ "state": "complete" }));

        let v = serde_json::to_value(CompletenessState::Degraded {
            dropped: 5,
            affected_component_count: 2,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "state": "degraded",
                "dropped": 5,
                "affected_component_count": 2,
            })
        );

        let v = serde_json::to_value(CompletenessState::Partial {
            reason: PartialReason::AttachLate,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "state": "partial",
                "reason": "attach_late",
            })
        );
    }

    #[test]
    fn compiler_pipeline_data_empty_round_trips_through_json() {
        let d = CompilerPipelineData {
            invocations: Vec::new(),
            dag_edges: Vec::new(),
            completeness: CompletenessState::Complete,
            secrets_read_filtered: 0,
            include_system_reads_flag: false,
            filter_categories_applied: Vec::new(),
        };
        let json = serde_json::to_value(&d).unwrap();
        let back: CompilerPipelineData = serde_json::from_value(json).unwrap();
        assert_eq!(back.invocations.len(), 0);
        assert_eq!(back.completeness, CompletenessState::Complete);
    }

    #[test]
    fn compiler_invocation_omits_none_fields() {
        let inv = CompilerInvocation {
            invocation_id: 1,
            compiler: CompilerFamily::Rustc,
            pid: 100,
            ppid: 50,
            parent_invocation_id: None,
            cgroup_id: 42,
            start_timestamp: Timestamp::now(),
            end_timestamp: None,
            argv_full_path: None,
            argv: vec!["rustc".into(), "--edition=2021".into()],
            cwd: None,
            exit_code: None,
            read_set: Vec::new(),
            write_set: Vec::new(),
            events_dropped: 0,
        };
        let json = serde_json::to_string(&inv).unwrap();
        // None-valued fields should be absent from the wire form
        // (skip_serializing_if = "Option::is_none") — matches C-4
        // backwards-compatibility contract.
        assert!(!json.contains("parent_invocation_id"), "{json}");
        assert!(!json.contains("end_timestamp"), "{json}");
        assert!(!json.contains("argv_full_path"), "{json}");
        assert!(!json.contains("cwd"), "{json}");
        assert!(!json.contains("exit_code"), "{json}");
    }

    #[test]
    fn write_set_entry_omits_none_sha256() {
        let e = WriteSetEntry {
            path: PathBuf::from("/tmp/foo.o"),
            sha256: None,
            survived_trace_window: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("sha256"), "{json}");
        assert!(json.contains("survived_trace_window"), "{json}");
    }

    #[test]
    fn read_set_entry_carries_hash_when_file_kind() {
        let e = ReadSetEntry {
            path: PathBuf::from("/src/main.rs"),
            sha256: zero_sha256(),
            kind: ReadKind::File,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["path"], "/src/main.rs");
        assert_eq!(json["kind"], "file");
        assert!(json["sha256"].is_object());
    }

    #[test]
    fn invocation_dag_edge_round_trips() {
        let e = InvocationDagEdge {
            parent_invocation_id: 10,
            child_invocation_id: 25,
        };
        let json = serde_json::to_value(&e).unwrap();
        let back: InvocationDagEdge = serde_json::from_value(json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn filter_category_serializes_snake_case_and_sorts() {
        let mut cats = vec![
            FilterCategory::SecretsAdjacent,
            FilterCategory::System,
            FilterCategory::Ephemeral,
        ];
        cats.sort();
        let json = serde_json::to_value(&cats).unwrap();
        // Sort order per Ord derivation: System (declared first) <
        // UserCache < Ephemeral < SecretsAdjacent.
        assert_eq!(
            json,
            serde_json::json!(["system", "ephemeral", "secrets_adjacent"])
        );
    }

    #[test]
    fn predicate_type_uri_is_mikebom_owned_per_q3() {
        // Contract lock — this URI is documented in
        // contracts/attestor-predicate.md C-1 and MUST NOT change
        // without a URI bump (v0.2, etc.).
        assert_eq!(
            CompilerPipelineData::PREDICATE_TYPE,
            "https://waybill.dev/attestation/compiler-invocation/v0.1"
        );
    }
}
