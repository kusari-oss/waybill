//! Read installed-package databases from a filesystem root.
//!
//! Two formats supported this round:
//! - **dpkg**: `<root>/var/lib/dpkg/status` (Debian, Ubuntu, derivatives)
//! - **apk**: `<root>/lib/apk/db/installed` (Alpine, Wolfi)
//!
//! The dispatcher tries both and returns whichever parses cleanly. In
//! the rare case a rootfs has *both* (it shouldn't; no real distro
//! does), entries are returned in the order the readers were tried —
//! dpkg first, then apk. The scan pipeline de-duplicates by PURL so
//! that scenario's output is still well-formed.

pub mod apk;
pub mod bazel;
pub mod cargo;
pub mod cmake;
pub mod conan;
mod control_file;
pub mod copyright;
pub mod dpkg;
pub mod file_hashes;
pub mod gem;
pub mod go_binary;
pub mod golang;
pub mod gradle;
pub mod maven;
pub mod maven_sidecar;
pub mod npm;
pub mod nuget;
pub mod pip;
mod project_roots;
pub mod rpm;
pub mod rpm_file;
pub mod rpmdb_bdb;
pub mod rpmdb_sqlite;
pub mod vcpkg;
mod workspace;

use std::path::Path;

use mikebom_common::types::hash::ContentHash;
use mikebom_common::types::license::SpdxExpression;
use mikebom_common::types::purl::Purl;

/// A parsed row from an OS package database, normalised to the shape
/// the scan pipeline consumes. `source_path` is the db file we read —
/// it goes straight into the resulting `ResolutionEvidence.source_file_paths`.
#[derive(Clone, Debug)]
pub struct PackageDbEntry {
    pub purl: Purl,
    pub name: String,
    pub version: String,
    pub arch: Option<String>,
    pub source_path: String,
    /// Raw dependency package names declared by this entry (dpkg's
    /// `Depends:` field, apk's `D:` field). Version constraints and
    /// alternative (`|`) separators are already tokenised into
    /// individual names here; the scan orchestrator looks each name
    /// up against the set of entries found in the same scan and drops
    /// any that don't resolve.
    pub depends: Vec<String>,
    /// Free-form package supplier — for dpkg, the `Maintainer:` field
    /// (e.g. `"Matthias Klose <doko@debian.org>"`). Maps directly to
    /// CycloneDX `component.supplier.name`. `None` when the source db
    /// doesn't carry a supplier (apk's installed db has no equivalent
    /// per-package field).
    pub maintainer: Option<String>,
    /// Lifecycle scope (milestone 052) — replaces the prior
    /// `is_dev: Option<bool>` flag with a 4-variant typed enum
    /// (`Runtime`, `Development`, `Build`, `Test`). Maps directly
    /// to native fields per format: CDX `scope` + new
    /// `mikebom:lifecycle-scope` property; SPDX 2.3 native
    /// `DEV/BUILD/TEST_DEPENDENCY_OF` relationship types via the
    /// matching `RelationshipType` variant; SPDX 3 `lifecycleScope`
    /// on `dependsOn`. `None` = source doesn't carry the
    /// distinction (dpkg, apk, venv `.dist-info`,
    /// `requirements.txt`).
    pub lifecycle_scope: Option<mikebom_common::resolution::LifecycleScope>,
    /// Original unresolved requirement specification for fallback-tier
    /// entries (`requirements.txt` lines, root `package.json`
    /// dependencies). `None` for authoritative sources.
    /// Drives the `mikebom:requirement-range` property at serialization.
    pub requirement_range: Option<String>,
    /// Source-kind marker for non-registry dependencies: `"local"`
    /// (file:), `"git"` (git+...), `"url"` (http(s)://...). `None`
    /// for normal registry-sourced components. Drives the
    /// `mikebom:source-type` property at serialization.
    pub source_type: Option<String>,
    /// Licenses the source embedded directly on the entry (e.g. pypi's
    /// `dist-info/METADATA::License-Expression:`, npm's
    /// `package.json::license:`). Empty for sources where licenses are
    /// resolved out-of-band (dpkg reads `/usr/share/doc/<pkg>/copyright`
    /// separately in `scan_fs::mod.rs`; apk doesn't carry licenses
    /// inline in the scan yet). When populated, `scan_fs::scan_path`
    /// uses these values instead of calling an out-of-band resolver.
    pub licenses: Vec<SpdxExpression>,
    /// Go-binary BuildInfo extraction status for diagnostic file-level
    /// entries (FR-015, milestone 003 US1). `Some("missing")` means the
    /// magic bytes were absent; `Some("unsupported")` means the format
    /// variant isn't implemented (pre-1.18 pointer-indirection). `None`
    /// for every non-diagnostic entry. Drives the
    /// `mikebom:buildinfo-status` property at serialization.
    pub buildinfo_status: Option<String>,
    /// Traceability-ladder tier per research.md R13 (Milestone 002):
    /// `"deployed"` (installed-package-db entries — dpkg, apk, Python
    /// venv, npm `node_modules/`), `"analyzed"` (artefact files on
    /// disk, identified by filename + hash), `"source"` (lockfile
    /// entries without a corresponding install), `"design"` (unlocked
    /// manifest entries — requirements.txt ranges, root package.json
    /// fallback). `None` during transition to preserve compatibility
    /// with any PackageDbEntry construction site that hasn't been
    /// retrofitted yet. Trace-mode components carry `"build"` but
    /// don't flow through PackageDbEntry.
    pub sbom_tier: Option<String>,
    /// Milestone 004: canonical `mikebom:evidence-kind` value per
    /// `contracts/schema.md`. One of:
    /// - `rpm-file` — `.rpm` artefact reader
    /// - `rpmdb-sqlite` — milestone-003 sqlite rpmdb reader (retrofit Q7)
    /// - `rpmdb-bdb` — legacy BDB rpmdb reader (US4)
    /// - `dynamic-linkage` — ELF DT_NEEDED / Mach-O LC_LOAD_DYLIB / PE IMPORT
    /// - `elf-note-package` — systemd Packaging Metadata Notes
    /// - `embedded-version-string` — curated heuristic scanner
    ///
    /// `None` on readers not yet retrofitted (milestones 001–003 non-rpm
    /// ecosystems). Drives the `mikebom:evidence-kind` property at
    /// serialization; value space is enforced by a `debug_assert!` gate
    /// in `generate/cyclonedx/builder.rs`.
    pub evidence_kind: Option<String>,
    /// Milestone 004 US2 — file-level binary classifier (`"elf"` /
    /// `"macho"` / `"pe"`). Set only on file-level binary components
    /// emitted by the new `scan_fs::binary` reader.
    pub binary_class: Option<String>,
    /// Milestone 004 US2 — true when format-appropriate debug / symbol
    /// / version metadata is absent on a file-level binary component.
    pub binary_stripped: Option<bool>,
    /// Milestone 004 US2 — `"dynamic"` / `"static"` / `"mixed"` on
    /// file-level binary components.
    pub linkage_kind: Option<String>,
    /// Milestone 004 US2 — set to `Some(true)` on a file-level binary
    /// component when the Go BuildInfo extractor also matched on the
    /// same binary (R8 flat cross-link).
    pub detected_go: Option<bool>,
    /// Milestone 004 US2 — `"heuristic"` on components emitted via the
    /// curated embedded-version-string scanner (FR-025).
    pub confidence: Option<String>,
    /// Milestone 004 US2 — `"upx"` when a UPX packer signature was
    /// detected on a file-level binary component. `None` otherwise.
    pub binary_packed: Option<String>,
    /// Feature 005 US4 — the raw `<VERSION>-<RELEASE>` string from the
    /// rpmdb header (or `.rpm` artefact), preserved verbatim before any
    /// PURL encoding. Drives the `mikebom:raw-version` property at
    /// serialization. `None` on non-rpm readers.
    pub raw_version: Option<String>,
    /// Parent/container component's PURL, when this entry was extracted
    /// from inside another physical artifact. Set by the Maven scanner
    /// on coords discovered inside a shade-plugin fat-jar's
    /// `META-INF/maven/<g>/<a>/` directories — the enclosing fat-jar's
    /// own PURL is recorded here so the downstream CDX emitter can nest
    /// this component under `component.components[]` on its parent.
    /// `None` on top-level (on-disk-as-their-own-file) components.
    pub parent_purl: Option<String>,
    /// Feature 005 US1 — role marker for packages that are part of a
    /// package-manager's own toolchain rather than an application
    /// dependency. Currently set to `Some("internal")` by the npm
    /// reader on packages under the canonical `**/node_modules/npm/node_modules/**`
    /// glob. Drives the `mikebom:npm-role` CycloneDX component property.
    pub npm_role: Option<String>,
    /// Ecosystem that claims the bytes this component's identity was
    /// extracted from, when the same on-disk artifact is also owned
    /// by a package-database reader. Currently set by the Maven JAR
    /// walker to `Some("rpm")`, `Some("deb")`, or `Some("apk")` when
    /// embedded `META-INF/maven/.../pom.properties` identifies a
    /// Maven coord inside a JAR whose path is already claimed by an
    /// OS package-db reader (e.g. `/usr/share/java/guava/guava.jar`
    /// owned by a Fedora RPM). The Maven coord emits alongside the
    /// RPM/deb/apk component — same bytes, two valid identities for
    /// different downstream use cases. Drives the CDX property
    /// `mikebom:co-owned-by` so consumers can filter to a single-
    /// identity view if they prefer. `None` on free-standing JARs.
    pub co_owned_by: Option<String>,
    /// Content hashes carried by the source manifest. npm
    /// `package-lock.json::integrity` (sha256 / sha384 / sha512) and
    /// Cargo.lock's `checksum` (sha256 hex) land here; dpkg / rpm /
    /// apk hashes are computed separately via `file_hashes.rs` and
    /// attached to `ResolvedComponent.hashes` in `scan_fs::mod.rs`
    /// after this reader returns. Empty by default; populated by
    /// readers that have manifest-level hashes available.
    pub hashes: Vec<ContentHash>,
    /// Feature 009: `Some(true)` when the entry was derived from a
    /// shaded JAR's `META-INF/DEPENDENCIES` file (ancestor dep with
    /// relocated bytecode inside the enclosing JAR). Consumers can
    /// filter on this to separate "linkable direct deps" from
    /// "bytecode-present shaded ancestors." Surfaced via CDX
    /// property `mikebom:shade-relocation = true`.
    pub shade_relocation: Option<bool>,
    /// Milestone 023: generic per-component annotation bag. Each
    /// entry is emitted at SBOM-generation time as `mikebom:<key>`:
    /// a CycloneDX `properties[]` entry, a SPDX 2.3 `annotations[]`
    /// envelope, and a SPDX 3 graph-element Annotation. Used by the
    /// binary scanner for fields like `mikebom:elf-build-id`,
    /// `mikebom:elf-runpath`, `mikebom:elf-debuglink`; future
    /// per-binary-metadata milestones (024 Mach-O LC_UUID, 025 Go
    /// VCS, 026 version strings, 027 layer attribution) populate
    /// the same bag without requiring per-field schema migration.
    /// `BTreeMap` chosen over `HashMap` for deterministic emission
    /// order — byte-identity goldens depend on stable output.
    /// Default empty.
    pub extra_annotations: std::collections::BTreeMap<String, serde_json::Value>,
    /// Milestone 104 — role classification for binary-reader-discovered
    /// components (Application / SharedLibrary / Object / Other).
    /// `Some(_)` for components from `scan_fs/binary/`; `None` for
    /// every manifest- and lockfile-driven reader. Propagates verbatim
    /// to `ResolvedComponent.binary_role` and drives the format-native
    /// component-type field at emission time.
    pub binary_role: Option<mikebom_common::resolution::BinaryRole>,
}

