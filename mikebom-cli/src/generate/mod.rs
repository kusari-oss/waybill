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
pub mod divergence_annotation;
pub mod lifecycle_phases;
pub mod openvex;
pub mod root_selector;
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
    /// Milestone 133 US3: file-tier walker diagnostic counters.
    /// `None` when the walker didn't run (`--file-inventory=off`).
    /// `Some(_)` when `orphan` or `full` ran — each non-zero
    /// counter projects onto one `mikebom:file-inventory-skipped-*`
    /// document-level annotation per Constitution Principle X.
    pub file_inventory_stats:
        Option<&'a crate::scan_fs::file_tier::walker::WalkerStats>,
    /// Milestone 133 US4 (Constitution Strict Boundary §5):
    /// operator-supplied `--file-inventory` mode label (`"off"` /
    /// `"orphan"` / `"full"`). `Some("full")` triggers a mandatory
    /// document-level `mikebom:file-inventory-mode` annotation so
    /// consumers can detect when the FR-011 dedupe was bypassed.
    /// `None` for pre-feature scans and tests; `Some("off")` /
    /// `Some("orphan")` permit transparent passthrough without
    /// emitting the override marker (preserves byte-identity on
    /// default-mode SBOMs).
    pub file_inventory_mode: Option<&'a str>,
    /// Milestone 077: operator-supplied overrides for the root
    /// component's name + version. When `name` or `version` is
    /// `Some(_)`, the override replaces the corresponding auto-derived
    /// value in `metadata.component` (CDX) / main-module Package
    /// (SPDX 2.3) / root element (SPDX 3). When both are `None`, the
    /// existing auto-derivation flow runs unchanged (byte-identical to
    /// alpha.17 per FR-009). Default `RootComponentOverride::default()`
    /// keeps existing struct-literal call sites compiling.
    pub root_override: RootComponentOverride,
    /// Milestone 080: user-provided SBOM metadata aggregated from the
    /// `--creator` / `--annotator` / `--annotation-comment` /
    /// `--metadata-comment` / `--scan-target-name` / `--metadata-file`
    /// flags. Per-format builders consume `&UserMetadata` verbatim and
    /// route each entry to the format's standards-native landing slot
    /// (CDX 1.6 `bom.annotations[]` + `metadata.tools.components[]` +
    /// `metadata.authors[]` + `metadata.manufacturer`; SPDX 2.3
    /// `creationInfo.creators[]` + `creationInfo.comment` +
    /// `annotations[]`; SPDX 3 new `Tool` / `Organization` / `Person`
    /// / `Annotation` elements in `@graph`). When `is_active()` is
    /// false, builders short-circuit so pre-080 invocations stay
    /// byte-identical to alpha.20.
    pub user_metadata: mikebom::binding::user_metadata::UserMetadata,
    /// Milestone 081: operator-asserted CISA SBOM Type from the
    /// `--sbom-type <type>` flag. When `Some(_)`, all three formats'
    /// document-level lifecycle aggregations collapse to a single-
    /// element output corresponding to the asserted value (CDX
    /// `metadata.lifecycles[{phase: "<cdx-phase>"}]`, SPDX 2.3
    /// `creationInfo.comment` "Observed lifecycle phases:
    /// <single-phase>", SPDX 3 `software_Sbom.software_sbomType:
    /// ["<short-name>"]`). When `None`, the milestone-047
    /// per-component aggregation continues unchanged. Per-component
    /// `mikebom:sbom-tier` annotations are NEVER overridden — the
    /// operator-assert is document-level only per research §4 +
    /// FR-005 + VR-081-005. Default `None` keeps existing
    /// struct-literal call sites compiling.
    pub sbom_type_override:
        Option<crate::generate::lifecycle_phases::SbomType>,
    /// Issue #228 — selects the SPDX 2.3 relationship-type vocabulary
    /// the emitter uses for scoped dependency edges (dev / build /
    /// test). Both modes are spec-conformant; the flag exists because
    /// some downstream SBOM consumers only implement the basic
    /// relationship vocabulary (`DEPENDS_ON` / `CONTAINS` /
    /// `DESCRIBES`) and ignore the typed scoped variants. `Full`
    /// (default) emits the spec-native typed reversed-direction
    /// variants (`DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` /
    /// `TEST_DEPENDENCY_OF`). `Basic` collapses every dep — runtime,
    /// dev, build, test — into natural-direction `DEPENDS_ON` for
    /// compatibility with the basic-vocabulary consumer set. Scope
    /// info still rides on the target Package's
    /// `mikebom:lifecycle-scope` annotation in both modes. CDX and
    /// SPDX 3 are unaffected. See
    /// `docs/reference/sbom-format-mapping.md` C42 + the
    /// `--spdx2-relationship-compat` CLI flag.
    pub spdx2_relationship_compat: Spdx2RelationshipCompat,
    /// Milestone 134 (closes #125): document-scope aggregate of every
    /// divergent-PURL collision detected in the scan. `None` when no
    /// divergence was detected — emitters MUST omit the
    /// `mikebom:purl-collisions-detected` annotation entirely in
    /// that case (FR-009: no SBOM bloat, no spurious signal on
    /// clean scans). `Some(_)` triggers a document-level annotation
    /// in CDX `metadata.properties[]`, SPDX 2.3 top-level
    /// `annotations[]`, and SPDX 3 `SpdxDocument` element-level
    /// `annotations` via the standard envelope plumbing.
    pub collisions_summary:
        Option<&'a mikebom_common::divergence::CollisionsSummary>,
}

