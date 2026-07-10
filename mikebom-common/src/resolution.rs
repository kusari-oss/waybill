use serde::{Deserialize, Serialize};

use crate::types::hash::ContentHash;
use crate::types::license::SpdxExpression;
use crate::types::purl::Purl;

/// A software component resolved from build-trace evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedComponent {
    pub purl: Purl,
    pub name: String,
    pub version: String,
    pub evidence: ResolutionEvidence,
    /// Licenses asserted by the package author in their manifest
    /// (npm package.json, Cargo.toml, etc.) or by the OS package
    /// metadata (dpkg copyright, rpm header). Mapped to CycloneDX
    /// `licenses[]` entries with `acknowledgement: "declared"`.
    pub licenses: Vec<SpdxExpression>,
    /// Licenses determined through external analysis (currently:
    /// ClearlyDefined.io's curated `licensed.declared` field, which
    /// is itself the result of CD's automated analysis pass). Mapped
    /// to CycloneDX `licenses[]` entries with
    /// `acknowledgement: "concluded"`. Empty when no enrichment was
    /// performed (offline mode, ecosystem unsupported by the
    /// enricher, or the package isn't curated by ClearlyDefined).
    /// May overlap with [`licenses`] when both sources agree; the
    /// CDX serializer emits each side once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concluded_licenses: Vec<SpdxExpression>,
    pub hashes: Vec<ContentHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<String>,
    /// CPE 2.3 identifiers for this component. Synthesized locally
    /// using syft-style heuristic vendor candidates (e.g. `debian`,
    /// `<name>`). Multiple entries are emitted per component because
    /// NVD's CPE dictionary uses different vendor slugs for different
    /// packages and no single heuristic wins in all cases — downstream
    /// matchers can use any candidate that hits. Empty for ecosystems
    /// where the synthesizer has no opinion (rare). Serialized in
    /// CycloneDX as the first entry on `component.cpe` plus the full
    /// set under `properties["mikebom:cpe-candidates"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cpes: Vec<String>,
    pub advisories: Vec<AdvisoryRef>,
    /// Per-file occurrences when the component is sourced from an OS
    /// installed-package db with deep-hashing enabled. Each entry
    /// records the on-disk path that the package owns plus a SHA-256
    /// of its contents and the dpkg-recorded MD5 (when available) for
    /// cross-reference. Empty for trace-mode and filename-resolved
    /// components, and for db-sourced components when `--no-deep-hash`
    /// was passed. Maps to CycloneDX `evidence.occurrences[]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occurrences: Vec<FileOccurrence>,
    /// Lifecycle scope (milestone 052). Replaces the prior
    /// boolean `is_dev` field. Maps to native fields per format:
    /// CDX `scope: "excluded"` + `mikebom:lifecycle-scope` property
    /// for non-Runtime variants; SPDX 2.3 native
    /// `DEV/BUILD/TEST_DEPENDENCY_OF` relationship types via the
    /// matching `RelationshipType` variant; SPDX 3 `lifecycleScope`
    /// parameter on `dependsOn` relationships. `None` means
    /// "scope unknown" — sources that don't carry the distinction
    /// (dpkg, apk, rpmdb, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_scope: Option<LifecycleScope>,
    /// Build-inclusion status (milestone 112). Set on source-tier
    /// components whose participation in the main module's production
    /// build was ruled out (`NotNeeded`) or could not be determined
    /// (`Unknown`). `None` means production participation is confirmed
    /// or assumed — pre-feature semantics, byte-identical emission.
    /// Maps per format: CDX `scope: "excluded"` (NotNeeded only) +
    /// `mikebom:build-inclusion` property; SPDX 2.3 package annotation;
    /// SPDX 3 element annotation (parity bridges — neither SPDX format
    /// has a native excluded-scope construct).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_inclusion: Option<BuildInclusion>,
    /// Original unresolved requirement specification for fallback-tier
    /// entries (`requirements.txt` range specs, root `package.json`
    /// dependency declarations without a lockfile). The string is
    /// preserved verbatim so consumers can see what the original
    /// declaration was. Drives the `mikebom:requirement-range` property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_range: Option<String>,
    /// Source-kind marker for non-registry dependencies: `"local"`
    /// (`file:` URIs), `"git"` (`git+...`), `"url"` (`http(s)://...`).
    /// `None` for normal registry-sourced components. Drives the
    /// `mikebom:source-type` property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Traceability-ladder tier per milestone 002's research R13:
    /// `"build"` (eBPF trace), `"deployed"` (installed-package-db /
    /// installed venv / populated node_modules), `"analyzed"` (artefact
    /// file on disk identified by filename + hash), `"source"` (lockfile
    /// entry without a corresponding install), `"design"` (unlocked
    /// manifest declaration — requirements range, root package.json
    /// fallback). Drives the `mikebom:sbom-tier` property and the
    /// envelope-level `metadata.lifecycles[]` aggregation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sbom_tier: Option<String>,
    /// Milestone 003 diagnostic for Go binaries: `"missing"` when a
    /// file was detected as a Go binary but `runtime/debug.BuildInfo`
    /// extraction failed (stripped binary, external `strip` run),
    /// `"unsupported"` for Go <1.18 binaries whose pre-inline format
    /// we don't parse. Drives the `mikebom:buildinfo-status` property
    /// on the file-level component emitted when the module list
    /// couldn't be recovered. `None` on every other component,
    /// including successful Go BuildInfo extractions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buildinfo_status: Option<String>,
    /// Milestone 004 canonical evidence-kind per `contracts/schema.md`.
    /// One of: `rpm-file`, `rpmdb-sqlite`, `rpmdb-bdb`, `dynamic-linkage`,
    /// `elf-note-package`, `embedded-version-string`. `None` on every
    /// pre-milestone-004 component (milestones 001–003 non-rpm ecosystems
    /// keep their existing serialization unchanged). Drives the
    /// `mikebom:evidence-kind` property at CycloneDX serialization time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_kind: Option<String>,
    /// Milestone 004 US2 — binary-format classifier for file-level
    /// binary components. `"elf"` / `"macho"` / `"pe"`; `None` for
    /// non-binary components. Drives `mikebom:binary-class`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_class: Option<String>,
    /// Milestone 004 US2 — true when the file-level binary lacks
    /// symbol tables / debug info / version resources. Drives
    /// `mikebom:binary-stripped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_stripped: Option<bool>,
    /// Milestone 004 US2 — `"dynamic"` / `"static"` / `"mixed"`.
    /// Drives `mikebom:linkage-kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linkage_kind: Option<String>,
    /// Milestone 004 US2 — set on the file-level binary component when
    /// Go BuildInfo extraction succeeded on the same binary (R8 flat
    /// cross-link; FR-026). Drives `mikebom:detected-go`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_go: Option<bool>,
    /// Milestone 004 US2 — heuristic-confidence marker for components
    /// emitted via the curated embedded-version-string scanner.
    /// Exactly `"heuristic"` when present. Drives `mikebom:confidence`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// Milestone 004 US2 — packer-signature marker on file-level
    /// binary components. `"upx"` when the scanner hit a UPX
    /// signature (research R7). Drives `mikebom:binary-packed`.
    /// `None` for unpacked binaries + non-binary components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_packed: Option<String>,
    /// Feature 005 US1 — npm-role classifier. Exactly `"internal"`
    /// when present, on components discovered inside npm's own bundled
    /// tree (`**/node_modules/npm/node_modules/**`) during `--image`
    /// scans. `None` on application deps and on every `--path`-mode
    /// scan (internals are filtered out before resolution). Drives the
    /// `mikebom:npm-role` property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm_role: Option<String>,
    /// Feature 005 US4 — verbatim `VERSION-RELEASE` string from the
    /// rpmdb header (or equivalent source in other ecosystems that
    /// opt into this). Preserved so consumers can cross-reference
    /// `rpm -qa`'s `%{VERSION}-%{RELEASE}` column without re-parsing
    /// the PURL. Populated on every rpm component (both rpmdb-sourced
    /// via `rpm.rs` and standalone-artefact via `rpm_file.rs`); `None`
    /// elsewhere until another ecosystem adopts the pattern. Drives
    /// the `mikebom:raw-version` property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_version: Option<String>,
    /// PURL of a parent/container component that physically bundles
    /// this one. Set when the component was discovered inside another
    /// component — e.g. a vendored coord extracted from a Maven
    /// shade-plugin fat-jar's `META-INF/maven/<g>/<a>/` directory. The
    /// enclosing fat-jar's own PURL is recorded here so the CDX
    /// emitter can nest this component under its parent's
    /// `component.components[]` array (CDX 1.6 nested-components
    /// shape). Deduplication groups by `(ecosystem, name, version,
    /// parent_purl)` so the same coord vendored in two different
    /// parents surfaces as two distinct nested children rather than
    /// collapsing to one. `None` on top-level components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_purl: Option<String>,
    /// Ecosystem (other than this component's own) that owns the
    /// bytes from which this component's identity was extracted. Set
    /// when the same on-disk artifact carries two valid package
    /// identities — e.g. a JAR at `/usr/share/java/guava/guava.jar`
    /// owned by a Fedora RPM AND carrying a Maven coord in its
    /// embedded `META-INF/maven/.../pom.properties`. The Maven coord
    /// emits with `co_owned_by = Some("rpm")`; the RPM coord emits
    /// independently. Drives the CDX property `mikebom:co-owned-by`
    /// so downstream consumers can filter to a single-identity view.
    /// `None` on standalone artifacts (no cross-ecosystem overlap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub co_owned_by: Option<String>,
    /// Feature 009: `Some(true)` when the component was derived from a
    /// shaded JAR's `META-INF/DEPENDENCIES` file (ancestor dep with
    /// relocated bytecode inside the enclosing JAR). Vulnerability
    /// scanners can match against these coords even when the classes
    /// are namespace-relocated in the image. Surfaced via CDX property
    /// `mikebom:shade-relocation = true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shade_relocation: Option<bool>,
    /// External references for this component — repository URLs,
    /// homepages, issue trackers. Maps to CycloneDX
    /// `components[].externalReferences[]`. Populated from PURL
    /// heuristics (e.g. `pkg:golang/github.com/X/Y` → vcs
    /// `https://github.com/X/Y`) and from deps.dev `VersionInfo.links`.
    /// Drives sbomqs `comp_with_source_code` when a `vcs`-type
    /// entry is present.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_references: Vec<ExternalReference>,
    /// Milestone 023: generic per-component annotation bag mirroring
    /// `PackageDbEntry::extra_annotations`. Each entry is emitted at
    /// SBOM-generation time as a `mikebom:<key>` annotation across
    /// all three formats (CDX property, SPDX 2.3 annotation envelope,
    /// SPDX 3 graph-element Annotation). Used by the binary scanner
    /// for fields like `mikebom:elf-build-id`, `mikebom:elf-runpath`,
    /// `mikebom:elf-debuglink`. Future per-binary-metadata milestones
    /// (024 Mach-O LC_UUID, 025 Go VCS, 026 version strings, 027
    /// layer attribution) populate the same bag without per-field
    /// schema migration. `BTreeMap` for deterministic emission order.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub extra_annotations: std::collections::BTreeMap<String, serde_json::Value>,
    /// Milestone 104 — role classification for binary-reader-discovered
    /// components. `Some(role)` when this component came from
    /// `mikebom-cli/src/scan_fs/binary/`; `None` for manifest- and
    /// lockfile-driven readers. Emitters map this to the format-native
    /// component-type field (CDX `Component.type`, SPDX 2.3
    /// `Package.primaryPackagePurpose`, SPDX 3
    /// SPDX 3 primary-purpose equivalent). See `BinaryRole`
    /// for the format-to-role table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_role: Option<BinaryRole>,
}