/// Hard failures a database reader can raise that MUST abort the scan
/// rather than degrade silently. Currently the only case is the npm
/// v1 lockfile refusal — per `contracts/cli-interface.md` the CLI must
/// emit a specific stderr message and exit non-zero rather than produce
/// a partial SBOM.
#[derive(Debug, thiserror::Error)]
pub enum PackageDbError {
    #[error("{0}")]
    Npm(#[from] npm::NpmError),
    #[error("{0}")]
    Cargo(#[from] cargo::CargoError),
}

/// Aggregate output of all package-db readers. Milestone-004 post-ship
/// fix for the binary-walker double-counting issue: when a file is
/// claimed by a package-db reader (dpkg `.list`, apk `R:`, pip `RECORD`),
/// the binary walker must skip its file-level + linkage-evidence
/// emissions for that path to avoid reporting the same file as both
/// `pkg:deb/…/coreutils` AND `pkg:generic/base64?file-sha256=…`.
///
/// `.note.package` + embedded-version-string emissions remain unconditional
/// because those surface signals the package-db can't produce (distro
/// self-identification, statically-linked TLS-library versions).
#[derive(Debug, Default)]
pub struct DbScanResult {
    pub entries: Vec<PackageDbEntry>,
    /// Absolute rootfs-joined paths claimed by at least one package-db
    /// reader. Each claim is inserted in raw form + parent-canonical
    /// form so the walker's path matches against either representation
    /// on usrmerged rootfs.
    pub claimed_paths: std::collections::HashSet<std::path::PathBuf>,
    /// (device, inode) pairs of every claimed file that exists at
    /// claim-insert time. Provides symlink-robust matching that closes
    /// the gap path-based matching leaves for hard links, canonicalize
    /// output-form differences, and multiarch path quirks. If the
    /// walker's binary and a claim share (dev, ino), they're the same
    /// physical file — no path-level reasoning required.
    #[cfg(unix)]
    pub claimed_inodes: std::collections::HashSet<(u64, u64)>,
    /// Feature 005 — non-fatal diagnostics collected during `read_all`.
    /// Surfaced into the SBOM's `metadata.properties` so consumers can
    /// detect degraded output without needing the scanner's log stream.
    pub diagnostics: ScanDiagnostics,
    /// M3 — Maven scan-subject coord identified during the JAR walk,
    /// either by `target_name` artifactId match or by the fat-jar
    /// heuristic (≥2 embedded `META-INF/maven/` entries in a
    /// non-OS-claimed JAR). Populated when mikebom suppresses the
    /// primary coord from `components[]` because it represents the
    /// SBOM subject, not a dependency. `None` when no Maven scan
    /// subject was identified (non-Java target or plain-JAR layout).
    /// The orchestrator uses this to promote the real Maven PURL
    /// into `metadata.component` instead of the generic placeholder.
    pub scan_target_coord: Option<maven::ScanTargetCoord>,
}

/// Non-fatal scan-time diagnostics accumulated during `read_all`. Drives
/// document-level CycloneDX `metadata.properties` entries so SBOM
/// consumers can detect degraded output (missing `/etc/os-release` fields,
/// etc.) without needing access to the scanner's log stream.
///
/// Intentionally open-ended — future scan-time diagnostics (rpmdb WAL
/// warnings, docker extraction failures) can be added without churning
/// cross-module signatures.
#[derive(Default, Debug, Clone)]
pub struct ScanDiagnostics {
    /// Fields from `/etc/os-release` that were absent or empty when the
    /// dpkg/apk/rpm readers tried to read them. Each entry is a string
    /// naming the missing field (e.g. `"ID"`, `"VERSION_ID"`).
    /// Deduplicated; insertion order preserved for determinism.
    pub os_release_missing_fields: Vec<String>,

    /// Milestone 061 (closes #119): document-level graph-completeness
    /// signal for the Go ecosystem. Aggregated from the milestone 058
    /// orphan classification + milestone 055 resolver's `LadderSummary`
    /// — populated by `golang::legacy::read()` on every Go-scan path
    /// and propagated up through `read_all` into format emitters'
    /// `metadata.properties[]` / document-level `annotations[]`.
    ///
    /// `None` ⇒ no Go scan happened, signal not applicable. `Complete`
    /// ⇒ zero orphans across `pkg:golang/...` components. `Partial`
    /// ⇒ one or more orphans (sibling `go_graph_completeness_reason`
    /// names the why).
    pub go_graph_completeness: Option<GraphCompleteness>,

    /// Comma-separated list of `<ecosystem>:<reason-class>` tokens
    /// explaining WHY `go_graph_completeness == Partial`. Empty when
    /// completeness is `Complete` or `None`. Format follows the
    /// milestone 061 FR-005 contract: tokens like
    /// `"go:unresolved-indirect-require"`, `"go:proxy-fetch-failed"`,
    /// joined with `,` when multiple classes contributed.
    pub go_graph_completeness_reason: Option<String>,
}

/// Document-level completeness classification for the Go ecosystem
/// graph. Per Constitution Principle X (Transparency): when mikebom
/// can't supply every transitive edge for `go.sum` components (typical
/// in `--offline` + empty cache + `// indirect` requires), the SBOM
/// MUST signal that limitation so consumers can distinguish "dead dep"
/// from "couldn't resolve."
///
/// Serializes as kebab-case for the document-level annotation values:
/// `complete` / `partial`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphCompleteness {
    /// All Go components emitted from `go.sum` have at least one
    /// incoming `dependsOn` edge. The graph is fully connected from
    /// the workspace root through transitive components.
    Complete,
    /// One or more Go components are orphans (no incoming
    /// `dependsOn`). Per-orphan reason carried via the milestone 061
    /// per-component `mikebom:orphan-reason` annotation.
    Partial,
}

impl ScanDiagnostics {
    /// Record a missing os-release field. No-op if the same field was
    /// already recorded — preserves idempotency for readers that check
    /// the same field multiple times within a single scan.
    pub fn record_missing_os_release_field(&mut self, field: &str) {
        if !self.os_release_missing_fields.iter().any(|f| f == field) {
            self.os_release_missing_fields.push(field.to_string());
        }
    }
}

/// Insert a claimed path into the set in BOTH raw and parent-canonical
/// forms AND (on unix) record the file's (device, inode) tuple.
///
/// The raw path form matches walker paths on plain (non-usrmerge)
/// rootfs. The parent-canonical form handles directory-level symlinks
/// (`/bin → usr/bin`). The (dev, inode) tuple handles final-component
/// symlinks and hard links — any two paths pointing to the same
/// physical file share the same tuple, bypassing path-form quirks
/// entirely.
///
/// Parent canonicalization rather than full-path canonicalization
/// because the file itself might not exist at claim time (some
/// `.list` entries reference files removed post-install), but the
/// parent directory's symlink resolution is stable and cheap.
pub(crate) fn insert_claim_with_canonical(
    claimed: &mut std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut std::collections::HashSet<(u64, u64)>,
    abs_path: std::path::PathBuf,
) {
    if let (Some(parent), Some(basename)) = (abs_path.parent(), abs_path.file_name()) {
        if let Ok(canonical_parent) = std::fs::canonicalize(parent) {
            let canonical = canonical_parent.join(basename);
            if canonical != abs_path {
                claimed.insert(canonical);
            }
        }
    }
    // Record (dev, inode) of both the symlink itself AND its resolved
    // target. If dpkg lists the symlink, walker walking the target
    // still matches via target's inode. If dpkg lists the target,
    // walker walking the symlink still matches via symlink's inode
    // (which in Unix semantics IS the target's inode — symlinks don't
    // have their own inode in the directory-entry sense; `metadata`
    // follows symlinks and `symlink_metadata` reveals the symlink
    // itself, which has its own inode on the filesystem).
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(&abs_path) {
            claimed_inodes.insert((meta.dev(), meta.ino()));
        }
        if let Ok(meta) = std::fs::metadata(&abs_path) {
            claimed_inodes.insert((meta.dev(), meta.ino()));
        }
    }
    claimed.insert(abs_path);
}

