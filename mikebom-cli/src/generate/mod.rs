//! SBOM output generation — format dispatch layer (milestone 010).
//!
//! The [`SbomSerializer`] trait is the sole extension point for
//! adding a new SBOM output format. Every concrete emitter
//! ([`cyclonedx::CycloneDxJsonSerializer`] today; SPDX 2.3 +
//! SPDX 3.0.1 stub + OpenVEX sidecar land in later phases of this
//! milestone) consumes a neutral [`ScanArtifacts`] bundle and a shared
//! [`OutputConfig`] and returns one or more [`EmittedArtifact`] byte
//! buffers — the CLI layer owns filesystem placement.
//!
//! Per feature 010 FR-019, adding a future format (or extending the
//! SPDX 3 stub to more ecosystems) is a single-line registration in
//! [`SerializerRegistry::with_defaults`] plus a new module; the scan,
//! resolution, and other format implementations do not have to change.
//!
//! Determinism contract (data-model.md §8):
//!   - serializers MUST be pure functions of `(scan, cfg)`;
//!   - [`OutputConfig::created`] is the single timestamp source
//!     shared across every format emitted in one invocation;
//!   - any `HashMap` use is forbidden on the serialization path —
//!     use `BTreeMap` or an explicitly sorted `Vec`.

pub mod cpe;
pub mod cyclonedx;
pub mod lifecycle_phases;
pub mod openvex;
pub mod spdx;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use mikebom_common::attestation::integrity::TraceIntegrity;
use mikebom_common::attestation::metadata::GenerationContext;
use mikebom_common::resolution::{Relationship, ResolvedComponent};

/// Format-neutral bundle of everything a serializer might consume.
///
/// Mirrors the inputs the existing
/// [`cyclonedx::builder::CycloneDxBuilder::build`] has always taken,
/// so the CDX refactor behind [`SbomSerializer`] does not need to
/// change its output bytes — the load-bearing protection for
/// FR-022 / SC-006.
pub struct ScanArtifacts<'a> {
    pub target_name: &'a str,
    pub components: &'a [ResolvedComponent],
    pub relationships: &'a [Relationship],
    pub integrity: &'a TraceIntegrity,
    pub complete_ecosystems: &'a [String],
    pub os_release_missing_fields: &'a [String],
    pub scan_target_coord:
        Option<&'a crate::scan_fs::package_db::maven::ScanTargetCoord>,
    pub generation_context: GenerationContext,
    pub include_dev: bool,
    pub include_hashes: bool,
    pub include_source_files: bool,
    /// Document-level scope mode. Resolved from
    /// `--include-declared-deps` (with the `--path`/`--image`
    /// auto-default rule). Surfaced in CDX `metadata.lifecycles[]`
    /// (component-derived, indirect) and SPDX
    /// `creationInfo.comment` / `SpdxDocument.comment` (direct).
    /// Milestone 047.
    pub scope_mode: ScopeMode,
    /// Milestone 061 (closes #119): doc-level Go graph-completeness
    /// signal. `None` when no Go scan happened (annotation absent in
    /// output). `Some(Complete)` / `Some(Partial)` per the per-scan
    /// orphan classification done by `golang::legacy::read()`.
    pub go_graph_completeness:
        Option<crate::scan_fs::package_db::GraphCompleteness>,
    /// Milestone 061 — comma-separated `<ecosystem>:<reason-class>`
    /// list summarizing why `go_graph_completeness == Partial`.
    /// Empty/None when completeness is `Complete` or `None`.
    pub go_graph_completeness_reason: Option<&'a str>,
    /// Milestone 072 / T010-T014: when the scan was invoked with
    /// `--bind-to-source <path>` AND the source SBOM was loaded
    /// successfully, this field carries the source SBOM's stable
    /// identifier (SHA-256 + optional IRI). Each format's metadata
    /// builder emits a standards-native cross-document reference
    /// (CDX `metadata.component.externalReferences[type:bom]`,
    /// SPDX 2.3 `externalDocumentRefs` + `BUILT_FROM` relationship,
    /// SPDX 3 `import[]` ExternalMap + `Relationship[built_from]`)
    /// when populated. Per `contracts/source-document-binding-annotation.md`
    /// C-2; per Constitution Principle V (standards-native first).
    /// `None` for every pre-072 / non-bind-to-source scan.
    pub source_document_binding: Option<&'a mikebom::binding::SourceDocumentId>,
    /// Milestone 073: identifiers attached at scan invocation
    /// (auto-detected `repo:` / `image:` plus manual flags
    /// `--repo` / `--git-ref` / `--image` / `--attestation` / `--id
    /// <scheme>=<value>`). Auto-detected entries appear FIRST in the
    /// Vec; manual entries follow in supply order, with the
    /// override-position rule applied (manual entries that
    /// deduplicate against auto-detected entries on `(scheme, value)`
    /// inherit the auto-detected entry's position) per FR-009.
    /// Already deduplicated by `(scheme, value)` pre-emit. Built-in
    /// identifiers ride per-format standards-native carriers (CDX
    /// `metadata.component.externalReferences[]`, SPDX 2.3
    /// dual-carrier on main-module `Package.externalRefs[
    /// PERSISTENT-ID]` + `creationInfo.creators` text, SPDX 3
    /// `Element.externalIdentifier[]`). User-defined identifiers
    /// ride the `mikebom:identifiers` annotation envelope
    /// (parity-catalog row C47); SPDX 3 also carries them natively
    /// in `Element.externalIdentifier[]` per
    /// `contracts/identifiers-annotation.md` C-1.
    pub identifiers:
        &'a [mikebom::binding::identifiers::Identifier],
    /// Milestone 076: per-component user-defined identifiers from
    /// `--component-id <PURL>=<scheme>:<value>` flags. Threaded to
    /// per-format emitters which match `selector_purl` byte-equally
    /// against emitted `components[].purl` and append the identifier
    /// to every match in the per-format native carrier (CDX
    /// `components[].properties[]`, SPDX 2.3
    /// `Package.externalRefs[PERSISTENT-ID]`, SPDX 3
    /// `Element.externalIdentifier[]`). Emission is deterministic per
    /// FR-012 — pre-existing entries preserve their original
    /// positions; new per-component identifier entries append after
    /// in lexical order by `(scheme, value)`. Built-in scheme names
    /// (`repo`, `git`, `image`, `attestation`, `subject`) are rejected
    /// at CLI parse time per FR-009. Default empty for callers not
    /// using the flag — backwards-compatible.
    pub component_identifiers:
        &'a [mikebom::binding::identifiers::component_id::ComponentIdentifierFlag],
}