/// A single external reference on a `ResolvedComponent`. The
/// `ref_type` values mirror CDX 1.6's `externalReferences[].type`
/// enum: `vcs` (source-code repo), `website` (project homepage),
/// `issue-tracker`, `distribution`, etc. Kept as a string rather
/// than an enum so new values flow through without a crate release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalReference {
    pub ref_type: String,
    pub url: String,
}

/// One installed file owned by a `ResolvedComponent`. The presence of
/// per-file occurrences is what distinguishes a deep-hashed db-sourced
/// component from a fast db-sourced one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOccurrence {
    /// Canonical on-disk path the package owns — the path dpkg's
    /// `<pkg>.list` manifest declares (e.g. `/usr/bin/jq`), not the
    /// tempdir-prefixed path observed during an image-rootfs scan. This
    /// keeps occurrences comparable across hosts and the per-component
    /// Merkle root deterministic across scans.
    pub location: String,
    /// SHA-256 of the file contents at scan time, lowercase hex.
    pub sha256: String,
    /// MD5 reference dpkg recorded at install time, lowercase hex.
    /// `None` when the file was on disk but had no entry in the
    /// package's `.md5sums` (config files, /etc overrides, files
    /// created post-install).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5_legacy: Option<String>,
    /// apk-provided SHA-1 from the `Z:` line in the package's
    /// stanza, lowercase hex (40 chars). `None` for non-apk
    /// occurrences (deb, rpm) and for apk files whose stanza
    /// omitted the `Z:` line. Milestone 040 / #75 follow-on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apk_sha1: Option<String>,
    /// rpm-provided per-file digest from the package's
    /// `FILEDIGESTS` tag, in algorithm-prefixed form
    /// (`"sha256:<hex>"`, `"md5:<hex>"`, etc.). `None` for non-rpm
    /// occurrences (deb, apk) and for rpm files whose stanza had
    /// no usable cross-ref (non-regular files, missing
    /// FILEDIGESTS, unknown FILEDIGESTALGO). Milestone 041.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm_file_digest: Option<String>,
}

