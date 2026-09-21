//! Milestone 924 (#932) — the emitted document's shape.
//!
//! Field names here are the published contract
//! (`specs/924-repo-observation-report/contracts/report-schema.md`). Changing
//! one is a **major** schema bump, and so is changing what an existing field
//! means while leaving its name and type intact — the latter being the change
//! nothing mechanical catches (contract C-2).

use serde::Serialize;

/// Two-part version (FR-017a). Minor is additive; major means a field was
/// removed, renamed, or had its type or meaning changed.
#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct SchemaVersion {
    pub(crate) major: u16,
    pub(crate) minor: u16,
}

/// The version this build emits. Bump deliberately, per contract C-2.
pub(crate) const SCHEMA_VERSION: SchemaVersion = SchemaVersion { major: 0, minor: 1 };

/// FR-019c — always present, in both modes, so a reader never has to infer
/// whether an absent name was absent or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RedactionMode {
    /// Repository-relative paths retained verbatim (FR-019a, the default).
    None,
    /// Path segments replaced by stable identifiers (FR-019b).
    Paths,
}

/// FR-012a — **exclusive**. Exactly one applies to every recorded directory.
///
/// Note what is *not* here: `indeterminate`. Ambiguity is a separate,
/// independent field (FR-012b), because a directory can be claimed **and**
/// ambiguous — which is precisely this repository's `waybill-cli/tests/`.
/// Collapsing the two into one verdict would discard the signal this feature
/// exists to surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClaimStatus {
    Claimed,
    Unclaimed,
    ExcludedByPolicy,
}

/// FR-007 — whether waybill has a reader for an observed ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EcosystemSupport {
    Supported,
    NoReader,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EcosystemAttribution {
    pub(crate) ecosystem: String,
    /// FR-008 — an attribution MUST cite a marker file. Extensions alone never
    /// produce one.
    pub(crate) evidence_marker: String,
    pub(crate) support: EcosystemSupport,
}

/// FR-012b — independent of `ClaimStatus`, and never a substitute for it.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AmbiguityRecord {
    pub(crate) kind: String,
    /// FR-014 — at least two, and **never ranked**. One interpretation is a
    /// classification and belongs in `ecosystems` instead.
    pub(crate) interpretations: Vec<String>,
    /// FR-015 — what was observed, so a reader can evaluate the call rather
    /// than trust it.
    pub(crate) evidence: Vec<String>,
}

/// FR-011 — emitted when a directory is not confidently classified, which
/// FR-011a defines as: at least one ecosystem attribution **and** no ambiguity
/// record.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DirectoryObservationDetail {
    pub(crate) file_count: u64,
    pub(crate) max_depth: u32,
    pub(crate) extension_histogram: std::collections::BTreeMap<String, u64>,
    pub(crate) content_kind: String,
    /// The per-file sample bound. Present so a reader knows the verdict came
    /// from a sample rather than the whole file (research R4).
    pub(crate) content_sample_bytes: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DirectoryObservation {
    /// Repository-relative. Never absolute, in any mode (FR-019).
    pub(crate) path: String,
    pub(crate) claim_status: ClaimStatus,
    /// Non-empty exactly when `claim_status == Claimed`.
    pub(crate) claimed_by: Vec<String>,
    pub(crate) ecosystems: Vec<EcosystemAttribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ambiguity: Option<AmbiguityRecord>,
    pub(crate) files_direct: u64,
    /// Rolled up from descendants that earned no record of their own
    /// (FR-021b). Kept separate from `files_direct` so aggregation preserves
    /// counts without pretending the files live here.
    pub(crate) files_aggregated: u64,
    pub(crate) components_emitted: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) observation: Option<DirectoryObservationDetail>,
}

/// FR-004 — both counts, because `files_matched > 0` with
/// `components_emitted == 0` (a reader that engaged and produced nothing) and
/// `files_matched == 0` (a reader that never saw a candidate) are different
/// diagnoses with different fixes. A single number conflates them.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReaderCoverage {
    pub(crate) reader_id: String,
    pub(crate) files_matched: u64,
    pub(crate) components_emitted: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepositoryTotals {
    pub(crate) directories_walked: u64,
    pub(crate) directories_recorded: u64,
    pub(crate) files_walked: u64,
    pub(crate) files_claimed: u64,
    pub(crate) files_unclaimed: u64,
    pub(crate) files_skipped: std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ObservationReport {
    pub(crate) schema_version: SchemaVersion,
    pub(crate) schema_stability: &'static str,
    pub(crate) tool_version: String,
    pub(crate) generated_at: String,
    /// FR-020 / contract C-5 — **self-describing**. A differ reads this rather
    /// than hard-coding a list, so it stays correct as the schema grows.
    pub(crate) volatile_fields: Vec<String>,
    pub(crate) redaction_mode: RedactionMode,
    /// FR-021c. Two reports produced under different thresholds are not
    /// comparable, and a reader must never have to guess which applied.
    pub(crate) significance_threshold: u32,
    pub(crate) totals: RepositoryTotals,
    pub(crate) readers: Vec<ReaderCoverage>,
    pub(crate) directories: Vec<DirectoryObservation>,
}

/// The fields that legitimately differ between two runs of an unchanged
/// repository. Emitted into every report (FR-020).
pub(crate) fn volatile_fields() -> Vec<String> {
    vec!["tool_version".to_string(), "generated_at".to_string()]
}