/// Document-level scope mode for a single mikebom scan. Surfaced
/// in SPDX 2.3 `creationInfo.comment` and SPDX 3
/// `SpdxDocument.comment` so consumers reading metadata-only know
/// whether the document represents on-disk-only emission
/// (`Artifact`) or includes declared transitives that may not be
/// on disk yet (`Manifest`).
///
/// The value derives from the resolution of
/// `--include-declared-deps`: when that flag resolves true (the
/// default for `--path` scans), the scan is `Manifest`; when
/// false (the default for `--image` scans), the scan is
/// `Artifact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeMode {
    /// On-disk components only — every emitted component has its
    /// bytes physically present in the scanned tree or image.
    /// Default for `--image`. CDX phase aggregation typically
    /// shows `operations` (deployed runtime) plus whatever
    /// build-time tiers happen to be present in installed
    /// packages.
    Artifact,
    /// On-disk components plus declared-but-not-on-disk
    /// transitives (lockfile-pinned but absent from local
    /// caches, deps.dev-resolved, Maven cache-miss BFS, etc.).
    /// Default for `--path` scans (source trees) so SBOM
    /// consumers get the full "what would this build pull in"
    /// view.
    Manifest,
}

/// Per-invocation configuration threaded through every serializer.
///
/// `created` is the single timestamp source used for any `timestamp`
/// / `creationInfo.created` / `annotationDate` field in any format —
/// serializers MUST NOT call `Utc::now()` directly. `overrides` is
/// the per-format output-path map built by the CLI layer from
/// `--output <fmt>=<path>` flags.
///
/// Note: today's [`cyclonedx::CycloneDxJsonSerializer`] does not
/// consume these fields — pre-milestone-010 CDX output uses its own
/// internal `Utc::now()` + `Uuid::new_v4()` to preserve byte-identity
/// (FR-022 / SC-006). SPDX 2.3, SPDX 3.0.1-experimental, and the
/// OpenVEX sidecar all consume them in later phases of this milestone.
#[allow(dead_code)]
pub struct OutputConfig {
    pub mikebom_version: &'static str,
    pub created: DateTime<Utc>,
    pub overrides: BTreeMap<String, PathBuf>,
}