/// Evidence describing how a component was resolved from trace data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolutionEvidence {
    pub technique: ResolutionTechnique,
    pub confidence: f64,
    pub source_connection_ids: Vec<String>,
    pub source_file_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deps_dev_match: Option<DepsDevMatch>,
}

/// The technique used to resolve a component from observed activity.
///
/// Ordered by typical confidence (highest first). When the deduplicator
/// merges components that resolve from multiple techniques, the entry
/// with the highest per-component confidence wins; the variant ordering
/// is documentary only.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTechnique {
    /// HTTPS download URL matched a known package-registry pattern
    /// during a build-time trace. Confidence 0.95.
    UrlPattern,
    /// Content hash returned a hit from a deps.dev lookup.
    /// Confidence 0.90.
    HashMatch,
    /// Read directly from an OS-level installed-package database
    /// (`/var/lib/dpkg/status`, `/lib/apk/db/installed`, …).
    /// Authoritative for what's installed but doesn't carry per-file
    /// content hashes and didn't observe the install event.
    /// Confidence 0.85.
    PackageDatabase,
    /// A file matching a recognised cache path pattern
    /// (`~/.cargo/registry/cache/...*.crate`, `/var/cache/apt/archives/...deb`,
    /// etc.) present on disk. Confidence 0.70.
    FilePathPattern,
    /// The observed hostname matched a known registry but no specific
    /// package URL was extracted. Confidence 0.40.
    HostnameHeuristic,
}