/// Milestone 077 — operator-supplied overrides for the root component
/// identity. See `ScanArtifacts::root_override`.
///
/// When `is_active()` returns true, per-format builders MUST:
/// 1. Replace the auto-derived root component name/version with the
///    override values (where each is `Some(_)`); the unset half falls
///    through to the existing auto-derivation.
/// 2. Filter manifest-derived main-module components (identified by
///    `mikebom:component-role = main-module`) from the emitted
///    `components[]` array per the 2026-05-06 clean-replacement
///    clarification (Q2). The future demote-to-library follow-up is
///    tracked as GitHub issue #151.
#[derive(Debug, Clone, Default)]
pub struct RootComponentOverride {
    /// When `Some(name)`, replaces the auto-derived
    /// `metadata.component.name` (CDX) / main-module `Package.name`
    /// (SPDX 2.3) / root element name (SPDX 3) with `name`. Validated
    /// at CLI parse per VR-077-001.
    pub name: Option<String>,
    /// When `Some(version)`, replaces the auto-derived version field
    /// across all three formats. Validated at CLI parse per VR-077-001.
    pub version: Option<String>,
    /// Override the type segment of the root component's PURL.
    /// `None` keeps the default `generic`. `Some("golang")` produces
    /// `pkg:golang/<name>@<version>` instead of `pkg:generic/...`.
    /// Validated at CLI parse against the purl-spec type charset
    /// (`^[a-z][a-z0-9.+-]*$`). REQUIRES `name` to be `Some`.
    /// Mutually exclusive with `omit_purl`.
    pub purl_type: Option<String>,
    /// When `true`, the root component is emitted WITHOUT a PURL at
    /// all. CDX: `metadata.component.purl` field absent. SPDX 2.3: no
    /// `purl` entry in the root Package's `externalRefs[]`. SPDX 3:
    /// no `software_packageUrl` AND no `externalIdentifier[]` entry
    /// with `externalIdentifierType: "packageUrl"`. REQUIRES `name`
    /// to be `Some`. Mutually exclusive with `purl_type`.
    pub omit_purl: bool,
    /// Issue #359 — operator-supplied full PURL string. When `Some(_)`,
    /// it WINS over every other field on this struct:
    /// `build_subject_purl` returns the value verbatim, and the BOM
    /// subject's name/version are parsed from the PURL itself (the
    /// `Purl` newtype validates at construction time, so the parse
    /// can't fail at emission). Mutually exclusive at the CLI layer
    /// with `name`/`version`/`purl_type`/`omit_purl` (clap
    /// `conflicts_with`) so combinations of "full PURL" + "override
    /// one piece of the PURL" can't reach the emission code.
    pub full_purl: Option<String>,
    /// Name parsed out of `full_purl` at CLI-flag construction time.
    /// Surfaces in `metadata.component.name` (CDX) / root
    /// `Package.name` (SPDX 2.3) / root element name (SPDX 3). Set
    /// only when `full_purl` is `Some(_)`.
    pub full_purl_name: Option<String>,
    /// Version parsed out of `full_purl` at CLI-flag construction
    /// time. Surfaces in `metadata.component.version` / `versionInfo`
    /// / `software_packageVersion`. Set only when `full_purl` is
    /// `Some(_)`.
    pub full_purl_version: Option<String>,
}