/// G3 — filter `pkg:golang` source-tier emissions (from
/// `golang.rs`'s go.sum parsing) against the analyzed-tier set
/// produced by `go_binary.rs`'s BuildInfo extraction. When at least
/// one Go binary was scanned, retain only source-tier entries whose
/// `(name, version)` the BuildInfo confirms as linked. Source-tree-
/// only scans (no Go binary present → empty analyzed set) no-op.
///
/// go.sum lists every module the resolver ever touched, including
/// test-only transitives and indirect deps. BuildInfo lists what
/// the linker actually embedded in the compiled binary. When both
/// are available, BuildInfo is authoritative for "what ships" on
/// image-style scans (polyglot-builder-image was emitting 22
/// source-tier golang entries, only 7 of which were in any
/// scanned binary's BuildInfo — the other 15 were test/tool deps
/// that never linked).
///
/// Runs post-reader, pre-dedup. The existing Go-specific tier-
/// preference in `resolve::deduplicator::deduplicate` (source wins
/// over analyzed on same-coord collision) still applies to
/// surviving entries.
/// G4 (feature 007 US2 → milestone 049): tag `pkg:golang` source-tier
/// entries that are imported only from `_test.go` files with
/// `is_dev = Some(true)`, and drop tagged entries when `--include-dev`
/// is off (mirrors npm/Poetry/Pipfile semantics).
///
/// Pre-milestone-049 behavior was "drop everything not in the project's
/// direct prod imports", which collapsed legitimate transitive prod
/// deps (e.g., aws-sdk-go-v2 internals) into the test-only bucket.
/// Milestone 049 inverts the default: every go.sum entry is emitted
/// (FR-001), then we *only* tag the small subset that source-walking
/// proves is test-only — `test_imports - production_imports` at this
/// project's level. Indirect transitives (in go.sum, not directly
/// imported by either prod or test source here) pass through as prod.
///
/// We deliberately do NOT BFS through deps' go.mod `require` blocks:
/// a dep can declare a module in its own go.mod purely for its tests
/// (logrus declares testify), but that doesn't mean a downstream
/// consumer loads it in prod. Source-import walking at the project
/// boundary is the trustworthy signal.
///
/// The filter no-ops when `production_imports` is empty (pure-binary
/// scans with no .go source to parse) — G3 alone already handles
/// those correctly.
fn apply_go_production_set_filter(
    entries: &mut Vec<PackageDbEntry>,
    production_imports: &std::collections::HashSet<String>,
    test_only_imports: &std::collections::HashSet<String>,
    include_dev: bool,
) {
    if production_imports.is_empty() && test_only_imports.is_empty() {
        return;
    }
    let mut tagged_test_only = 0usize;
    for e in entries.iter_mut() {
        if e.purl.ecosystem() != "golang" {
            continue;
        }
        if e.sbom_tier.as_deref() != Some("source") {
            // Analyzed-tier (BuildInfo) entries pass through; G3 is
            // their authority.
            continue;
        }
        if test_only_imports.contains(&e.name) {
            // Milestone 052/part-2: Go's test-only-import detection
            // (per milestone 049's `_test.go` import-walk) maps to
            // the standards-defined `Test` scope. SPDX 2.3 emits
            // `TEST_DEPENDENCY_OF`; SPDX 3 emits
            // `lifecycleScope: "test"`; CDX emits
            // `scope: "excluded"` + `mikebom:lifecycle-scope: "test"`.
            e.lifecycle_scope = Some(mikebom_common::resolution::LifecycleScope::Test);
            tagged_test_only += 1;
        }
    }

    // Honor --include-dev: when off, drop tagged entries entirely.
    let mut dropped = 0usize;
    if !include_dev {
        let before = entries.len();
        entries.retain(|e| {
            if e.purl.ecosystem() != "golang" {
                return true;
            }
            if e.sbom_tier.as_deref() != Some("source") {
                return true;
            }
            !mikebom_common::resolution::lifecycle_scope_is_legacy_dev(&e.lifecycle_scope)
        });
        dropped = before.saturating_sub(entries.len());
    }

    if tagged_test_only + dropped > 0 {
        tracing::info!(
            tagged_test_only,
            dropped_when_no_include_dev = dropped,
            production_imports = production_imports.len(),
            include_dev,
            "G4 classifier: tagged Go test-only modules; dropped tagged entries when --include-dev=off",
        );
    }
}