/// Reference to a security advisory affecting a resolved component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvisoryRef {
    pub id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A match result from the deps.dev dependency resolution service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepsDevMatch {
    pub system: String,
    pub name: String,
    pub version: String,
}

/// A dependency relationship between two resolved components.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    /// The component that depends on another (PURL string).
    pub from: String,
    /// The component being depended upon (PURL string).
    pub to: String,
    /// Type of relationship.
    pub relationship_type: RelationshipType,
    /// Where this relationship was discovered.
    pub provenance: EnrichmentProvenance,
}

/// Lifecycle scope of a component (milestone 052). Replaces the
/// boolean `is_dev` field with a 4-variant typed enum that maps
/// directly onto each target SBOM format's native scope construct:
///
/// - **CycloneDX 1.6**: `Runtime` / `None` → `scope` field omitted
///   (default `required`); `Development` / `Build` / `Test` →
///   `scope: "excluded"` plus `mikebom:lifecycle-scope` property
///   carrying the finer variant name (CDX's 3-value `scope` enum
///   cannot express the dev-vs-build-vs-test split).
/// - **SPDX 2.3**: maps to the relationship-type variants below
///   (`Development` → `DevDependsOn` → SPDX `DEV_DEPENDENCY_OF`;
///   `Build` → `BuildDependsOn` → `BUILD_DEPENDENCY_OF`; `Test` →
///   `TestDependsOn` → `TEST_DEPENDENCY_OF`).
/// - **SPDX 3.0.1**: `lifecycleScope` field on `dependsOn`
///   relationship elements (`development`, `build`, `test`,
///   `runtime`).
///
/// `None` means "scope unknown / unclassified" — sources that
/// don't carry the distinction (dpkg, apk, rpmdb, etc.). Three-state
/// semantics preserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleScope {
    Runtime,
    Development,
    Build,
    Test,
    /// Milestone 179 — declared-optional dependency (Cargo `optional =
    /// true`, npm `optionalDependencies`, pip extras, Maven
    /// `<optional>true</optional>`, Gradle `compileOnly`, Erlang
    /// `optional_applications`). May or may not appear in a
    /// production build. Emits CDX `scope: "excluded"` (via
    /// `is_non_runtime()`) + SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`
    /// (reversed direction, m052 convention). SPDX 3.0.1's
    /// `LifecycleScopeType` enum has no `optional` value at spec
    /// 3.0.1; SPDX 3 emits classification via the
    /// `mikebom:optional-derivation` component annotation instead
    /// (Principle V KEEP-BOTH carve-out).
    Optional,
}