impl RootComponentOverride {
    /// Returns true iff at least one field is set. Used by per-format
    /// builders to decide whether to filter manifest-derived main-
    /// module components from the emitted `components[]` array per
    /// the 2026-05-06 clean-replacement clarification.
    pub fn is_active(&self) -> bool {
        self.name.is_some()
            || self.version.is_some()
            || self.purl_type.is_some()
            || self.omit_purl
            || self.full_purl.is_some()
    }

    /// Build the PURL string for the BOM subject given the resolved
    /// name + version. Issue #359 takes precedence: when `full_purl`
    /// is `Some(_)` the operator's verbatim PURL string is returned
    /// (already validated by `Purl::new` at CLI parse time, so
    /// downstream emission can trust it). Otherwise the existing
    /// milestone-077 behavior applies — `None` for `omit_purl`,
    /// `pkg:<type>/<name>@<version>` for everything else, with name +
    /// version percent-encoded per RFC 3986.
    pub fn build_subject_purl(&self, name: &str, version: &str) -> Option<String> {
        if let Some(full) = &self.full_purl {
            return Some(full.clone());
        }
        if self.omit_purl {
            return None;
        }
        let type_token = self.purl_type.as_deref().unwrap_or("generic");
        Some(format!(
            "pkg:{type_token}/{}@{}",
            percent_encode_purl_name(name),
            percent_encode_purl_name(version),
        ))
    }

    /// Issue #359 — resolved BOM-subject name. Returns the
    /// PURL-parsed name when `full_purl` is set; otherwise falls back
    /// to the discrete `name` override; otherwise `None` (caller uses
    /// the auto-derived name). Callers MUST prefer this over reading
    /// `self.name` directly so the new flag's parsed name is honored.
    pub fn resolved_name(&self) -> Option<&str> {
        if let Some(n) = self.full_purl_name.as_deref() {
            return Some(n);
        }
        self.name.as_deref()
    }

    /// Issue #359 — resolved BOM-subject version. See
    /// [`Self::resolved_name`] for the precedence contract.
    pub fn resolved_version(&self) -> Option<&str> {
        if let Some(v) = self.full_purl_version.as_deref() {
            return Some(v);
        }
        self.version.as_deref()
    }
}