/// G5 (feature 007 US3): drop `pkg:golang` entries whose module path
/// matches the project's own go.mod `module` directive or a Go
/// binary's BuildInfo `mod` line. A project is never its own
/// dependency (spec FR-010 through FR-012).
///
/// Applies to ALL tiers (source + analyzed) — unlike G3/G4 which only
/// touch source-tier entries. BuildInfo emits the main module as an
/// analyzed-tier entry; the project-self filter must strip it
/// regardless of tier.
///
/// Milestone 053 FR-009: when an entry carries
/// `mikebom:component-role: main-module` in its `extra_annotations`,
/// PRESERVE it — that's the new synthetic main-module component
/// emitted by `golang::build_main_module_entry()` per FR-001a, and
/// the SBOM is supposed to have it. The filter still drops every
/// OTHER entry whose name matches a main module — typically the
/// binary BuildInfo's emission of the same module path, which is
/// now redundant given the source-tree synthetic entry. Dedup
/// precedence: source-tree synthetic wins, binary-derived main-
/// module is dropped silently (FR-009).
fn apply_go_main_module_filter(
    entries: &mut Vec<PackageDbEntry>,
    main_modules: &std::collections::HashSet<String>,
) {
    if main_modules.is_empty() {
        return;
    }
    let before = entries.len();
    entries.retain(|e| {
        if e.purl.ecosystem() != "golang" {
            return true;
        }
        if !main_modules.contains(&e.name) {
            return true;
        }
        // Match for the project's own main module. Preserve only the
        // synthetic main-module entry (carries the C40 role tag);
        // drop binary BuildInfo's redundant emission and any other
        // self-reference per FR-009 dedup.
        e.extra_annotations
            .get("mikebom:component-role")
            .and_then(|v| v.as_str())
            == Some("main-module")
    });
    let dropped = before.saturating_sub(entries.len());
    if dropped > 0 {
        tracing::info!(
            dropped,
            main_modules = main_modules.len(),
            "G5 filter: dropped main-module self-references (preserving synthetic main-module entries per milestone 053 FR-009)",
        );
    }
}

/// G3 (milestone 050 redesign): when a Go binary is present in the
/// scanned rootfs, TAG every `pkg:golang` source-tier entry whose
/// `(name, version)` is NOT in the binary's BuildInfo with a
/// `mikebom:not-linked = true` property. The annotation tells SBOM
/// consumers "this module was in go.sum but the linker did not embed
/// it in the compiled binary in this rootfs" — a precise signal for
/// scope-narrowing without throwing the data away.
///
/// Pre-milestone-050 behavior was to DROP non-linked entries (which
/// silently lost data — consumers had no way to recover it). The
/// new design preserves the data, lets consumers filter on the
/// annotation, and aligns with the milestone 049 pattern of
/// "tag-don't-drop" for test-only deps.
///
/// No-ops when no Go binary was scanned (linked set empty).
fn apply_go_linked_filter(entries: &mut [PackageDbEntry]) {
    let linked: std::collections::HashSet<(String, String)> = entries
        .iter()
        .filter(|e| {
            e.purl.ecosystem() == "golang"
                && e.sbom_tier.as_deref() == Some("analyzed")
        })
        .map(|e| (e.name.clone(), e.version.clone()))
        .collect();
    if linked.is_empty() {
        // No Go binary was scanned — pure source-tree path.
        // Nothing to tag against.
        return;
    }
    let mut tagged = 0usize;
    for e in entries.iter_mut() {
        if e.purl.ecosystem() != "golang" {
            continue;
        }
        if e.sbom_tier.as_deref() != Some("source") {
            continue;
        }
        // Milestone 053 FR-010: the synthetic main-module component
        // (carrying `mikebom:component-role: main-module`) is the
        // linker root by definition, never a non-linked dep. Skip it
        // unconditionally — the binary's BuildInfo doesn't list the
        // main module among its `Deps[]`, so without this guard the
        // not-linked filter would falsely tag the project itself.
        if e.extra_annotations
            .get("mikebom:component-role")
            .and_then(|v| v.as_str())
            == Some("main-module")
        {
            continue;
        }
        if !linked.contains(&(e.name.clone(), e.version.clone())) {
            e.extra_annotations.insert(
                "mikebom:not-linked".to_string(),
                serde_json::Value::Bool(true),
            );
            tagged += 1;
        }
    }
    if tagged > 0 {
        tracing::info!(
            tagged,
            linked_count = linked.len(),
            "G3 filter: tagged go.sum entries not confirmed by Go binary BuildInfo with mikebom:not-linked",
        );
    }
}