impl LifecycleScope {
    /// Lower-cased serde-style variant name. Used for the new
    /// `mikebom:lifecycle-scope` CDX property carry of the finer
    /// distinction the standards-native `scope: "excluded"` cannot
    /// express.
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleScope::Runtime => "runtime",
            LifecycleScope::Development => "development",
            LifecycleScope::Build => "build",
            LifecycleScope::Test => "test",
            LifecycleScope::Optional => "optional",
        }
    }

    /// True when the scope is anything but `Runtime`. Convenience
    /// for serializer call sites that ask "should this component
    /// emit `scope: \"excluded\"`?" or "is this a non-runtime dep?".
    pub fn is_non_runtime(&self) -> bool {
        !matches!(self, LifecycleScope::Runtime)
    }
}

/// Milestone 104 — role classification for binary-reader-discovered
/// components. Derived from the source file's format header
/// (Mach-O `MH_*` filetype, ELF `e_type` + program-header inspection,
/// PE `IMAGE_FILE_HEADER.Characteristics`). Maps at emission time to
/// the format-native component-type slot in each of CycloneDX,
/// SPDX 2.3, and SPDX 3.
///
/// Per `specs/104-binary-role-classification/contracts/binary-role-cross-format-mapping.md`:
///
/// | Role          | CDX type      | SPDX 2.3 primary purpose | SPDX 3 primary purpose |
/// |---------------|---------------|--------------------------|------------------------|
/// | Application   | `application` | `APPLICATION`            | `application`          |
/// | SharedLibrary | `library`     | `LIBRARY`                | `library`              |
/// | Object        | `file`        | `FILE`                   | `file`                 |
/// | Other         | `library`     | _omitted_                | _omitted_              |
///
/// `None` on `ResolvedComponent.binary_role` means the component did
/// NOT come from the binary reader (manifest- and lockfile-driven
/// readers leave the field unset) — emitters fall back to the
/// per-ecosystem default (today: CDX `library`, SPDX 2.3 omitted,
/// SPDX 3 omitted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryRole {
    /// Executable program — Mach-O `MH_EXECUTE`, ELF `ET_EXEC`, ELF
    /// `ET_DYN` with `PT_INTERP` (PIE executables), PE without
    /// `IMAGE_FILE_DLL`.
    Application,
    /// Dynamically loadable code unit — Mach-O `MH_DYLIB`, ELF
    /// `ET_DYN` without `PT_INTERP`, PE with `IMAGE_FILE_DLL`.
    SharedLibrary,
    /// Relocatable object file (intermediate build artifact) —
    /// Mach-O `MH_OBJECT`, ELF `ET_REL`.
    Object,
    /// Format-specific bucket that doesn't map cleanly to the above:
    /// Mach-O `MH_BUNDLE` / `MH_KEXT_BUNDLE` / `MH_CORE`; ELF
    /// `ET_CORE`; PE with `IMAGE_FILE_SYSTEM`; unparseable headers.
    Other,
}