/// Milestone 077 — RFC 3986 percent-encoding for the PURL `name`
/// segment when the operator-supplied `--root-name` / `--root-version`
/// override is in play.
///
/// Per RFC 3986 §2.3 (Unreserved Characters), preserves
/// `[A-Za-z0-9._~-]` verbatim and percent-encodes everything else
/// (UTF-8-aware: non-ASCII characters expand to multi-byte
/// percent-encoded runs of `%XX` per RFC 3986 §2.5).
///
/// This helper is **only** used on the override-active emission path.
/// Non-override paths continue to use `encode_purl_segment` (CDX) or
/// `url_friendly` (SPDX 3) to preserve byte-identical alpha.17 output
/// per FR-009 / SC-002 / SC-010. Per research §1, the existing helpers
/// are not refactored to use percent-encoding because consolidating
/// would risk regressing existing fixture goldens.
pub fn percent_encode_purl_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        let is_unreserved = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if is_unreserved {
            out.push(byte as char);
        } else {
            // Uppercase hex per RFC 3986 §2.1 ("uppercase letters
            // SHOULD be used"); matches the CDX `encode_purl_segment`
            // helper's case convention.
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Issue #228 — SPDX 2.3 relationship-vocabulary compatibility
/// selector. Both modes are spec-conformant, but they are not
/// equivalent: `Full` (default) preserves more information than
/// `Basic`. Per Constitution Principle X (Transparency), mikebom
/// defaults to the spec-native mechanism that carries the most
/// consumer-actionable signal, and the SPDX 2.3 spec defines the
/// typed scoped relationship variants for exactly the purpose of
/// expressing dev/build/test scope on a dependency edge. Choosing
/// `Basic` is a deliberate downshift — accept the information loss
/// when targeting consumers that don't implement those variants.
///
/// `Full` (default): each scoped dep emits the spec-native typed
/// reversed-direction variant — `DEV_DEPENDENCY_OF` /
/// `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF`. Runtime deps emit
/// as natural-direction `DEPENDS_ON`. The SPDX 2.3 spec's
/// purpose-built field for the dev/build/test distinction — a
/// consumer that implements the full SPDX 2.3 relationshipType enum
/// sees the scope on every edge directly.
///
/// `Basic`: every dep — runtime, dev, build, test — emits as natural-
/// direction `DEPENDS_ON`. The scope distinction lives entirely on
/// the target Package via the `mikebom:lifecycle-scope` annotation
/// (which is also emitted under `Full`, so consumers can rely on it
/// in either mode). Use only when emitting for downstream tooling
/// that doesn't implement the typed scoped variants (Trivy, Syft,
/// and tooling built on top of them — empirically the dominant
/// consumer set, but spec-incomplete).
///
/// CDX and SPDX 3 emission are unaffected — CDX always carries scope
/// on the component (`scope: "excluded"` plus the
/// `mikebom:lifecycle-scope` property), and SPDX 3 always uses
/// `LifecycleScopedRelationship` with `relationshipType: "dependsOn"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spdx2RelationshipCompat {
    /// Full SPDX 2.3 relationship vocabulary — typed
    /// reversed-direction edges for scoped deps;
    /// natural-direction `DEPENDS_ON` for runtime. Default.
    #[default]
    Full,
    /// Basic SPDX 2.3 vocabulary only — every dep emits as natural-
    /// direction `DEPENDS_ON` regardless of scope. Scope info lives
    /// on the target Package's `mikebom:lifecycle-scope` annotation.
    Basic,
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

    // -------- Milestone 077 — RootComponentOverride::is_active --------

    #[test]
    fn root_override_default_is_inactive() {
        let o = RootComponentOverride::default();
        assert!(!o.is_active());
    }

    #[test]
    fn root_override_name_only_is_active() {
        let o = RootComponentOverride {
            name: Some("widget-svc".to_string()),
            version: None,
        ..Default::default()
    };
        assert!(o.is_active());
    }

    #[test]
    fn root_override_version_only_is_active() {
        let o = RootComponentOverride {
            name: None,
            version: Some("1.2.3".to_string()),
        ..Default::default()
    };
        assert!(o.is_active());
    }

    #[test]
    fn root_override_both_fields_is_active() {
        let o = RootComponentOverride {
            name: Some("widget-svc".to_string()),
            version: Some("1.2.3".to_string()),
        ..Default::default()
    };
        assert!(o.is_active());
    }

    // -------- Milestone 077 — percent_encode_purl_name --------

    #[test]
    fn percent_encode_purl_name_passthrough_for_unreserved() {
        // RFC 3986 §2.3 unreserved set: ALPHA / DIGIT / "-" / "." / "_" / "~"
        let s = "abc-123_xyz.foo~bar";
        assert_eq!(percent_encode_purl_name(s), s);
    }

    #[test]
    fn percent_encode_purl_name_encodes_ascii_reserved() {
        // npm-scoped name shape: `@` and `/` percent-encoded.
        assert_eq!(
            percent_encode_purl_name("@acme/widget-svc"),
            "%40acme%2Fwidget-svc"
        );
    }

    #[test]
    fn percent_encode_purl_name_encodes_utf8_multibyte() {
        // UTF-8 multi-byte run for an emoji (4 bytes) → four `%XX`
        // sequences.
        let encoded = percent_encode_purl_name("foo🎉bar");
        // The emoji 🎉 (U+1F389) encodes as 0xF0 0x9F 0x8E 0x89.
        assert_eq!(encoded, "foo%F0%9F%8E%89bar");
    }

    #[test]
    fn percent_encode_purl_name_empty_returns_empty() {
        assert_eq!(percent_encode_purl_name(""), "");
    }

    #[test]
    fn percent_encode_purl_name_all_url_syntax_chars() {
        // `?` and `#` are rejected at parse, but other URL-reserved
        // characters (e.g., `@`, `/`, `:`, `+`, ` `) MUST encode.
        assert_eq!(
            percent_encode_purl_name("a@b/c:d+e f"),
            "a%40b%2Fc%3Ad%2Be%20f"
        );
    }
}