/// Try every supported database reader against `rootfs` and return all
/// successful entries. Missing db files are not an error — a rootfs
/// with no apt/apk is just empty output. Only fail-closed errors (npm
/// v1 lockfile per FR-006) propagate as `Err`.
///
/// * `rootfs` — absolute path to a rootfs directory (the output of
///   `docker_image::extract` or a user-supplied `--path`).
/// * `deb_codename` — used to stamp the `distro=` qualifier on deb
///   PURLs when present.
pub fn read_all(
    rootfs: &Path,
    _deb_codename: Option<&str>,
    include_dev: bool,
    include_legacy_rpmdb: bool,
    scan_mode: crate::scan_fs::ScanMode,
    include_declared_deps: bool,
    scan_target_name: Option<&str>,
) -> Result<DbScanResult, PackageDbError> {
    // Milestone 102 FR-016: opt-in vendored-dep emission for CMake
    // `add_subdirectory(third_party/...)`. Read via env var so the
    // CLI flag in scan_cmd can set it without plumbing through the
    // 75-callsite `scan_path` -> `read_all` signature chain. Default
    // off; `--include-vendored` CLI flag sets `MIKEBOM_INCLUDE_VENDORED=1`.
    let include_vendored = std::env::var("MIKEBOM_INCLUDE_VENDORED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut out = Vec::new();
    let mut claimed: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    #[cfg(unix)]
    let mut claimed_inodes: std::collections::HashSet<(u64, u64)> =
        std::collections::HashSet::new();
    let mut diagnostics = ScanDiagnostics::default();

    // Feature 005 US2/US3: read os-release once per scan. `ID`
    // drives the deb/rpm/apk PURL namespace + distro-qualifier prefix
    // (falls back to `debian` when missing, with diagnostic emitted).
    // `VERSION_ID` becomes the version half of the qualifier (omitted
    // when missing). Both are recorded in ScanDiagnostics so the SBOM
    // surfaces whichever were missing in `metadata.properties`.
    //
    // v6 fix (conformance bug 1): use the rootfs-aware reader which
    // tries `/etc/os-release` first and falls back to
    // `/usr/lib/os-release` (per the os-release spec) when the primary
    // is missing. Ubuntu 24.04 ships `/etc/os-release` as a relative
    // symlink to `../usr/lib/os-release`; some layer-reorderings during
    // container-image extraction can leave the symlink dangling, which
    // was causing Ubuntu images to fall back to the `debian` namespace.
    let id_raw = crate::scan_fs::os_release::read_id_from_rootfs(rootfs);
    let distro_version =
        crate::scan_fs::os_release::read_version_id_from_rootfs(rootfs);
    let deb_namespace: String = match &id_raw {
        Some(id) if !id.is_empty() => id.to_ascii_lowercase(),
        _ => {
            diagnostics.record_missing_os_release_field("ID");
            "debian".to_string()
        }
    };
    if distro_version.is_none() {
        diagnostics.record_missing_os_release_field("VERSION_ID");
    }

    match dpkg::read(rootfs, &deb_namespace, distro_version.as_deref()) {
        Ok(entries) => {
            out.extend(entries);
            // Milestone 004 post-ship: collect dpkg-owned file paths
            // (from /var/lib/dpkg/info/*.list) + inodes. Drives the
            // binary walker's skip gate so /usr/bin/base64 et al.
            // don't produce duplicate pkg:generic/ components.
            dpkg::collect_claimed_paths(
                rootfs,
                &mut claimed,
                #[cfg(unix)]
                &mut claimed_inodes,
            );
        }
        Err(e) => tracing::debug!(error = %e, "dpkg db read failed (expected if no dpkg)"),
    }
    match apk::read(rootfs, distro_version.as_deref()) {
        Ok(entries) => {
            out.extend(entries);
            // Milestone 004 post-ship: collect apk-owned file paths.
            apk::collect_claimed_paths(
                rootfs,
                &mut claimed,
                #[cfg(unix)]
                &mut claimed_inodes,
            );
        }
        Err(e) => tracing::debug!(error = %e, "apk db read failed (expected if no apk)"),
    }

    // Python: venv dist-info + lockfiles + requirements.txt per R13 tiers.
    // No fail-closed: an empty Python section is fine if the scan root
    // doesn't contain any Python artefacts.
    out.extend(pip::read(rootfs, include_dev));
    // Collect pip-claimed paths from dist-info RECORD files.
    pip::collect_claimed_paths(
        rootfs,
        &mut claimed,
        #[cfg(unix)]
        &mut claimed_inodes,
    );

    // Node.js: the reader dispatches lockfile > node_modules > root
    // package.json internally.
    //
    // Milestone 105 phase 2G (SC-008, T026): unsupported v1 lockfiles
    // are now warn-and-skip at the dispatcher level rather than fatal.
    // The change preserves polyglot-safety per FR-014: a stray legacy
    // v1 lockfile sitting in `third_party/<deep>/package-lock.json` of
    // a C/C++ project (the gRPC scenario that motivated this) MUST
    // NOT abort the whole scan and prevent the C/C++ readers from
    // emitting their components. The npm reader's `NpmError` type and
    // its direct `read()` API still expose the v1-unsupported case as
    // `Err` for callers that want to handle it explicitly; the
    // dispatcher just chooses warn-and-continue for the polyglot
    // safety case.
    match npm::read(rootfs, include_dev, scan_mode) {
        Ok(entries) => out.extend(entries),
        Err(npm::NpmError::LockfileV1Unsupported { path }) => {
            tracing::warn!(
                path = %path.display(),
                "npm package-lock.json v1 unsupported; skipping npm reader for this scan. Regenerate with npm >= 7 to include this project's npm dependencies (FR-014 + SC-008)"
            );
        }
    }

    // Milestone 003 ecosystem readers. Concrete implementations land in
    // the per-story tasks (US1 Go, US2 RPM, US3 Maven, US4 Cargo, US5
    // Gem). The stubs below return empty vectors today so the dispatcher
    // compose-order is settled and future story work only needs to touch
    // the individual reader module — no revisit of `read_all`.
    // Milestone 055 (T010 / T024 / T025): the new Go transitive-edge
    // resolver needs the global `--offline` flag to gate ladder steps 1
    // (`go mod graph`) and 3 (proxy fetch) per spec FR-005. The flag
    // lives at `cli::scan_cmd::offline` (line ~583) but is not yet
    // threaded through `scan_path` → `read_all` → `golang::read`. T024
    // and T025 will plumb it via a new parameter on `read_all` (and on
    // `scan_path` upstream); until then the resolver assumes
    // `offline=false` (network-permitted), which matches the default
    // CLI behavior. Empty `$GOMODCACHE` + missing `go` toolchain still
    // triggers the proxy-fetch path, so the headline US1 behavior works
    // even without the flag being threaded — T010 records this as a
    // known limitation.
    let (golang_entries, go_signals) = golang::read(rootfs, include_dev);
    out.extend(golang_entries);

    // Milestone 061 (closes #119): propagate the Go ecosystem's
    // graph-completeness aggregate from `golang::read()`'s GoScanSignals
    // into the document-level `ScanDiagnostics` that flows up to the
    // format emitters. Per spec FR-005/FR-006/FR-007, each format's
    // metadata builder reads these fields and emits the doc-level
    // `mikebom:graph-completeness` + `*-reason` annotations.
    diagnostics.go_graph_completeness = go_signals.graph_completeness;
    if !go_signals.graph_completeness_reasons.is_empty() {
        // Tokens carry the bare reason class from legacy.rs; prefix
        // with the ecosystem name here so the final annotation value
        // disambiguates across multi-ecosystem scans (`go:<class>`).
        let prefixed: Vec<String> = go_signals
            .graph_completeness_reasons
            .iter()
            .map(|r| format!("go:{r}"))
            .collect();
        diagnostics.go_graph_completeness_reason = Some(prefixed.join(","));
    }
    out.extend(rpm::read(rootfs, include_dev, distro_version.as_deref()));
    // v5 Phase B: rpm-owned file claim-skip — mirrors the dpkg / apk /
    // pip pattern. Real RHEL / Fedora rpmdbs store file paths inside
    // the header blob (BASENAMES / DIRNAMES / DIRINDEXES tags); the
    // paths get reconstructed via `rpm_header::parse_header_blob` and
    // inserted with `insert_claim_with_canonical`.
    rpm::collect_claimed_paths(
        rootfs,
        &mut claimed,
        #[cfg(unix)]
        &mut claimed_inodes,
    );
    // v9 Phase O: go_binary runs AFTER rpm's claim-path collection so
    // its diagnostic emissions (Unsupported / Missing BuildInfo) can
    // be suppressed for Go toolchain binaries owned by an rpm/deb/apk
    // package. Without the reorder, the claim set would be empty at
    // the time go_binary iterates, and golang-owned `link`/`compile`/
    // `asm` tools (which ship with intentionally unreadable BuildInfo)
    // would leak as `pkg:generic/link` etc.
    let (go_binary_entries, go_binary_main_modules) = go_binary::read(
        rootfs,
        include_dev,
        &claimed,
        #[cfg(unix)]
        &claimed_inodes,
    );
    // Milestone 050: capture binary count BEFORE moving entries
    // into `out`, for the source-tree-no-binary scope hint emitted
    // after the G3/G4/G5 chain finishes.
    let go_binary_entries_count = go_binary_entries.len();
    out.extend(go_binary_entries);
    // Milestone 004 US1: standalone `.rpm` artefact reader (stub until
    // T015–T018 land). No-op today; wiring in place so the dispatcher
    // is settled and future story work only touches rpm_file.rs.
    out.extend(rpm_file::read(rootfs, distro_version.as_deref()));
    // Milestone 004 US4: legacy BDB rpmdb reader (stub until T061–T065
    // land). Gated behind --include-legacy-rpmdb; no-op when flag unset.
    out.extend(rpmdb_bdb::read(rootfs, include_legacy_rpmdb));
    let (maven_entries, scan_target_coord) = maven::read_with_claims(
        rootfs,
        include_dev,
        include_declared_deps,
        &claimed,
        #[cfg(unix)]
        &claimed_inodes,
        scan_target_name,
    );
    out.extend(maven_entries);
    // Milestone 106 US3 (closes #277): Gradle source-tree readers
    // (`gradle.lockfile` + `buildscript-gradle.lockfile`). Emits
    // `pkg:maven/<g>/<a>@<v>` PURLs so it shares deps.dev enrichment
    // with the existing Maven path. Buildscript entries carry
    // `LifecycleScope::Build` so the existing milestone-052 emission
    // path tags them `scope: "excluded"` (CDX) /
    // `BUILD_DEPENDENCY_OF` (SPDX 2.3) automatically.
    out.extend(gradle::read(rootfs));
    // Milestone 106 US4 (closes #275): NuGet source-tree reader for
    // `.csproj` / `.vbproj` / `.fsproj` with packages.lock.json
    // precedence, Directory.Packages.props (CPM) fallback, and
    // PrivateAssets-driven lifecycle-scope mapping. Emits
    // `pkg:nuget/<name>@<version>` PURLs feeding the existing
    // deps.dev enrichment path.
    out.extend(nuget::read(rootfs));
    // Cargo is fail-closed on v1/v2 lockfiles (FR-040), mirroring the
    // npm v1 refusal pattern.
    out.extend(cargo::read(rootfs, include_dev)?);
    out.extend(gem::read(rootfs, include_dev));

    // Milestone 102: C/C++ source-tree readers (Bazel + CMake +
    // vcpkg + Conan). Skip-with-warn on parse errors per FR-015;
    // cross-platform (no `#[cfg(unix)]` per FR-013); zero new
    // Cargo deps (workspace `regex` + `serde_json` reused).
    out.extend(bazel::read(rootfs));
    out.extend(cmake::read(rootfs, include_vendored));
    out.extend(vcpkg::read(rootfs));
    out.extend(conan::read(rootfs));

    // G3: when a scan produced BOTH `pkg:golang` source-tier entries
    // (from `golang.rs`'s go.sum parsing) AND `pkg:golang` analyzed-
    // tier entries (from `go_binary.rs`'s BuildInfo extraction),
    // filter the source-tier emissions to only those coords the
    // BuildInfo confirms as linked.
    //
    // Rationale: go.sum is a resolver-touched manifest — it includes
    // test-only transitives, indirect deps, and anything
    // `go mod tidy` ever fetched. BuildInfo lists what the linker
    // actually embedded in the compiled binary. On image scans that
    // carry both, BuildInfo is authoritative for "what ships."
    //
    // Source-tree-only scans (no Go binary present) are unchanged:
    // the filter no-ops when the analyzed set is empty, and go.sum
    // remains the only signal.
    apply_go_linked_filter(&mut out);

    // G4 (feature 007 US2 → milestone 049): tag test-only Go
    // entries with is_dev=Some(true) and drop them when
    // --include-dev=off. Pre-milestone-049 dropped test-only
    // unconditionally; now the full transitive prod closure is
    // emitted by default and test-only deps are filterable via the
    // existing --include-dev flag (matches npm/Poetry/Pipfile
    // semantics). When no Go source is parsed (transitive_prod_set
    // empty), this no-ops and G3 alone drives Go filtering.
    apply_go_production_set_filter(
        &mut out,
        &go_signals.production_imports,
        &go_signals.test_only_imports,
        include_dev,
    );

    // G5 (feature 007 US3): drop the project's own main module from
    // the dependency listing. Main modules come from BOTH go.mod
    // `module` directives (via `golang::read`) AND binary BuildInfo
    // `mod` lines (via `go_binary::read`); union for safety when a
    // rootfs carries multiple projects.
    let main_modules: std::collections::HashSet<String> = go_signals
        .main_modules
        .iter()
        .chain(go_binary_main_modules.iter())
        .cloned()
        .collect();
    apply_go_main_module_filter(&mut out, &main_modules);

    // Milestone 050: source-tree Go scan with go.mod parsed but no
    // built Go binary present. Without a binary, mikebom can't
    // distinguish modules that the linker actually embedded from
    // modules that are merely in go.sum (build-tag alternatives,
    // test scaffolding). When a binary IS present, G3 tags every
    // non-BuildInfo go.sum entry with `mikebom:not-linked = true`
    // so consumers can filter precisely. Emit a single
    // tracing::info hint so users know to tighten their workflow.
    // Gated to --path scans because --image scans don't give the
    // user an opportunity to run `go build`.
    if !go_signals.main_modules.is_empty()
        && go_binary_entries_count == 0
        && matches!(scan_mode, crate::scan_fs::ScanMode::Path)
    {
        let go_sum_components = out
            .iter()
            .filter(|e| {
                e.purl.ecosystem() == "golang"
                    && e.sbom_tier.as_deref() == Some("source")
            })
            .count();
        tracing::info!(
            go_modules = go_signals.main_modules.len(),
            go_sum_components,
            "no Go binary found alongside go.mod — every go.sum \
             entry is emitted unmarked. Run `go build` and re-scan \
             to annotate non-linked entries with mikebom:not-linked.",
        );
    }

    Ok(DbScanResult {
        entries: out,
        claimed_paths: claimed,
        #[cfg(unix)]
        claimed_inodes,
        diagnostics,
        scan_target_coord,
    })
}

/// Map an `/etc/os-release::ID` value to the PURL vendor segment used
/// for `pkg:rpm/<vendor>/...` components, per milestone 003 research R8.
///
/// The mapping covers the nine ID values mikebom commits to supporting
/// in milestone 003:
///
/// | `ID=` | `<vendor>` |
/// |---|---|
/// | `rhel` | `redhat` |
/// | `centos` | `centos` |
/// | `fedora` | `fedora` |
/// | `rocky` | `rocky` |
/// | `almalinux` | `almalinux` |
/// | `amzn` | `amazon` |
/// | `ol` | `oracle` |
/// | `opensuse-leap` / `opensuse-tumbleweed` / `opensuse` | `opensuse` |
/// | `sles` | `suse` |
///
/// Any other value is returned verbatim (preserving whatever the distro
/// wrote in its os-release) so an unmapped distro still produces a
/// deterministic — if unfamiliar — PURL. This is the contract: the
/// scanner never invents a vendor, it just normalises the ones it
/// recognises.
pub fn rpm_vendor_from_id(id: &str) -> String {
    match id {
        "rhel" => "redhat".to_string(),
        "centos" => "centos".to_string(),
        "fedora" => "fedora".to_string(),
        "rocky" => "rocky".to_string(),
        "almalinux" => "almalinux".to_string(),
        "amzn" => "amazon".to_string(),
        "ol" => "oracle".to_string(),
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" => "opensuse".to_string(),
        "sles" => "suse".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn rpm_vendor_maps_rhel_family() {
        assert_eq!(rpm_vendor_from_id("rhel"), "redhat");
        assert_eq!(rpm_vendor_from_id("centos"), "centos");
        assert_eq!(rpm_vendor_from_id("fedora"), "fedora");
        assert_eq!(rpm_vendor_from_id("rocky"), "rocky");
        assert_eq!(rpm_vendor_from_id("almalinux"), "almalinux");
        assert_eq!(rpm_vendor_from_id("ol"), "oracle");
    }

    #[test]
    fn rpm_vendor_maps_amazon_linux() {
        assert_eq!(rpm_vendor_from_id("amzn"), "amazon");
    }

    #[test]
    fn rpm_vendor_maps_suse_family() {
        assert_eq!(rpm_vendor_from_id("opensuse-leap"), "opensuse");
        assert_eq!(rpm_vendor_from_id("opensuse-tumbleweed"), "opensuse");
        assert_eq!(rpm_vendor_from_id("opensuse"), "opensuse");
        assert_eq!(rpm_vendor_from_id("sles"), "suse");
    }

    #[test]
    fn rpm_vendor_unmapped_id_returns_verbatim() {
        // Mageia is RPM-based but not in the committed map; assert the
        // verbatim fallback so the scanner still produces a deterministic
        // PURL rather than silently misattributing the packages.
        assert_eq!(rpm_vendor_from_id("mageia"), "mageia");
        assert_eq!(rpm_vendor_from_id("openmandriva"), "openmandriva");
    }

    #[test]
    fn rpm_vendor_preserves_empty_input() {
        // Defensive: an empty ID shouldn't silently become anything
        // meaningful. Caller is responsible for treating `""` as
        // "ecosystem unknown" at the read-site.
        assert_eq!(rpm_vendor_from_id(""), "");
    }

    /// T035 — when `/etc/os-release` is absent, `read_all` must fall
    /// back to `namespace = "debian"` AND record `"ID"` in
    /// diagnostics. Same test also covers the VERSION_ID-missing
    /// diagnostic since both fields are derived from the same file.
    #[test]
    fn read_all_falls_back_to_debian_namespace_when_id_missing() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path();
        // dpkg status planted, /etc/os-release intentionally absent.
        let dpkg_dir = rootfs.join("var/lib/dpkg");
        std::fs::create_dir_all(&dpkg_dir).unwrap();
        std::fs::write(
            dpkg_dir.join("status"),
            "\
Package: curl
Status: install ok installed
Version: 8.0.0
Architecture: arm64
",
        )
        .unwrap();

        let result = read_all(
            rootfs,
            None,
            false,
            false,
            crate::scan_fs::ScanMode::Path,
            true, None,
        )
        .unwrap();

        let deb_entries: Vec<_> = result
            .entries
            .iter()
            .filter(|e| e.purl.as_str().starts_with("pkg:deb/"))
            .collect();
        assert!(!deb_entries.is_empty(), "expected at least one deb entry");
        for e in &deb_entries {
            assert!(
                e.purl.as_str().starts_with("pkg:deb/debian/"),
                "expected debian fallback namespace, got {}",
                e.purl.as_str()
            );
            // No distro qualifier because VERSION_ID is also missing.
            assert!(
                !e.purl.as_str().contains("distro="),
                "expected no distro qualifier when VERSION_ID missing, got {}",
                e.purl.as_str()
            );
        }
        assert!(
            result
                .diagnostics
                .os_release_missing_fields
                .iter()
                .any(|f| f == "ID"),
            "expected diagnostics to record missing ID"
        );
        assert!(
            result
                .diagnostics
                .os_release_missing_fields
                .iter()
                .any(|f| f == "VERSION_ID"),
            "expected diagnostics to record missing VERSION_ID"
        );
    }

    // --- G3: filter go.sum against BuildInfo ----------------------------

    fn make_entry(
        purl_str: &str,
        name: &str,
        version: &str,
        sbom_tier: Option<&str>,
    ) -> PackageDbEntry {
        PackageDbEntry {
            purl: Purl::new(purl_str).expect("valid purl"),
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: String::new(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: None,
            requirement_range: None,
            source_type: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            raw_version: None,
            parent_purl: None,
            npm_role: None,
            co_owned_by: None,
            hashes: Vec::new(),
            sbom_tier: sbom_tier.map(String::from),
            shade_relocation: None,
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    #[test]
    fn g3_tags_go_sum_entries_without_buildinfo_match() {
        // Milestone 050: G3 tags non-BuildInfo entries with
        // mikebom:not-linked rather than dropping them. Three
        // source-tier Go entries (from go.sum). Two analyzed-tier
        // (from BuildInfo) — only `logrus` overlaps. Plus non-Go
        // entries that must pass through untouched.
        let mut entries = vec![
            make_entry(
                "pkg:golang/github.com/davecgh/go-spew@v1.1.1",
                "github.com/davecgh/go-spew",
                "v1.1.1",
                Some("source"),
            ),
            make_entry(
                "pkg:golang/github.com/stretchr/testify@v1.7.0",
                "github.com/stretchr/testify",
                "v1.7.0",
                Some("source"),
            ),
            make_entry(
                "pkg:golang/github.com/sirupsen/logrus@v1.9.3",
                "github.com/sirupsen/logrus",
                "v1.9.3",
                Some("source"),
            ),
            make_entry(
                "pkg:golang/github.com/sirupsen/logrus@v1.9.3",
                "github.com/sirupsen/logrus",
                "v1.9.3",
                Some("analyzed"),
            ),
            make_entry(
                "pkg:golang/golang.org/x/sys@v0.0.0-20220715",
                "golang.org/x/sys",
                "v0.0.0-20220715",
                Some("analyzed"),
            ),
            make_entry(
                "pkg:maven/com.google.guava/guava@32.1.3-jre",
                "guava",
                "32.1.3-jre",
                Some("source"),
            ),
            make_entry(
                "pkg:cargo/serde@1.0.0",
                "serde",
                "1.0.0",
                Some("source"),
            ),
        ];

        apply_go_linked_filter(&mut entries);

        let lookup = |name: &str, tier: &str| -> Option<&PackageDbEntry> {
            entries.iter().find(|e| {
                e.name == name && e.sbom_tier.as_deref() == Some(tier)
            })
        };

        // Milestone 050 FR-001: non-BuildInfo source-tier entries
        // are TAGGED, not dropped.
        let go_spew = lookup("github.com/davecgh/go-spew", "source")
            .expect("go-spew source-tier must be retained (tagged, not dropped)");
        assert_eq!(
            go_spew.extra_annotations.get("mikebom:not-linked"),
            Some(&serde_json::Value::Bool(true)),
            "go-spew must carry mikebom:not-linked = true: \
             extra_annotations={:?}",
            go_spew.extra_annotations,
        );
        let testify = lookup("github.com/stretchr/testify", "source")
            .expect("testify source-tier must be retained (tagged, not dropped)");
        assert_eq!(
            testify.extra_annotations.get("mikebom:not-linked"),
            Some(&serde_json::Value::Bool(true)),
        );

        // Matched source-tier entry → NOT tagged.
        let logrus_source = lookup("github.com/sirupsen/logrus", "source")
            .expect("logrus source-tier must be retained");
        assert!(
            !logrus_source
                .extra_annotations
                .contains_key("mikebom:not-linked"),
            "logrus source-tier must NOT carry mikebom:not-linked \
             (it's in BuildInfo): extra_annotations={:?}",
            logrus_source.extra_annotations,
        );

        // Analyzed-tier entries pass through (G3 only tags
        // source-tier).
        assert!(
            lookup("golang.org/x/sys", "analyzed").is_some(),
            "x/sys analyzed-tier must pass through",
        );

        // Non-Go entries untouched.
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"guava"), "maven must pass through: {names:?}");
        assert!(names.contains(&"serde"), "cargo must pass through: {names:?}");
    }

    #[test]
    fn g3_noop_when_no_buildinfo_present() {
        // Pure source-tree scan: go.sum entries only, no binary
        // analyzed-tier. Filter must no-op — go.sum is the only
        // available signal.
        let mut entries = vec![
            make_entry(
                "pkg:golang/github.com/davecgh/go-spew@v1.1.1",
                "github.com/davecgh/go-spew",
                "v1.1.1",
                Some("source"),
            ),
            make_entry(
                "pkg:golang/github.com/never-in-binary/pkg@v9.9.9",
                "github.com/never-in-binary/pkg",
                "v9.9.9",
                Some("source"),
            ),
        ];

        let before = entries.len();
        apply_go_linked_filter(&mut entries);
        assert_eq!(
            entries.len(),
            before,
            "filter must no-op when no BuildInfo (analyzed) entries present",
        );
    }

    #[test]
    fn g3_filter_doesnt_touch_non_go_ecosystems() {
        // Filter should only affect Go entries even when the
        // `linked` set is non-empty. A Maven / cargo / npm coord
        // that happens to share a name with an absent Go module
        // must NOT be dropped.
        let mut entries = vec![
            // One Go analyzed entry to activate the filter.
            make_entry(
                "pkg:golang/github.com/sirupsen/logrus@v1.9.3",
                "github.com/sirupsen/logrus",
                "v1.9.3",
                Some("analyzed"),
            ),
            // Non-Go source-tier entries — all must survive.
            make_entry(
                "pkg:maven/com.example/never@1.0.0",
                "never",
                "1.0.0",
                Some("source"),
            ),
            make_entry(
                "pkg:cargo/never@1.0.0",
                "never",
                "1.0.0",
                Some("source"),
            ),
            make_entry(
                "pkg:npm/never@1.0.0",
                "never",
                "1.0.0",
                Some("source"),
            ),
            make_entry(
                "pkg:pypi/never@1.0.0",
                "never",
                "1.0.0",
                Some("source"),
            ),
        ];

        let before = entries.len();
        apply_go_linked_filter(&mut entries);
        // All 5 should survive: 4 non-Go + 1 Go analyzed.
        assert_eq!(
            entries.len(),
            before,
            "non-Go ecosystems must be unaffected by G3 filter",
        );
    }

    // --- Milestone 053 FR-009 + FR-010 ------------------------------------

    /// FR-009: when a source-tree main-module entry (carrying
    /// `mikebom:component-role: "main-module"`) AND a binary-derived
    /// main-module entry (same module path, no role tag) both exist,
    /// the G5 filter MUST drop the binary-derived one and preserve
    /// the synthetic source-tree entry.
    #[test]
    fn fr_009_g5_filter_preserves_synthetic_main_module_drops_binary_dup() {
        let mut synthetic = make_entry(
            "pkg:golang/example.com/proj@v1.2.3",
            "example.com/proj",
            "v1.2.3",
            Some("source"),
        );
        synthetic.extra_annotations.insert(
            "mikebom:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        // Binary BuildInfo's redundant emission of the same main module.
        let binary_dup = make_entry(
            "pkg:golang/example.com/proj@v0.0.0",
            "example.com/proj",
            "v0.0.0",
            Some("analyzed"),
        );
        let mut entries = vec![synthetic, binary_dup];
        let main_modules: std::collections::HashSet<String> =
            std::iter::once("example.com/proj".to_string()).collect();
        apply_go_main_module_filter(&mut entries, &main_modules);
        assert_eq!(entries.len(), 1, "binary dup must be dropped");
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str()),
            Some("main-module"),
            "the synthetic main-module entry must survive",
        );
    }

    /// FR-010: the synthetic main-module entry MUST NOT receive the
    /// `mikebom:not-linked` annotation, even when a Go binary's
    /// BuildInfo lists no main-module among its Deps[]. The project
    /// itself is the linker root, never a non-linked dep.
    #[test]
    fn fr_010_g3_filter_excludes_synthetic_main_module_from_not_linked_eligibility() {
        // Linker-confirmed analyzed-tier entry (something else, not
        // the project's own main-module).
        let analyzed = make_entry(
            "pkg:golang/something/linked@v1.0.0",
            "something/linked",
            "v1.0.0",
            Some("analyzed"),
        );
        let mut main_module = make_entry(
            "pkg:golang/example.com/proj@v0.0.0-unknown",
            "example.com/proj",
            "v0.0.0-unknown",
            Some("source"),
        );
        main_module.extra_annotations.insert(
            "mikebom:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        let mut entries = vec![analyzed, main_module];
        apply_go_linked_filter(&mut entries);
        // The main-module is NOT in the analyzed/linked set (the
        // binary's BuildInfo never lists itself), so without the
        // FR-010 guard it would be tagged not-linked. Verify the
        // guard skips it.
        let mm = entries
            .iter()
            .find(|e| e.name == "example.com/proj")
            .expect("main-module entry preserved");
        assert!(
            !mm.extra_annotations.contains_key("mikebom:not-linked"),
            "FR-010 violated: main-module was incorrectly tagged \
             mikebom:not-linked. extra_annotations = {:?}",
            mm.extra_annotations,
        );
    }
}