/// Milestone 112 — build-inclusion status for a source-tier component
/// whose participation in a production build was either ruled out by
/// package-level analysis or could not be determined. Absence
/// (`None` on `ResolvedComponent.build_inclusion`) means production
/// participation is confirmed or assumed (pre-feature semantics).
///
/// String forms (`"unknown"`, `"not-needed"`) appear only at emission
/// via [`BuildInclusion::as_str`] / serde — never as raw strings
/// across internal boundaries (Constitution IV).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildInclusion {
    /// Discovered only via go.sum fallback / orphan flat-attach and
    /// confirmed by no higher-fidelity signal.
    Unknown,
    /// Package-level analysis (`go mod why`) determined the main
    /// module's production build does not need this module.
    NotNeeded,
}

impl BuildInclusion {
    /// Kebab-case serde-style variant name. Used for the
    /// `mikebom:build-inclusion` property/annotation value.
    pub fn as_str(&self) -> &'static str {
        match self {
            BuildInclusion::Unknown => "unknown",
            BuildInclusion::NotNeeded => "not-needed",
        }
    }
}

/// Backward-compat helper bridging the milestone-052 lifecycle_scope
/// field to the pre-052 `is_dev: Option<bool>` semantic. Returns true
/// when the component is `Development`, `Build`, or `Test` scoped —
/// i.e., when the legacy `is_dev` flag would have been
/// `Some(true)`. Intermediate during the milestone 052 transition;
/// to be removed when all serializer call sites migrate to native
/// fields.
pub fn lifecycle_scope_is_legacy_dev(scope: &Option<LifecycleScope>) -> bool {
    matches!(
        scope,
        Some(LifecycleScope::Development) | Some(LifecycleScope::Build) | Some(LifecycleScope::Test)
    )
}

/// Type of dependency relationship.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    DependsOn,
    DevDependsOn,
    BuildDependsOn,
    TestDependsOn,
    /// Milestone 179 — target is declared-optional (see
    /// [`LifecycleScope::Optional`]). Emits SPDX 2.3
    /// `OPTIONAL_DEPENDENCY_OF` (reversed direction, m052 convention)
    /// under `--spdx2-relationship-compat=full`; collapses to
    /// natural-direction `DEPENDS_ON` under `basic` per m228.
    OptionalDependsOn,
}