/// One serialized file produced by a serializer.
///
/// Multi-artifact returns let a single serializer emit a primary
/// document plus side artifacts — e.g. the SPDX 2.3 emitter co-emits
/// the OpenVEX sidecar when a scan produces VEX, with the
/// cross-reference baked into the primary doc.
pub struct EmittedArtifact {
    /// Suggested filename relative to the output root. The CLI layer
    /// uses this when the user did not pass a `--output <fmt>=<path>`
    /// override for this format.
    pub relative_path: PathBuf,
    pub bytes: Vec<u8>,
}

/// One concrete SBOM output format.
pub trait SbomSerializer: Send + Sync {
    /// Stable identifier matching the CLI `--format` value (e.g.
    /// `"cyclonedx-json"`). Returned strings are compared case-sensitive.
    fn id(&self) -> &'static str;

    /// Default output filename when no per-format `--output` override
    /// is set. Distinct per format, so default paths never collide.
    fn default_filename(&self) -> &'static str;

    /// Whether this serializer is labeled experimental (FR-019b).
    fn experimental(&self) -> bool {
        false
    }

    /// Serialize a scan result into one or more output artifacts.
    fn serialize(
        &self,
        artifacts: &ScanArtifacts<'_>,
        cfg: &OutputConfig,
    ) -> anyhow::Result<Vec<EmittedArtifact>>;
}

/// Registry of every SBOM output format the CLI can dispatch to.
///
/// [`with_defaults`](Self::with_defaults) is the single registration
/// site for built-in serializers (FR-019). Adding a new format in a
/// future milestone is a one-line insertion here plus the serializer
/// implementation.
pub struct SerializerRegistry {
    by_id: BTreeMap<&'static str, Arc<dyn SbomSerializer>>,
}

impl SerializerRegistry {
    /// Register every built-in serializer: three stable formats
    /// (`cyclonedx-json`, `spdx-2.3-json`, `spdx-3-json`) plus the
    /// deprecation alias `spdx-3-json-experimental` that delegates
    /// verbatim to the stable SPDX 3 serializer (research.md §R6).
    ///
    /// The `experimental()` flag is surfaced in the CLI's `--help`
    /// text via `SbomSerializer::experimental()`. During milestone
    /// 011 Phase 2 (foundational) both SPDX 3 entries return
    /// `experimental() = true`; the flag flips to `false` in US3
    /// (T029 stable / T030 alias) once full parity is achieved.
    pub fn with_defaults() -> Self {
        let mut by_id: BTreeMap<&'static str, Arc<dyn SbomSerializer>> =
            BTreeMap::new();
        let cdx: Arc<dyn SbomSerializer> =
            Arc::new(cyclonedx::CycloneDxJsonSerializer);
        by_id.insert(cdx.id(), cdx);
        let spdx23: Arc<dyn SbomSerializer> =
            Arc::new(spdx::Spdx2_3JsonSerializer);
        by_id.insert(spdx23.id(), spdx23);
        let spdx3: Arc<dyn SbomSerializer> = Arc::new(spdx::Spdx3JsonSerializer);
        by_id.insert(spdx3.id(), spdx3);
        let spdx3_alias: Arc<dyn SbomSerializer> =
            Arc::new(spdx::Spdx3JsonExperimentalSerializer);
        by_id.insert(spdx3_alias.id(), spdx3_alias);
        Self { by_id }
    }

    /// Iterator over every registered format id, in deterministic order.
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.by_id.keys().copied()
    }

    /// Look up one serializer by format id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn SbomSerializer>> {
        self.by_id.get(id).cloned()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_cyclonedx_json() {
        let reg = SerializerRegistry::with_defaults();
        let ids: Vec<&str> = reg.ids().collect();
        assert!(
            ids.contains(&"cyclonedx-json"),
            "default registry must include cyclonedx-json, got {ids:?}"
        );
        let s = reg.get("cyclonedx-json").expect("cyclonedx-json registered");
        assert_eq!(s.id(), "cyclonedx-json");
        assert_eq!(s.default_filename(), "mikebom.cdx.json");
        assert!(!s.experimental());
    }

    #[test]
    fn unknown_id_returns_none() {
        let reg = SerializerRegistry::with_defaults();
        assert!(reg.get("not-a-real-format").is_none());
    }

    #[test]
    fn ids_are_in_deterministic_order() {
        // Two independent registries must iterate identically.
        let a: Vec<&str> = SerializerRegistry::with_defaults().ids().collect();
        let b: Vec<&str> = SerializerRegistry::with_defaults().ids().collect();
        assert_eq!(a, b);
    }
}