/// Provenance tracking for enriched data (Constitution Principle X).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnrichmentProvenance {
    /// Name of the enrichment source (e.g., "Cargo.lock", "deps.dev", "osv")
    pub source: String,
    /// What type of data this source provided
    pub data_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_technique_serde_snake_case() {
        let json = serde_json::to_string(&ResolutionTechnique::UrlPattern)
            .expect("serialize technique");
        assert_eq!(json, "\"url_pattern\"");

        let back: ResolutionTechnique =
            serde_json::from_str("\"hash_match\"").expect("deserialize technique");
        assert_eq!(back, ResolutionTechnique::HashMatch);
    }

    #[test]
    fn resolved_component_omits_none_fields() {
        let component = ResolvedComponent {
            purl: Purl::new("pkg:cargo/serde@1.0.197").expect("valid purl"),
            name: "serde".to_string(),
            version: "1.0.197".to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.95,
                source_connection_ids: vec!["conn-1".to_string()],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        };

        let json = serde_json::to_string(&component).expect("serialize component");
        assert!(!json.contains("\"supplier\""));
        assert!(!json.contains("\"deps_dev_match\""));
        assert!(!json.contains("\"cpes\""));
        assert!(!json.contains("\"build_inclusion\""));
    }

    #[test]
    fn build_inclusion_serde_kebab_case_round_trip() {
        let json = serde_json::to_string(&BuildInclusion::Unknown).expect("serialize unknown");
        assert_eq!(json, "\"unknown\"");
        let json =
            serde_json::to_string(&BuildInclusion::NotNeeded).expect("serialize not-needed");
        assert_eq!(json, "\"not-needed\"");

        let back: BuildInclusion =
            serde_json::from_str("\"unknown\"").expect("deserialize unknown");
        assert_eq!(back, BuildInclusion::Unknown);
        let back: BuildInclusion =
            serde_json::from_str("\"not-needed\"").expect("deserialize not-needed");
        assert_eq!(back, BuildInclusion::NotNeeded);

        assert_eq!(BuildInclusion::Unknown.as_str(), "unknown");
        assert_eq!(BuildInclusion::NotNeeded.as_str(), "not-needed");
    }

    #[test]
    fn lifecycle_scope_optional_serde_roundtrip() {
        let scope = LifecycleScope::Optional;
        let json = serde_json::to_string(&scope).expect("serialize optional");
        assert_eq!(json, "\"optional\"");
        let back: LifecycleScope = serde_json::from_str(&json).expect("deserialize optional");
        assert_eq!(back, scope);
    }

    #[test]
    fn lifecycle_scope_optional_is_non_runtime() {
        // Milestone 179 FR-006: `Optional` drives CDX `scope:
        // "excluded"` via the existing `is_non_runtime()` helper.
        assert!(LifecycleScope::Optional.is_non_runtime());
    }

    #[test]
    fn lifecycle_scope_optional_as_str() {
        assert_eq!(LifecycleScope::Optional.as_str(), "optional");
    }

    #[test]
    fn lifecycle_scope_legacy_dev_excludes_optional() {
        // Milestone 179 data-model.md §1.1: `Optional` is NOT a
        // legacy-dev variant. The m052 compat bridge (which pre-052
        // callers use to check "was this an `is_dev == Some(true)`
        // component?") MUST continue to return false for the new
        // variant so it doesn't accidentally inherit dev-scope
        // semantics via the compat shim.
        assert!(!lifecycle_scope_is_legacy_dev(&Some(
            LifecycleScope::Optional
        )));
    }

    #[test]
    fn relationship_type_optional_serde_roundtrip() {
        let rt = RelationshipType::OptionalDependsOn;
        let json = serde_json::to_string(&rt).expect("serialize optional_depends_on");
        assert_eq!(json, "\"optional_depends_on\"");
        let back: RelationshipType =
            serde_json::from_str(&json).expect("deserialize optional_depends_on");
        assert_eq!(back, rt);
    }

    #[test]
    fn resolved_component_serde_round_trip() {
        let component = ResolvedComponent {
            purl: Purl::new("pkg:npm/lodash@4.17.21").expect("valid purl"),
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::HashMatch,
                confidence: 0.99,
                source_connection_ids: vec!["conn-5".to_string()],
                source_file_paths: vec!["/tmp/build/node_modules/lodash".to_string()],
                deps_dev_match: Some(DepsDevMatch {
                    system: "npm".to_string(),
                    name: "lodash".to_string(),
                    version: "4.17.21".to_string(),
                }),
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: Some("Lodash contributors".to_string()),
            cpes: vec![],
            advisories: vec![AdvisoryRef {
                id: "GHSA-xxxx-yyyy-zzzz".to_string(),
                source: "github".to_string(),
                url: Some("https://github.com/advisories/GHSA-xxxx-yyyy-zzzz".to_string()),
            }],
            occurrences: vec![],
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        };

        let json = serde_json::to_string(&component).expect("serialize component");
        let back: ResolvedComponent = serde_json::from_str(&json).expect("deserialize component");
        assert_eq!(component.purl, back.purl);
        assert_eq!(component.evidence.confidence, back.evidence.confidence);
        assert_eq!(component.supplier, back.supplier);
        assert_eq!(component.advisories.len(), back.advisories.len());
    }
}