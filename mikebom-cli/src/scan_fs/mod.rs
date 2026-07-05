//! Filesystem-based SBOM generation.
//!
//! Two entry points:
//! - [`walker::walk_and_hash`] — cross-platform directory traversal that
//!   returns a set of artifact files with their SHA-256 hashes. Shared
//!   with trace mode's post-exit scan.
//! - [`scan_path`] — end-to-end orchestrator: walks a root, runs the
//!   path resolver over each captured file, and returns a
//!   `Vec<ResolvedComponent>` ready for the CycloneDX builder. Used by
//!   the standalone `mikebom sbom scan` subcommand.

pub mod binary;
pub mod dedup;
pub mod docker_daemon;
pub mod docker_image;
// Milestone 133 US1.A scaffolding — orphan file-tier walker,
// content-shape classifier, hybrid dedupe. The submodules are
// pub(crate) and their entry points are reachable from inside
// `scan_fs::file_tier` but the production scan pipeline does NOT
// yet invoke them. US1.B wires the walker into `scan_cmd::scan`
// alongside the new `--file-inventory` CLI flag, multi-format
// SBOM emission, and the new C-rows.
pub(crate) mod file_tier;
#[cfg(feature = "oci-registry")]
pub mod oci_pull;
pub mod os_release;
pub mod package_db;
pub(crate) mod produces_binaries;
pub mod sbom_path;
pub(crate) mod walk;
pub mod walker;

use std::path::Path;

use mikebom_common::resolution::{
    EnrichmentProvenance, Relationship, RelationshipType, ResolutionEvidence,
    ResolutionTechnique, ResolvedComponent,
};

use crate::generate::cpe::synthesize_cpes;
use crate::resolve::deduplicator::{canonicalize_source_files_by_purl, deduplicate};
use crate::resolve::path_resolver::resolve_path_with_context;

/// Confidence assigned to components discovered by a filesystem walk.
/// Mirrors the value used by the `resolve_path` branch of
/// `ResolutionPipeline` so the resulting SBOM is directly comparable to a
/// trace-sourced one where the artifact-dir scan fired.
pub const FILE_PATH_CONFIDENCE: f64 = 0.70;

/// How the caller invoked mikebom — image-tarball extraction vs. plain
/// directory. Drives scan-mode-aware scoping decisions like npm-internals
/// inclusion (feature 005 US1): `Image` includes npm's own internal
/// packages because the container IS the target; `Path` excludes them
/// because the target is the application, not the tooling that installed
/// its dependencies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanMode {
    /// `mikebom sbom scan --path <dir>` — target is an application source tree.
    Path,
    /// `mikebom sbom scan --image <tarball>` — target is a full container filesystem.
    Image,
}

/// Confidence assigned to components read from an OS installed-package
/// database. Higher than `FILE_PATH_CONFIDENCE` because the db is
/// authoritative about what's installed (not a guess from a filename),
/// and lower than instrumentation (0.95) because we didn't observe the
/// install event itself.
pub const PACKAGE_DB_CONFIDENCE: f64 = 0.85;

/// Everything a scan produces, ready to be fed to the CycloneDX builder.
/// The relationships list is populated from installed-package-database
/// Depends fields (dpkg) when available; it's empty for filesystem
/// walks that only find artefact files.
pub struct ScanResult {
    pub components: Vec<ResolvedComponent>,
    pub relationships: Vec<Relationship>,
    /// PURL ecosystem identifiers (e.g. `"deb"`, `"apk"`) whose installed
    /// database was read in full during this scan. Each listed ecosystem
    /// gets its own `aggregate: complete` record in the CycloneDX
    /// `compositions[]` section — consumers then know the dpkg subset is
    /// authoritative even when the surrounding SBOM is
    /// `incomplete_first_party_only`. Empty when no dbs were read (pure
    /// artefact-file scan or `--no-package-db`).
    pub complete_ecosystems: Vec<String>,
    /// Feature 005 SC-009 — `/etc/os-release` fields the package-db
    /// readers tried to read but found absent/empty. Surfaced into the
    /// SBOM's `metadata.properties` as `mikebom:os-release-missing-fields`
    /// when non-empty. Empty vec means clean scan.
    pub os_release_missing_fields: Vec<String>,
    /// Milestone 061 (closes #119): document-level Go graph-completeness
    /// signal. Propagated from `package_db::ScanDiagnostics` per FR-003.
    /// `None` means no Go scan happened (annotation absent in output).
    pub go_graph_completeness:
        Option<package_db::GraphCompleteness>,
    /// Milestone 061 — comma-separated `<ecosystem>:<reason-class>` list
    /// summarizing why `go_graph_completeness == Partial`. Empty when
    /// completeness is `Complete` or `None`.
    pub go_graph_completeness_reason: Option<String>,
    /// Milestone 160 (T009): document-scope Go-transitive coverage signal
    /// produced by `compute_coverage()` in the milestone-055/091 ladder
    /// resolver. Distinct from `go_graph_completeness` per research.md R1
    /// — C104 signals "did we build a top-level component graph at all?"
    /// while this signals "what fraction of Go modules had per-module
    /// transitive requires resolved via the ladder?". `None` iff no Go
    /// modules were scanned (C110 annotation absent in output).
    pub go_transitive_coverage:
        Option<crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage>,
    /// Milestone 161 (T012): workspace-mode detection outcome from
    /// `go.work` at scanned root. Distinct from
    /// `go_transitive_coverage` and `go_graph_completeness` per
    /// research.md R1. `None` iff no `go.work` at scanned root OR
    /// `GOWORK=off` (C112 annotation absent per SC-003).
    pub go_workspace_mode:
        Option<crate::scan_fs::package_db::golang::gowork::WorkspaceMode>,
    /// M3 — Maven scan-subject coord identified during the JAR walk,
    /// promoted from the `PackageDbEntry` layer to drive CDX
    /// `metadata.component`. `None` when no Maven fat-jar matched
    /// the scan-subject heuristic (non-Java target or simple
    /// standalone JAR layout).
    pub scan_target_coord: Option<package_db::maven::ScanTargetCoord>,
    /// Milestone 134 (closes #125): divergent-PURL collision records
    /// detected during per-ecosystem dedup. When non-empty, the format
    /// emitters surface a document-scope `mikebom:purl-collisions-
    /// detected` annotation aggregating every collision in the scan.
    /// Per-component annotations
    /// (`mikebom:duplicate-purl-divergent`) are already attached to
    /// the owning components' `extra_annotations` bag — this field is
    /// the document-scope twin.
    pub divergence_records:
        Vec<mikebom_common::divergence::DivergenceRecord>,
}

/// Walk `root`, hash matching artifact files, match each against the path
/// resolver, optionally consult OS package databases, and return
/// components + a real dependency graph. The caller (typically the
/// `sbom scan` subcommand) then feeds this into the CycloneDX builder
/// just like the generate-from-attestation path does.
///
/// * `deb_codename` — optional value for the `distro=` qualifier on deb
///   PURLs. Supplied by the CLI or auto-detected from `/etc/os-release`
///   in the scanned root.
/// * `size_cap` — maximum per-file byte count for hashing.
/// * `read_package_db` — when true, attempt to parse
///   `<root>/var/lib/dpkg/status` and `<root>/lib/apk/db/installed` and
///   merge their entries with the artefact-file results. The CLI
///   defaults this to true; pass `--no-package-db` to disable.
/// * `deep_hash` — when true, deep-hash every file each db-installed
///   package owns (via `<pkg>.list`) with SHA-256 and emit per-file
///   occurrences. When false, fall back to a microsecond-cost hash of
///   the dpkg-provided `.md5sums` file content (no per-file detail).
///   Ignored when `read_package_db` is false.
#[allow(clippy::too_many_arguments)] // entry-point flag bundle; keeps caller-side wiring shape stable across milestones.
pub fn scan_path(root: &Path, deb_codename: Option<&str>, size_cap: u64, read_package_db: bool, deep_hash: bool, include_dev: bool, include_legacy_rpmdb: bool, scan_mode: ScanMode, include_declared_deps: bool, scan_target_name: Option<&str>, max_rpm_bytes: Option<u64>, rpm_distro: Option<&str>, exclude_set: &package_db::exclude_path::ExclusionSet) -> Result<ScanResult, ScanError> {
    // Canonicalize the rootfs once at entry so downstream path
    // comparisons use a consistent base. Without this, macOS's
    // `/tmp` → `/private/tmp` symlink (and other host-level symlinks)
    // cause spurious mismatches between walker-emitted paths and
    // package-db claim paths. Milestone 004 post-ship fix.
    let canonical_root: std::path::PathBuf;
    let root = match std::fs::canonicalize(root) {
        Ok(c) => {
            canonical_root = c;
            canonical_root.as_path()
        }
        Err(_) => root,
    };
    // `include_dev` gates inclusion of packages marked dev-only by
    // ecosystems that carry the distinction (npm devDependencies,
    // Poetry `category = "dev"`, Pipfile `develop:`). Threaded through
    // to `package_db::read_all` so per-ecosystem readers can filter at
    // source.
    let artifacts = walker::walk_and_hash(root, None, size_cap);
    let mut components: Vec<ResolvedComponent> = Vec::with_capacity(artifacts.len());

    // Artifact-file walk — confidence 0.70, carries a real SHA-256.
    for artifact in artifacts {
        let path_str = artifact.path.to_string_lossy();
        let Some(purl) = resolve_path_with_context(&path_str, deb_codename) else {
            continue;
        };
        let path_string = path_str.into_owned();
        // G2: the `name` field must match the convention
        // installed-package-db readers use, or dedup misses coords
        // emitted by both paths. For Go, readers set
        // `name = "<namespace>/<last>"` (the full module path —
        // e.g. `github.com/davecgh/go-spew`); `purl.name()` alone
        // returns just the last segment (`go-spew`), which would
        // group differently in the deduplicator's
        // `(ecosystem, name, version, parent_purl)` key. For other
        // ecosystems (Maven, npm, pypi, cargo, etc.) `purl.name()`
        // is the canonical name the reader uses. Only Go needs the
        // namespace prefix here.
        let name = match purl.ecosystem() {
            "golang" => match purl.namespace() {
                Some(ns) => format!("{}/{}", ns, purl.name()),
                None => purl.name().to_string(),
            },
            _ => purl.name().to_string(),
        };
        components.push(ResolvedComponent {
            name,
            version: purl.version().unwrap_or("").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::FilePathPattern,
                confidence: FILE_PATH_CONFIDENCE,
                source_connection_ids: vec![],
                // Milestone 133 US2.1 (FR-012 Defects A + C): normalize the
                // path to rootfs-relative + no-leading-`/` at the moment we
                // record it, so all three SBOM formats (CDX/SPDX 2.3/SPDX 3)
                // get the same clean value (holistic_parity test asserts the
                // C18 row's `SymmetricEqual` directionality across formats).
                //
                // NOTE (milestone 145 US3): `mikebom:source-files` has TWO
                // emission sources — this field (canonical) AND
                // `extra_annotations["mikebom:source-files"]` (legacy,
                // dedup'd at emit time via
                // `root_selector::is_field_owned_annotation_key`). DO NOT
                // stamp the latter from a new reader; if you need to carry
                // per-reader source provenance, use a distinct key like
                // `mikebom:<reader>-source-url`.
                source_file_paths: vec![crate::scan_fs::sbom_path::normalize_sbom_path_relative(
                    &path_string,
                    Some(root),
                )],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![artifact.hash],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            // Artefact-file walks identify packages by filename + content
            // hash but can't tell whether the file is installed: tier =
            // "analyzed" per R13. No dev/prod info, no range spec.
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_range: None,
            source_type: None,
            sbom_tier: Some("analyzed".to_string()),
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
        });
    }

    // Installed-package-db pass — confidence 0.85, no content hash
    // (the installed files are sprayed across /usr and there's no
    // one-hash-per-package source inside the db itself). Also the
    // source of the dependency graph.
    let mut relationships: Vec<Relationship> = Vec::new();
    let mut complete_ecosystems: Vec<String> = Vec::new();
    // Feature 005 SC-009: scan-time diagnostics (missing os-release
    // fields). Carried from DbScanResult into the ScanResult so the
    // CycloneDX metadata builder can surface them.
    let mut os_release_missing_fields: Vec<String> = Vec::new();
    // Milestone 061 (closes #119): doc-level Go graph-completeness
    // signal carried from package_db::ScanDiagnostics through this
    // ScanResult into the format emitters per FR-005/FR-006/FR-007.
    let mut go_graph_completeness:
        Option<package_db::GraphCompleteness> = None;
    let mut go_graph_completeness_reason: Option<String> = None;
    // Milestone 160 (T010): doc-scope go-transitive coverage signal from
    // `compute_coverage()`. Distinct from `go_graph_completeness` per
    // research.md R1. Carried through ScanResult into the format
    // emitters for the C110/C111 doc-scope annotations.
    let mut go_transitive_coverage:
        Option<package_db::golang::graph_resolver::GoTransitiveCoverage> = None;
    // Milestone 161 (T012): doc-scope go-workspace-mode signal.
    // Distinct from `go_transitive_coverage` per research.md R1.
    // Carried through ScanResult into the format emitters for the
    // C112 doc-scope annotation.
    let mut go_workspace_mode:
        Option<package_db::golang::gowork::WorkspaceMode> = None;
    let mut scan_target_coord: Option<package_db::maven::ScanTargetCoord> = None;
    // Milestone 134 — divergent-PURL collision records collected by
    // per-ecosystem dedup. Routed into ScanResult.divergence_records
    // for document-scope `mikebom:purl-collisions-detected` emission.
    let mut divergence_records: Vec<mikebom_common::divergence::DivergenceRecord> =
        Vec::new();
    if read_package_db {
        // Milestone 134 US2 — surface the `--deep-hash` choice to the
        // cargo reader via env var (same plumbing pattern as
        // `MIKEBOM_INCLUDE_VENDORED`; avoids churning the 75-callsite
        // `scan_path -> read_all -> cargo::read` signature chain for
        // an opt-in observability feature).
        if deep_hash {
            std::env::set_var("MIKEBOM_DEEP_HASH", "1");
        } else {
            std::env::remove_var("MIKEBOM_DEEP_HASH");
        }
        // Milestone 144: thread `--max-rpm-bytes` + `--rpm-distro` to
        // the rpm-file reader via env vars (same convention as the
        // DEEP_HASH knob above). The reader builds `RpmReaderConfig`
        // from these in `package_db::read_all`.
        match max_rpm_bytes {
            Some(v) => std::env::set_var("MIKEBOM_MAX_RPM_BYTES", v.to_string()),
            None => std::env::remove_var("MIKEBOM_MAX_RPM_BYTES"),
        }
        match rpm_distro {
            Some(s) => std::env::set_var("MIKEBOM_RPM_DISTRO", s),
            None => std::env::remove_var("MIKEBOM_RPM_DISTRO"),
        }
        let scan_result = package_db::read_all(root, deb_codename, include_dev, include_legacy_rpmdb, scan_mode, include_declared_deps, scan_target_name, exclude_set)?;
        os_release_missing_fields = scan_result.diagnostics.os_release_missing_fields.clone();
        go_graph_completeness = scan_result.diagnostics.go_graph_completeness;
        go_graph_completeness_reason = scan_result.diagnostics.go_graph_completeness_reason.clone();
        go_transitive_coverage = scan_result.diagnostics.go_transitive_coverage.clone();
        go_workspace_mode = scan_result.diagnostics.go_workspace_mode.clone();
        scan_target_coord = scan_result.scan_target_coord.clone();
        divergence_records = scan_result.diagnostics.divergence_records.clone();
        let mut db_entries = scan_result.entries;
        let claimed_paths = scan_result.claimed_paths;
        #[cfg(unix)]
        let claimed_inodes = scan_result.claimed_inodes;
        // Milestone 004 US2 + post-ship claim-skip: generic-binary
        // reader consumes path + inode claim sets populated by dpkg
        // `.list`, apk `R:` / `F:`, and pip RECORD. Binaries whose
        // path OR inode match a claim skip file-level + linkage
        // emissions. `.note.package` + embedded-version-string
        // emissions remain unconditional (TLS preservation).
        db_entries.extend(binary::read(
            root,
            &claimed_paths,
            #[cfg(unix)]
            &claimed_inodes,
            exclude_set,
        ));

        // Record which ecosystems actually had a populated db — each
        // produces its own `aggregate: complete` compositions entry.
        // A pypi entry marks the ecosystem complete when it came from
        // an authoritative source (`sbom_tier` is "deployed" or
        // "source"). Design-tier entries (requirements.txt range specs)
        // do NOT trigger completeness per FR-019 / research R13.
        let mut saw_deb = false;
        let mut saw_apk = false;
        let mut saw_pypi_authoritative = false;
        let mut saw_npm_authoritative = false;
        let mut saw_golang_authoritative = false;
        let mut saw_cargo_authoritative = false;
        let mut saw_gem_authoritative = false;
        let mut saw_maven_authoritative = false;
        let mut saw_rpm_authoritative = false;
        for e in &db_entries {
            if !saw_deb && e.source_path.contains("dpkg/status") {
                saw_deb = true;
            }
            if !saw_apk && e.source_path.contains("apk/db/installed") {
                saw_apk = true;
            }
            if !saw_pypi_authoritative
                && e.purl.ecosystem() == "pypi"
                && matches!(
                    e.sbom_tier.as_deref(),
                    Some("deployed") | Some("source")
                )
            {
                saw_pypi_authoritative = true;
            }
            // npm ecosystem completeness mirrors pypi: only authoritative
            // sources (lockfile = `source`, node_modules = `deployed`)
            // count. `design` tier (root package.json fallback) does NOT
            // trigger completeness — it's unresolved range specs.
            if !saw_npm_authoritative
                && e.purl.ecosystem() == "npm"
                && matches!(
                    e.sbom_tier.as_deref(),
                    Some("deployed") | Some("source")
                )
            {
                saw_npm_authoritative = true;
            }
            // Go ecosystem completeness: either source-tier (go.sum)
            // OR analyzed-tier (binary BuildInfo) counts as
            // authoritative. A scratch/distroless image scan is
            // legitimately "complete" from the binary alone because
            // the binary IS the whole ecosystem there.
            if !saw_golang_authoritative
                && e.purl.ecosystem() == "golang"
                && matches!(
                    e.sbom_tier.as_deref(),
                    Some("analyzed") | Some("source")
                )
            {
                saw_golang_authoritative = true;
            }
            // Cargo ecosystem completeness: Cargo.lock v3/v4 resolves
            // every transitive dep to an exact version + SHA-256, so
            // any source-tier entry marks the ecosystem complete.
            if !saw_cargo_authoritative
                && e.purl.ecosystem() == "cargo"
                && matches!(e.sbom_tier.as_deref(), Some("source"))
            {
                saw_cargo_authoritative = true;
            }
            if !saw_gem_authoritative
                && e.purl.ecosystem() == "gem"
                && matches!(e.sbom_tier.as_deref(), Some("source"))
            {
                saw_gem_authoritative = true;
            }
            // Maven completeness: source-tier pom.xml only. JAR scans
            // are analyzed-tier subsets — not authoritative for the
            // ecosystem. Per T054, a design-tier placeholder
            // dependency does NOT mark the ecosystem complete.
            if !saw_maven_authoritative
                && e.purl.ecosystem() == "maven"
                && matches!(e.sbom_tier.as_deref(), Some("source"))
            {
                saw_maven_authoritative = true;
            }
            // RPM: rpmdb-sourced entries are always deployed-tier and
            // cover the whole installed set, so any rpm entry marks
            // the ecosystem complete.
            if !saw_rpm_authoritative && e.purl.ecosystem() == "rpm" {
                saw_rpm_authoritative = true;
            }
        }
        if saw_deb {
            complete_ecosystems.push("deb".to_string());
        }
        if saw_apk {
            complete_ecosystems.push("apk".to_string());
        }
        if saw_pypi_authoritative {
            complete_ecosystems.push("pypi".to_string());
        }
        if saw_npm_authoritative {
            complete_ecosystems.push("npm".to_string());
        }
        if saw_golang_authoritative {
            complete_ecosystems.push("golang".to_string());
        }
        if saw_cargo_authoritative {
            complete_ecosystems.push("cargo".to_string());
        }
        if saw_gem_authoritative {
            complete_ecosystems.push("gem".to_string());
        }
        if saw_maven_authoritative {
            complete_ecosystems.push("maven".to_string());
        }
        if saw_rpm_authoritative {
            complete_ecosystems.push("rpm".to_string());
        }

        // Index by (ecosystem, canonical-name) for dependency-edge
        // lookup. Keyed per-ecosystem so a `libc6` deb never collides
        // with a hypothetical `libc6` pypi package, and normalised
        // because pypi in particular stores dep tokens in varying
        // case / hyphen-vs-underscore forms (a dist-info `Name:
        // Requests` is referenced as `Requires-Dist: requests` in
        // every other package's metadata).
        let mut name_to_purl: std::collections::HashMap<(String, String), String> =
            std::collections::HashMap::with_capacity(db_entries.len());
        for e in &db_entries {
            let ecosystem = e.purl.ecosystem().to_string();
            name_to_purl.insert(
                (ecosystem.clone(), normalize_dep_name(e.purl.ecosystem(), &e.name)),
                e.purl.as_str().to_string(),
            );
            // Milestone 085 — maven main-modules emit `entry.depends`
            // entries in canonical `groupId:artifactId` form (per
            // `package_db/maven.rs:3451-3455`), but `e.name` is just
            // the artifact-id. Insert the disambiguated key so the
            // edge-emission loop below can resolve maven main-module
            // deps. Without this, every maven main-module → direct-dep
            // lookup misses, and SPDX 2.3 + SPDX 3 emit zero
            // DEPENDS_ON edges for maven (the gap milestone-084's
            // closure-invariant test surfaced via parity_maven row B1).
            if ecosystem == "maven" {
                if let Some(group_id) = e.purl.namespace() {
                    let gav_key = format!("{}:{}", group_id, e.name);
                    name_to_purl.insert(
                        (ecosystem.clone(), normalize_dep_name("maven", &gav_key)),
                        e.purl.as_str().to_string(),
                    );
                }
            }
            // Milestone 087 (issue #172) — cargo entries get a
            // "name version" disambiguation key so that
            // dependencies = ["clap_builder 4.5.21"] -style lookups
            // (used when Cargo.lock has multiple [[package]] blocks
            // for the same crate name) resolve to the correct
            // same-name same-version PURL. Without this, the
            // name-only key would last-write-win between e.g.
            // clap_builder@4.5.9 and clap_builder@4.5.21, producing
            // wrong-version edges in the emitted dep graph.
            // Cargo writes the `<name> <version>` form ONLY when
            // ambiguity exists; single-version deps are still
            // resolved via the existing name-only key above.
            //
            // Issue #262 — same pattern for npm. When a parent
            // package has a nested `node_modules/<parent>/node_modules/<dep>`
            // install, the lockfile parser at
            // `package_db/npm/package_lock.rs` emits the dep as
            // `<dep> <version>` (instead of bare `<dep>`) so this
            // disambiguation key resolves to the nested PURL rather
            // than the hoisted one. Without the nested entry, the
            // bare-name key from line 380 above still wins (matches
            // the hoisted version).
            if ecosystem == "cargo" || ecosystem == "npm" {
                let nv_key = format!("{} {}", e.name, e.version);
                name_to_purl.insert(
                    (ecosystem, normalize_dep_name(e.purl.ecosystem(), &nv_key)),
                    e.purl.as_str().to_string(),
                );
            }
        }

        // Milestone 039: build the apk per-package file-list map
        // once per scan. apk's installed-db carries file ownership
        // inline (F:/R: lines); we extract it here so the per-entry
        // loop can do per-package deep-hashing without re-parsing
        // the database. Returns an empty map for non-apk images
        // (file-not-found short-circuits cheaply) — the unconditional
        // call is fine.
        let apk_file_lists = package_db::apk::read_file_lists(root);
        // Milestone 040 US3: rpm per-file deep-hashing. Build the
        // per-package file-list map once per scan (mirrors the apk
        // pattern from milestone 039). `iter_rpmdb` underneath
        // handles SQLite-vs-BDB selection; an absent rpmdb returns
        // an empty map at zero cost.
        let rpm_file_lists = package_db::rpm::read_file_lists(root);

        // Milestone 133 US2.3 (FR-014): per-scan cache of manifest-file
        // SHA-256. Keyed by absolute `entry.source_path` (e.g. the
        // host-local tempdir path during image scans). Cached so a
        // single `Cargo.lock` shared by 500 cargo crates is hashed
        // once, not 500 times. `None` cached on read failure
        // (graceful skip per FR-014 second paragraph).
        let mut manifest_sha_cache: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();

        for entry in &db_entries {
            let purl_str = entry.purl.as_str().to_string();
            let ecosystem = entry.purl.ecosystem().to_string();
            // Only dpkg ships a per-package copyright file we can read.
            // apk packages have license info embedded in the install
            // db at varying quality (apk-license extraction is its own
            // follow-up). Detect the dpkg case via the source_path.
            let is_dpkg = entry.source_path.contains("dpkg/status");
            // Milestone 039: apk per-file deep-hashing — same shape
            // as the dpkg path. Detected via the standard apk
            // installed-db source_path.
            let is_apk = entry.source_path.contains("apk/db/installed");
            // Milestone 040 US3: rpm per-file deep-hashing.
            // Detector matches both legacy `var/lib/rpm/` and the
            // newer `usr/lib/sysimage/rpm/` paths via the common
            // `lib/rpm/` substring (verified to not collide with
            // any non-rpm source_path in mikebom).
            let is_rpm = entry.source_path.contains("lib/rpm/");
            // dpkg licenses live out-of-band in /usr/share/doc/<pkg>/
            // copyright; other sources (e.g. Python dist-info METADATA)
            // embed them directly on the entry.
            let licenses = if is_dpkg {
                package_db::copyright::read_copyright(root, &entry.name)
            } else if !entry.licenses.is_empty() {
                entry.licenses.clone()
            } else {
                Vec::new()
            };
            // Deep hashing reads every file the package owns
            // (`<pkg>.list`) and stream-hashes them with SHA-256, also
            // capturing the dpkg-recorded MD5 per file for cross-ref.
            // The fast path SHA-256s the dpkg `.md5sums` file content
            // as a per-package fingerprint with no per-file occurrences.
            //
            // Milestone 039: apk gets the parallel treatment via
            // hash_apk_package_files (deep) / hash_apk_db_only (fast).
            let (occurrences, mut component_hashes) = if is_dpkg {
                if deep_hash {
                    let (occs, root_hash) = package_db::file_hashes::hash_package_files(
                        root,
                        &entry.name,
                        entry.arch.as_deref(),
                    );
                    (occs, root_hash.into_iter().collect::<Vec<_>>())
            } else {
                    let h = package_db::file_hashes::hash_md5sums_only(
                        root,
                        &entry.name,
                        entry.arch.as_deref(),
                    );
                    (Vec::new(), h.into_iter().collect::<Vec<_>>())
                }
            } else if is_apk {
                if deep_hash {
                    let files: &[package_db::apk::ApkFileEntry] = apk_file_lists
                        .get(&entry.name)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let (occs, root_hash) =
                        package_db::file_hashes::hash_apk_package_files(root, files);
                    (occs, root_hash.into_iter().collect::<Vec<_>>())
                } else {
                    let h = package_db::file_hashes::hash_apk_db_only(root, &entry.name);
                    (Vec::new(), h.into_iter().collect::<Vec<_>>())
                }
            } else if is_rpm {
                if deep_hash {
                    let files: &[package_db::rpm::RpmFileListEntry] = rpm_file_lists
                        .get(&entry.name)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let (occs, root_hash) =
                        package_db::file_hashes::hash_rpm_package_files(root, files);
                    (occs, root_hash.into_iter().collect::<Vec<_>>())
                } else {
                    let h = package_db::file_hashes::hash_rpm_db_only(root, &entry.name);
                    (Vec::new(), h.into_iter().collect::<Vec<_>>())
                }
            } else {
                // Milestone 133 US2.3 (FR-014): language-ecosystem +
                // binary-tier readers populate a single FileOccurrence
                // at `entry.source_path` (the manifest file or binary
                // we read to discover this component). The location is
                // normalized to rootfs-relative + no-leading-`/` (the
                // same convention as `mikebom:source-files` per
                // FR-012). The SHA-256 anchors the occurrence to the
                // exact manifest bytes we parsed — useful for cross-
                // host SBOM diffing and supply-chain integrity.
                let occs =
                    manifest_occurrence(&entry.source_path, root, &mut manifest_sha_cache);
                (occs, Vec::new())
            };
            // Thread manifest-provided hashes (npm integrity, cargo
            // checksum) onto the component. `entry.hashes` is
            // populated by the npm / cargo readers; empty for other
            // ecosystems, in which case this is a no-op.
            component_hashes.extend(entry.hashes.iter().cloned());
            // OS-package ecosystems (deb/apk/rpm) read licenses
            // directly from the installed-package metadata or the
            // shipped copyright file — those sources ARE the
            // result of build-time analysis by the distro
            // maintainers. Emit them as both declared AND concluded
            // so sbomqs's `comp_with_licenses` / valid-licenses /
            // deprecated / restrictive checks (which key off
            // concluded) give full credit. Non-OS ecosystems are
            // filled by the CD enrichment pass and stay empty here.
            let os_ecosystem = matches!(entry.purl.ecosystem(), "deb" | "apk" | "rpm");
            let concluded_licenses_for_os = if os_ecosystem && !licenses.is_empty() {
                licenses.clone()
            } else {
                Vec::new()
            };
            components.push(ResolvedComponent {
                name: entry.name.clone(),
                version: entry.version.clone(),
                purl: entry.purl.clone(),
                evidence: ResolutionEvidence {
                    technique: ResolutionTechnique::PackageDatabase,
                    confidence: PACKAGE_DB_CONFIDENCE,
                    source_connection_ids: vec![],
                    // Milestone 133 US2.1 (FR-012 Defects A + C): see same-
                    // numbered comment above; normalize at source-population
                    // time so SPDX/CDX/SPDX3 all read the clean value.
                    //
                    // NOTE (milestone 145 US3): canonical source for
                    // `mikebom:source-files` emission — see also
                    // `root_selector::is_field_owned_annotation_key`.
                    source_file_paths: vec![crate::scan_fs::sbom_path::normalize_sbom_path_relative(
                        &entry.source_path,
                        Some(root),
                    )],
                    deps_dev_match: None,
                },
                licenses,
                concluded_licenses: concluded_licenses_for_os,
                hashes: component_hashes,
                supplier: entry
                    .maintainer
                    .clone()
                    .or_else(|| supplier_from_purl(&entry.purl)),
                cpes: vec![],
                advisories: vec![],
                occurrences,
                // Propagate dev/range/source/tier from PackageDbEntry.
                // dpkg/apk leave is_dev/requirement_range/source_type as
                // None (set in their constructors); sbom_tier = "deployed"
                // because both read installed-package databases.
                lifecycle_scope: entry.lifecycle_scope,
                build_inclusion: entry.build_inclusion,
                requirement_range: entry.requirement_range.clone(),
                source_type: entry.source_type.clone(),
                sbom_tier: entry.sbom_tier.clone(),
                buildinfo_status: entry.buildinfo_status.clone(),
                evidence_kind: entry.evidence_kind.clone(),
                binary_class: entry.binary_class.clone(),
                binary_stripped: entry.binary_stripped,
                linkage_kind: entry.linkage_kind.clone(),
                detected_go: entry.detected_go,
                confidence: entry.confidence.clone(),
                binary_packed: entry.binary_packed.clone(),
                npm_role: entry.npm_role.clone(),
                raw_version: entry.raw_version.clone(),
                parent_purl: entry.parent_purl.clone(),
                co_owned_by: entry.co_owned_by.clone(),
                shade_relocation: entry.shade_relocation,
                external_references: external_refs_from_purl(&entry.purl, &entry.extra_annotations),
                extra_annotations: entry.extra_annotations.clone(),
                // Milestone 104 — propagate role from PackageDbEntry
                // (set by the binary reader's `make_file_level_component`).
                binary_role: entry.binary_role,
            });

            // Emit a Relationship edge for each dependency that
            // resolved to another entry in this scan. Dangling targets
            // (Depends names we never saw) are silently dropped so the
            // CycloneDX `dependsOn[]` array only references bom-refs
            // that exist in this SBOM.
            for dep_name in &entry.depends {
                let key = (ecosystem.clone(), normalize_dep_name(&ecosystem, dep_name));
                if let Some(to) = name_to_purl.get(&key) {
                    if to != &purl_str {
                        // Skip self-loops (can happen via provides).
                        relationships.push(Relationship {
                            from: purl_str.clone(),
                            to: to.clone(),
                            relationship_type: RelationshipType::DependsOn,
                            provenance: EnrichmentProvenance {
                                source: entry.source_path.clone(),
                                data_type: "package-database-depends".to_string(),
                            },
                        });
                    }
                }
            }
        }
    }

    // Feature 008 US2 (G6): cache-ZIP-sourced Go components bypass
    // G3/G4/G5 because those filters live in `package_db::read_all`
    // and only touch `DbScanResult.entries`. The generic artifact
    // walker above (lines 126-190) emits every file at
    // `/go/pkg/mod/cache/download/<mod>/@v/<ver>.zip` as a
    // `pkg:golang/<mod>@<ver>` analyzed-tier component via
    // `path_resolver::resolve_go_path`. On polyglot-style images
    // where `go mod tidy` populated the cache with test-scope
    // transitives (testify / go-spew / go-difflib / yaml.v3), those
    // transitives leak to `components[]` even though they aren't
    // linked into the binary.
    //
    // When a Go binary's BuildInfo is ALSO on the same rootfs, it's
    // authoritative for "what ships" (same rationale as G3). Drop
    // cache-ZIP Go entries whose coord isn't confirmed by a non-cache
    // analyzed-tier entry. Pure-scratch scans (cache present, no
    // binary) retain all cache-ZIP entries — they're the only
    // available signal there.
    apply_go_cache_zip_filter(&mut components);

    // Milestone 052/part-2: rewrite DependsOn relationship edges to
    // the typed `RelationshipType::{DevDependsOn,BuildDependsOn,
    // TestDependsOn}` variants based on the target component's
    // `lifecycle_scope`. Each typed variant maps natively to SPDX 2.3
    // `DEV/BUILD/TEST_DEPENDENCY_OF` (via `spdx/relationships.rs`'s
    // existing mapper) and to SPDX 3 `lifecycleScope` parameter (via
    // `spdx/v3_relationships.rs`'s emission). Runs after all other
    // resolution + filtering steps so the target component's scope
    // is final by the time edges are typed.
    apply_lifecycle_scope_to_edges(&components, &mut relationships);

    let mut components = deduplicate(components);
    // Milestone 148: cross-PURL canonicalization. Some ecosystems (Maven
    // nested-coord case at scan_fs/package_db/maven.rs:3429-3457, Cargo
    // workspace vendoring, Go vendored modules) intentionally retain
    // multiple ResolvedComponent instances sharing the same Purl::as_str()
    // value but differing in parent_purl — the CDX nested-components
    // topology depends on this. Pre-148 each surviving entry carried its
    // own single-path source_file_paths Vec, and per-emitter iteration-
    // order differences produced cross-format divergence on the
    // mikebom:source-files annotation (51 polyglot-builder-image audit
    // findings, 2026-06-28). The canonicalize pass writes the alphabetically-
    // sorted union of all observed paths onto every same-PURL entry so
    // every emitter sees the same Vec content and emits the same
    // mikebom:source-files value regardless of which entry the harness
    // picks first. Idempotent; preserves all other fields including
    // parent_purl topology. See specs/148-source-files-union/ for details.
    canonicalize_source_files_by_purl(&mut components);
    // Post-dedup CPE synthesis — runs on the merged set so a component
    // that exists in both the filename pass and the dpkg pass gets one
    // set of CPEs (attached to the single winning entry) instead of two.
    for c in components.iter_mut() {
        c.cpes = synthesize_cpes(c);
    }

    // Milestone 127 FR-012 — Maven dedup. When a Maven `pom.xml` reader
    // emitted a main-module whose PURL matches the JAR walker's
    // `scan_target_coord`, suppress the `scan_target_coord` synthesis
    // before it reaches the metadata.component ladder. Keeps the FR-007
    // warning surface clean for pure-Java repos.
    let scan_target_coord =
        maybe_suppress_scan_target_coord(&components, scan_target_coord);

    // Milestone 127 FR-001 — tag every main-module-tagged component with
    // `mikebom:is-workspace-root: <bool>`. The signal feeds the
    // `generate/root_selector.rs` ladder (FR-002, FR-003) at SBOM
    // emission time. The key is internal-only — filtered out by
    // `is_internal_emission_key` at every per-format
    // `extra_annotations` iteration site so it never reaches
    // serialized SBOM output.
    tag_main_modules_with_workspace_root(&mut components, root);

    Ok(ScanResult {
        components,
        relationships,
        complete_ecosystems,
        os_release_missing_fields,
        go_graph_completeness,
        go_graph_completeness_reason,
        go_transitive_coverage,
        go_workspace_mode,
        scan_target_coord,
        divergence_records,
    })
}

/// Milestone 127 FR-001 implementation. Walks every component once;
/// for main-module-tagged ones, computes whether the component's
/// defining manifest file's parent directory canonicalizes to
/// `scan_root` and writes the result to `mikebom:is-workspace-root`.
///
/// Strict equality of canonicalized parent directories per research
/// R3. `canonicalize` failures on EITHER side degrade gracefully to
/// `false` (the affected main-module is just "not at the workspace
/// root" — ladder falls through to LCP or further). Matches the
/// permissive posture of milestone-114 `safe_walk`.
///
/// The annotation key is internal-only — `is_internal_emission_key`
/// strips it at every per-format `extra_annotations` iteration site
/// so it never appears in serialized SBOM output (preserves SC-003
/// byte-identity on the 33 alpha.48 goldens).
fn tag_main_modules_with_workspace_root(
    components: &mut [mikebom_common::resolution::ResolvedComponent],
    scan_root: &Path,
) {
    let canonical_root = std::fs::canonicalize(scan_root).ok();

    for c in components.iter_mut() {
        let is_main_module = c
            .extra_annotations
            .get("mikebom:component-role")
            .and_then(|v| v.as_str())
            == Some("main-module");
        if !is_main_module {
            continue;
        }

        // Manifest path lookup: ecosystems vary on where they record it.
        // Go reader populates `evidence.source_file_paths`. Workspace-
        // synthesizer + Swift / Kotlin / NuGet readers use the
        // `mikebom:source-files` annotation (as either a string OR an
        // array depending on the ecosystem). Read both, prefer evidence.
        let from_evidence: Option<String> = c
            .evidence
            .source_file_paths
            .first()
            .cloned();
        let from_annotation: Option<String> = c
            .extra_annotations
            .get("mikebom:source-files")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(arr) => arr
                    .first()
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                _ => None,
            });
        let manifest_path: Option<String> = from_evidence.or(from_annotation);
        let manifest_path = manifest_path.as_deref();

        let is_workspace_root = match (manifest_path, canonical_root.as_ref()) {
            (Some(p), Some(canon_root)) => {
                // Milestone 133 US2.1 (FR-012): post-normalization, manifest
                // paths in `evidence.source_file_paths` are rootfs-relative
                // (e.g. `sub/go.mod`) rather than absolute. Re-join against
                // `scan_root` so `canonicalize` resolves the right
                // filesystem location. Absolute paths (legacy / annotation-
                // sourced) join correctly because `Path::join` returns the
                // absolute when given one.
                let abs_path = scan_root.join(p);
                let manifest_parent = abs_path.parent();
                manifest_parent
                    .and_then(|parent| std::fs::canonicalize(parent).ok())
                    .map(|canon_manifest_parent| canon_manifest_parent == *canon_root)
                    .unwrap_or(false)
            }
            _ => false,
        };

        c.extra_annotations.insert(
            crate::generate::root_selector::IS_WORKSPACE_ROOT_KEY.to_string(),
            serde_json::Value::Bool(is_workspace_root),
        );
    }
}

/// Milestone 133 US2.2 (FR-013): stamp `mikebom:layer-digest` on every
/// component whose source path (first entry of `evidence.source_file_paths`)
/// appears in the OCI image's path → layer-digest map. The map is built
/// by `scan_fs::docker_image::extract` during layer extraction and
/// captures later-layer-wins overlay semantics.
///
/// Only emitted for image scans (`--image`). Non-image scans (`--path`)
/// pass `None` for `layer_path_map`, and the function is a no-op.
///
/// The annotation goes into `extra_annotations`; the existing CDX +
/// SPDX 2.3 + SPDX 3 emission paths iterate that bag and emit the
/// annotation under the same key in all three formats (validated by
/// the `holistic_parity` test under the C-row's `SymmetricEqual`
/// directionality).
pub fn tag_components_with_layer_digest(
    components: &mut [mikebom_common::resolution::ResolvedComponent],
    layer_path_map: Option<&std::collections::HashMap<String, String>>,
) {
    let Some(map) = layer_path_map else {
        return;
    };
    for c in components.iter_mut() {
        // First entry of source_file_paths is the canonical "where did
        // the reader identify this component from" path. After
        // milestone-133 PR US2.1 these paths are rootfs-relative with no
        // leading `/`, matching the keys in the layer map.
        let Some(source_path) = c.evidence.source_file_paths.first() else {
            continue;
        };
        if let Some(digest) = map.get(source_path.as_str()) {
            c.extra_annotations.insert(
                "mikebom:layer-digest".to_string(),
                serde_json::Value::String(digest.clone()),
            );
        }
    }
}

/// Milestone 133 US2.3 (FR-014): build a single `FileOccurrence` for a
/// language-ecosystem or binary-tier `PackageDbEntry`. The occurrence's
/// `location` is the manifest's rootfs-relative path (e.g.
/// `app/Cargo.lock`); the `sha256` is the streamed SHA-256 of the file
/// at that path. `sha256_cache` is keyed by absolute `source_path` so
/// shared manifests (one `Cargo.lock` → 500 cargo crates) are hashed
/// once.
///
/// Returns an empty `Vec` when:
/// - the source file can't be read (permission denied, missing — the
///   spec degrades gracefully here per FR-014),
/// - the file exceeds the 256 MB size cap (large binaries; we emit the
///   occurrence sans SHA in this case rather than blowing the scan).
///
/// The 256 MB cap matches `crate::trace::hasher::sha256_file_hex`'s
/// recommended cap. Cargo.lock / package.json / .gemspec / etc. are all
/// well under 1 MB; binaries on this codepath (cargo-auditable, Go
/// binaries) are typically 5-80 MB. The cap exists to keep an
/// accidentally-classified gigabyte log from stalling the scan.
fn manifest_occurrence(
    source_path: &str,
    root: &Path,
    sha256_cache: &mut std::collections::HashMap<String, Option<String>>,
) -> Vec<mikebom_common::resolution::FileOccurrence> {
    // Skip URL-scheme markers: the cargo / gem / maven / pip / npm
    // main-module path uses `path+file://<workspace>` as a marker
    // (`source_path` at e.g. `cargo.rs:387`) rather than a real
    // on-disk path. Treating it as an occurrence would leak a
    // placeholder string into `evidence.occurrences[].location` with
    // an empty `sha256`, and would also create a CDX/SPDX 2.3
    // asymmetry (CDX `metadata.component` isn't walked by the D2
    // extractor, SPDX's main-module Package is) which trips the
    // `holistic_parity::parity_*` SymmetricEqual assertion. The
    // workspace root is already surfaced via the existing
    // `mikebom:source-files` annotation; we don't need to duplicate it
    // here.
    if source_path.starts_with("path+file://")
        || source_path.starts_with("git+")
        || source_path.starts_with("url+")
        || source_path.starts_with("http://")
        || source_path.starts_with("https://")
    {
        return Vec::new();
    }

    // Compute (or look up cached) SHA-256 of the manifest file.
    let sha = if let Some(cached) = sha256_cache.get(source_path) {
        cached.clone()
    } else {
        const MAX_BYTES: u64 = 256 * 1024 * 1024;
        let computed =
            crate::trace::hasher::sha256_file_hex(std::path::Path::new(source_path), MAX_BYTES)
                .ok();
        sha256_cache.insert(source_path.to_string(), computed.clone());
        computed
    };

    // Either branch — SHA present or absent — emits the occurrence so
    // path coverage stays at ≥95% per SC-002. When the SHA can't be
    // computed (file removed, oversized, unreadable) we emit an empty
    // string; the CDX evidence emission preserves the field shape and
    // downstream consumers can still rely on `location`.
    let location =
        crate::scan_fs::sbom_path::normalize_sbom_path_relative(source_path, Some(root));
    if location.is_empty() {
        return Vec::new();
    }
    vec![mikebom_common::resolution::FileOccurrence {
        location,
        sha256: sha.unwrap_or_default(),
        md5_legacy: None,
        apk_sha1: None,
        rpm_file_digest: None,
    }]
}

/// Issue #363 — operator-asserted license-concluded promotion.
///
/// When the operator passes `--conclude-licenses`, mikebom promotes every
/// component's declared licenses to concluded IF AND ONLY IF the
/// concluded field is currently empty (preserves earlier ClearlyDefined /
/// deps.dev enrichment, which sets it to a verified value).
///
/// **Operator-assertion semantic**: the flag is a formal operator claim
/// that the declared licenses have been reviewed. Downstream consumers
/// reading `licenseConcluded` (sbomqs, Kusari Inspector, syft-style
/// comparators) treat the value as analyst-verified per SPDX 2.3 § 7.13
/// / SPDX 3.0.1 semantics. The per-component annotation
/// `mikebom:license-concluded-source = "operator-asserted"` records the
/// provenance so consumers can distinguish operator-asserted conclusions
/// from external-enrichment-derived ones.
///
/// Returns the count of components whose concluded set was updated. Used
/// by `scan_cmd::scan` for the post-pass info log.
pub fn apply_operator_asserted_conclusions(
    components: &mut [mikebom_common::resolution::ResolvedComponent],
) -> usize {
    let mut promoted = 0usize;
    for c in components.iter_mut() {
        if c.concluded_licenses.is_empty() && !c.licenses.is_empty() {
            c.concluded_licenses = c.licenses.clone();
            c.extra_annotations.insert(
                "mikebom:license-concluded-source".to_string(),
                serde_json::Value::String("operator-asserted".to_string()),
            );
            promoted += 1;
        }
    }
    promoted
}

/// Milestone 127 FR-012 implementation. When a Maven main-module
/// component exists whose PURL matches the JAR walker's
/// `scan_target_coord`, return `None` (suppress the duplicate
/// signal). Otherwise return the input untouched.
fn maybe_suppress_scan_target_coord(
    components: &[mikebom_common::resolution::ResolvedComponent],
    scan_target_coord: Option<package_db::maven::ScanTargetCoord>,
) -> Option<package_db::maven::ScanTargetCoord> {
    let coord = scan_target_coord.as_ref()?;
    let expected_purl =
        format!("pkg:maven/{}/{}@{}", coord.group, coord.artifact, coord.version);

    let dup = components.iter().any(|c| {
        let role = c
            .extra_annotations
            .get("mikebom:component-role")
            .and_then(|v| v.as_str());
        role == Some("main-module")
            && c.purl.ecosystem() == "maven"
            && c.purl.as_str() == expected_purl
    });

    if dup {
        tracing::debug!(
            coord = %expected_purl,
            "Milestone 127 FR-012: suppressed JAR-walker scan_target_coord (covered by Maven main-module)"
        );
        None
    } else {
        scan_target_coord
    }
}

/// Normalise a dependency name to the canonical form used as the
/// `name_to_purl` index key. Each ecosystem has its own rules; keeping
/// both the index side and the lookup side on this function guarantees
/// they stay in sync.
///
/// - **pypi** — case-insensitive, `_` ≡ `-` per PEP 503. Both
///   `Name: Requests` and `Requires-Dist: requests` must hit the same
///   bucket. Mirrors `pip::normalize_pypi_name_for_purl`.
/// - **npm** — lowercase (registry is case-insensitive). Scoped
///   `@scope/name` kept intact; only the case is normalised.
/// - **deb / apk / everything else** — lowercase. Debian and Alpine
///   treat names case-insensitively in practice; the installed db
///   stores them lowercase anyway but we stay tolerant.
///
/// Feature 008 US2 (G6): drop `pkg:golang` analyzed-tier components
/// whose source files are exclusively under `/go/pkg/mod/cache/download/`
/// when a non-cache analyzed-tier Go entry (from a binary's BuildInfo)
/// is also present on the rootfs. Cache-ZIP entries reflect what
/// `go mod tidy` downloaded — a superset of linked modules — and leak
/// test-scope transitives. BuildInfo reflects what the linker actually
/// embedded.
///
/// When no non-cache analyzed-tier Go entry exists at all (pure scratch
/// scan: cache present, no binary), the filter no-ops and cache-ZIP
/// entries remain the authoritative signal. This matches the design
/// comment at `path_resolver::resolve_go_path` line 284-294.
///
/// A component's source files are considered "from cache" when EVERY
/// path listed in `evidence.source_file_paths` contains
/// `/cache/download/`. A mixed entry (one cache source + one BuildInfo
/// source, merged by upstream dedup) is NOT from cache and passes
/// through.
fn apply_go_cache_zip_filter(components: &mut Vec<mikebom_common::resolution::ResolvedComponent>) {
    use std::collections::HashSet;
    let buildinfo_linked: HashSet<(String, String)> = components
        .iter()
        .filter(|c| {
            c.purl.ecosystem() == "golang"
                && c.sbom_tier.as_deref() == Some("analyzed")
                && !c
                    .evidence
                    .source_file_paths
                    .iter()
                    .all(|p| p.contains("/cache/download/"))
        })
        .map(|c| (c.name.clone(), c.version.clone()))
        .collect();
    if buildinfo_linked.is_empty() {
        return;
    }
    let before = components.len();
    components.retain(|c| {
        if c.purl.ecosystem() != "golang" {
            return true;
        }
        let from_cache_only = !c.evidence.source_file_paths.is_empty()
            && c.evidence
                .source_file_paths
                .iter()
                .all(|p| p.contains("/cache/download/"));
        if !from_cache_only {
            return true;
        }
        buildinfo_linked.contains(&(c.name.clone(), c.version.clone()))
    });
    let dropped = before.saturating_sub(components.len());
    if dropped > 0 {
        tracing::info!(
            dropped,
            buildinfo_linked_count = buildinfo_linked.len(),
            "G6 filter: dropped cache-ZIP Go components not confirmed by BuildInfo",
        );
    }
}

/// Milestone 052/part-2: rewrite generic `DependsOn` relationship
/// edges to typed `DevDependsOn` / `BuildDependsOn` / `TestDependsOn`
/// variants based on the target component's `lifecycle_scope`. The
/// SPDX 2.3 serializer (`spdx/relationships.rs`) maps each typed
/// variant to its native `DEV/BUILD/TEST_DEPENDENCY_OF` SPDX
/// relationship type; the SPDX 3 serializer
/// (`spdx/v3_relationships.rs`) emits the `lifecycleScope` parameter
/// on `dependsOn` relationships using the same source signal.
///
/// Runs after all component-resolution and filtering steps so each
/// target's `lifecycle_scope` is final by the time we type the edges.
/// `Runtime` and `None` leave edges as `DependsOn`.
fn apply_lifecycle_scope_to_edges(
    components: &[mikebom_common::resolution::ResolvedComponent],
    relationships: &mut [mikebom_common::resolution::Relationship],
) {
    use mikebom_common::resolution::{LifecycleScope, RelationshipType};
    let scope_by_purl: std::collections::HashMap<&str, LifecycleScope> = components
        .iter()
        .filter_map(|c| c.lifecycle_scope.map(|s| (c.purl.as_str(), s)))
        .collect();
    let mut rewrites = 0usize;
    for rel in relationships.iter_mut() {
        // Only rewrite edges that haven't already been typed by a
        // reader (the existing readers all emit `DependsOn`; this is
        // an invariant defense against future reader changes).
        if !matches!(rel.relationship_type, RelationshipType::DependsOn) {
            continue;
        }
        let Some(scope) = scope_by_purl.get(rel.to.as_str()) else {
            continue;
        };
        rel.relationship_type = match scope {
            LifecycleScope::Runtime => continue,
            LifecycleScope::Development => RelationshipType::DevDependsOn,
            LifecycleScope::Build => RelationshipType::BuildDependsOn,
            LifecycleScope::Test => RelationshipType::TestDependsOn,
        };
        rewrites += 1;
    }
    if rewrites > 0 {
        tracing::info!(
            rewrites,
            "rewrote DependsOn → typed (Dev|Build|Test)DependsOn edges based on target lifecycle_scope",
        );
    }
}

fn normalize_dep_name(ecosystem: &str, name: &str) -> String {
    match ecosystem {
        "pypi" => name.replace('_', "-").to_lowercase(),
        _ => name.to_lowercase(),
    }
}

/// Derive a best-effort supplier string from a PURL when the
/// component's source didn't carry explicit maintainer metadata.
/// Drives CycloneDX `component.supplier.name` (sbomqs
/// `comp_with_supplier`) + SPDX 2.3 `Package.originator` + SPDX 3
/// `software:supplier`.
///
/// Precedence (highest first):
/// 1. Reader-populated `entry.maintainer` (FR-006) — applied at the
///    callsite via `entry.maintainer.clone().or_else(supplier_from_purl)`.
/// 2. Namespace-derived heuristic for ecosystems whose PURL namespace
///    carries informative supplier signal:
///    - `pkg:golang/<host>/<org>/<repo>` → `<host>/<org>`.
///    - `pkg:maven/<group>/<artifact>` → `<group>`.
///    - `pkg:npm/@<scope>/<name>` → `@<scope>`.
/// 3. **Milestone 132 US1 (FR-005)**: canonical PURL-ecosystem →
///    registry-name table fallback. Fills the Supplier Attribution gap
///    for ecosystems whose PURLs don't carry a discriminating namespace
///    (cargo, nuget, pypi, gem, apk, deb, rpm) and for unscoped npm.
///    Sets the emitted `supplier.name` field via the existing
///    `entry.maintainer.or_else` chain at `scan_fs/mod.rs:572`.
fn supplier_from_purl(purl: &mikebom_common::types::purl::Purl) -> Option<String> {
    let ecosystem = purl.ecosystem();

    // Existing namespace-derived heuristics (milestone 001) — preserved
    // because they emit MORE-specific supplier strings than the canonical
    // table can. Apply only when the PURL carries a namespace segment.
    if let Some(namespace) = purl.namespace() {
        match ecosystem {
            "golang" => {
                let segments: Vec<&str> = namespace.split('/').collect();
                if segments.len() >= 2 {
                    return Some(format!("{}/{}", segments[0], segments[1]));
                }
                return Some(namespace.to_string());
            }
            "maven" => return Some(namespace.to_string()),
            "npm" if namespace.starts_with('@') => {
                return Some(namespace.to_string());
            }
            _ => {}
        }
    }

    // Milestone 132 US1 (FR-005): canonical registry-name lookup table.
    const SUPPLIER_TABLE: &[(&str, &str)] = &[
        ("cargo", "crates.io"),
        ("nuget", "nuget.org"),
        ("maven", "Maven Central"),
        ("npm", "npmjs.com"),
        ("pypi", "PyPI"),
        ("gem", "RubyGems"),
        ("apk", "Alpine Package Maintainer"),
        ("deb", "Debian Package Maintainer"),
        ("rpm", "RPM Package Maintainer"),
    ];

    SUPPLIER_TABLE
        .iter()
        .find(|(eco, _)| *eco == ecosystem)
        .map(|(_, supplier)| (*supplier).to_string())
}

/// Derive VCS / website external references from a PURL when the
/// module path embeds them. Currently limited to Go modules whose
/// canonical form starts with a known repo host — that's where the
/// signal is unambiguous.
///
/// Milestone 131 US3: extended to synthesize canonical registry
/// `website` URLs for `pkg:cargo`, `pkg:nuget`, and nested
/// `pkg:maven` components (FR-017..019), plus a `vcs` URL parsed
/// from cargo-auditable's `source` field when carried via the C98
/// `mikebom:cargo-vcs-source-url` plumbing annotation (FR-020).
fn external_refs_from_purl(
    purl: &mikebom_common::types::purl::Purl,
    extra_annotations: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Vec<mikebom_common::resolution::ExternalReference> {
    use mikebom_common::resolution::ExternalReference;
    let mut out = Vec::new();
    let ecosystem = purl.ecosystem();
    let name = purl.name();
    let version = purl.version().unwrap_or_default();
    match ecosystem {
        "golang" => {
            // Existing milestone-001 heuristic.
            if let Some(namespace) = purl.namespace() {
                let segments: Vec<&str> = namespace.split('/').collect();
                if segments.len() >= 2 {
                    let host = segments[0];
                    if matches!(host, "github.com" | "gitlab.com" | "bitbucket.org") {
                        let org = segments[1];
                        let repo = name;
                        out.push(ExternalReference {
                            ref_type: "vcs".to_string(),
                            url: format!("https://{host}/{org}/{repo}"),
                        });
                    }
                }
            }
        }
        "cargo" if !name.is_empty() && !version.is_empty() => {
            // FR-017: crates.io registry website.
            out.push(ExternalReference {
                ref_type: "website".to_string(),
                url: format!("https://crates.io/crates/{name}/{version}"),
            });
        }
        "nuget" if !name.is_empty() && !version.is_empty() => {
            // FR-018: nuget.org registry website.
            out.push(ExternalReference {
                ref_type: "website".to_string(),
                url: format!("https://www.nuget.org/packages/{name}/{version}"),
            });
        }
        "maven" => {
            // FR-019: search.maven.org canonical per-artifact URL.
            // Gated on `mikebom:source-mechanism = "maven-jar-nested"` so the
            // existing top-level milestone-009 reader's sidecar-derived URLs
            // aren't clobbered.
            let is_nested = extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str())
                == Some("maven-jar-nested");
            if is_nested {
                if let Some(namespace) = purl.namespace() {
                    if !namespace.is_empty() && !name.is_empty() && !version.is_empty() {
                        out.push(ExternalReference {
                            ref_type: "website".to_string(),
                            url: format!(
                                "https://search.maven.org/artifact/{namespace}/{name}/{version}/jar"
                            ),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    // FR-020: emit a `vcs`-type ExternalReference when cargo-auditable
    // carried a parseable `git+https://...` source field. The URL
    // itself flows through the C98 `mikebom:cargo-vcs-source-url`
    // plumbing annotation, populated upstream at
    // `binary/entry.rs::cargo_auditable_packages_to_entries`.
    if let Some(vcs_url) = extra_annotations
        .get("mikebom:cargo-vcs-source-url")
        .and_then(|v| v.as_str())
    {
        out.push(ExternalReference {
            ref_type: "vcs".to_string(),
            url: vcs_url.to_string(),
        });
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod supplier_tests {
    use super::supplier_from_purl;
    use mikebom_common::types::purl::Purl;

    #[test]
    fn golang_host_and_org() {
        let p = Purl::new("pkg:golang/github.com/sirupsen/logrus@v1.9.3").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("github.com/sirupsen".to_string()));
    }

    #[test]
    fn maven_group_id() {
        let p = Purl::new("pkg:maven/com.fasterxml.jackson.core/jackson-core@2.17.2").unwrap();
        assert_eq!(
            supplier_from_purl(&p),
            Some("com.fasterxml.jackson.core".to_string()),
        );
    }

    #[test]
    fn npm_scoped_has_supplier() {
        let p = Purl::new("pkg:npm/%40types/node@20.1.0").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("@types".to_string()));
    }

    // Milestone 132 US1: FR-005 SUPPLIER_TABLE fallback flips these from
    // None to canonical registry names. Tests updated in-place rather than
    // duplicated so the assertions remain the source of truth for the
    // function's current behavior.

    #[test]
    fn npm_unscoped_resolves_via_table() {
        let p = Purl::new("pkg:npm/express@4.22.1").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("npmjs.com".to_string()));
    }

    #[test]
    fn cargo_resolves_via_table() {
        let p = Purl::new("pkg:cargo/serde@1.0.197").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("crates.io".to_string()));
    }

    #[test]
    fn nuget_resolves_via_table() {
        let p = Purl::new("pkg:nuget/Newtonsoft.Json@13.0.3").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("nuget.org".to_string()));
    }

    #[test]
    fn pypi_resolves_via_table() {
        let p = Purl::new("pkg:pypi/requests@2.31.0").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("PyPI".to_string()));
    }

    #[test]
    fn gem_resolves_via_table() {
        let p = Purl::new("pkg:gem/rails@7.1.2").unwrap();
        assert_eq!(supplier_from_purl(&p), Some("RubyGems".to_string()));
    }

    #[test]
    fn apk_resolves_via_table() {
        let p = Purl::new("pkg:apk/alpine/ca-certificates@20240226-r0").unwrap();
        // apk has a namespace (the distro) but it is not a golang/maven/npm
        // ecosystem; falls through to the SUPPLIER_TABLE.
        assert_eq!(
            supplier_from_purl(&p),
            Some("Alpine Package Maintainer".to_string()),
        );
    }

    #[test]
    fn deb_resolves_via_table() {
        let p = Purl::new("pkg:deb/debian/curl@7.74.0-1.3+deb11u14").unwrap();
        assert_eq!(
            supplier_from_purl(&p),
            Some("Debian Package Maintainer".to_string()),
        );
    }

    #[test]
    fn rpm_resolves_via_table() {
        let p = Purl::new("pkg:rpm/redhat/openssl@1.1.1k-12.el8_9").unwrap();
        assert_eq!(
            supplier_from_purl(&p),
            Some("RPM Package Maintainer".to_string()),
        );
    }

    #[test]
    fn bitbake_not_in_table_returns_none() {
        // FR-001 edge case: ecosystems not in the FR-005 table return None;
        // existing readers populate entry.maintainer instead.
        let p = Purl::new("pkg:bitbake/openssl@1.1.1w-r0").unwrap();
        assert_eq!(supplier_from_purl(&p), None);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod external_refs_tests {
    use super::external_refs_from_purl;
    use mikebom_common::types::purl::Purl;
    use std::collections::BTreeMap;

    fn no_annotations() -> BTreeMap<String, serde_json::Value> {
        BTreeMap::new()
    }

    #[test]
    fn cargo_purl_emits_crates_io_website() {
        let p = Purl::new("pkg:cargo/serde@1.0.197").unwrap();
        let refs = external_refs_from_purl(&p, &no_annotations());
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_type, "website");
        assert_eq!(refs[0].url, "https://crates.io/crates/serde/1.0.197");
    }

    #[test]
    fn nuget_purl_emits_nuget_org_website() {
        let p = Purl::new("pkg:nuget/Microsoft.AspNetCore@8.0.27").unwrap();
        let refs = external_refs_from_purl(&p, &no_annotations());
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_type, "website");
        assert_eq!(refs[0].url, "https://www.nuget.org/packages/Microsoft.AspNetCore/8.0.27");
    }

    #[test]
    fn maven_nested_emits_search_maven_website_when_gated() {
        let p = Purl::new("pkg:maven/com.example/foo@1.0.0").unwrap();
        let mut annotations = BTreeMap::new();
        annotations.insert(
            "mikebom:source-mechanism".to_string(),
            serde_json::Value::String("maven-jar-nested".to_string()),
        );
        let refs = external_refs_from_purl(&p, &annotations);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_type, "website");
        assert_eq!(
            refs[0].url,
            "https://search.maven.org/artifact/com.example/foo/1.0.0/jar"
        );
    }

    #[test]
    fn maven_top_level_does_not_emit_website() {
        // Top-level maven JARs (no source-mechanism annotation) get
        // their URLs from sidecar paths elsewhere; we don't synthesize
        // here to avoid clobbering.
        let p = Purl::new("pkg:maven/com.example/foo@1.0.0").unwrap();
        let refs = external_refs_from_purl(&p, &no_annotations());
        assert!(refs.is_empty());
    }

    #[test]
    fn cargo_with_vcs_annotation_emits_both_website_and_vcs() {
        let p = Purl::new("pkg:cargo/serde@1.0.197").unwrap();
        let mut annotations = BTreeMap::new();
        annotations.insert(
            "mikebom:cargo-vcs-source-url".to_string(),
            serde_json::Value::String("https://github.com/serde-rs/serde".to_string()),
        );
        let refs = external_refs_from_purl(&p, &annotations);
        assert_eq!(refs.len(), 2);
        // Order: website first (ecosystem branch), then vcs (cross-cutting).
        assert_eq!(refs[0].ref_type, "website");
        assert_eq!(refs[1].ref_type, "vcs");
        assert_eq!(refs[1].url, "https://github.com/serde-rs/serde");
    }

    #[test]
    fn golang_heuristic_preserved() {
        let p = Purl::new("pkg:golang/github.com/sirupsen/logrus@v1.9.3").unwrap();
        let refs = external_refs_from_purl(&p, &no_annotations());
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_type, "vcs");
        assert_eq!(refs[0].url, "https://github.com/sirupsen/logrus");
    }
}

/// Fail-closed errors from `scan_path`. Only raised when a downstream
/// reader reports a hard failure that must abort the scan rather than
/// degrade silently (e.g. npm v1 lockfile refusal per FR-006). Wraps
/// `PackageDbError` so the CLI can print the specific stderr message
/// documented in `contracts/cli-interface.md`.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("{0}")]
    PackageDb(#[from] package_db::PackageDbError),
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn scan_picks_up_cargo_crate_filenames() {
        // A path_resolver::resolve_cargo_path-compatible path resolves to
        // the right PURL even when the surrounding dir is synthetic.
        let dir = tempfile::tempdir().expect("tempdir");
        let cache_dir = dir.path().join(".cargo/registry/cache/idx");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("serde-1.0.197.crate"), b"bytes").unwrap();

        let result = scan_path(dir.path(), None, 1024, false, true, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();
        assert_eq!(result.components.len(), 1);
        assert!(result.relationships.is_empty());
        let c = &result.components[0];
        assert_eq!(c.name, "serde");
        assert_eq!(c.version, "1.0.197");
        assert_eq!(c.evidence.technique, ResolutionTechnique::FilePathPattern);
        assert!((c.evidence.confidence - 0.70).abs() < 1e-9);
        assert_eq!(c.hashes.len(), 1, "scan-sourced component must carry its file hash");
        assert_eq!(c.evidence.source_file_paths.len(), 1);
    }

    #[test]
    fn scan_picks_up_deb_filenames_with_codename_hint() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("jq_1.6-2.1+deb12u1_arm64.deb"),
            b"deb bytes",
        )
        .unwrap();

        let result = scan_path(dir.path(), Some("bookworm"), 1024, false, true, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();
        assert_eq!(result.components.len(), 1);
        let purl = result.components[0].purl.as_str();
        assert!(
            purl.contains("distro=bookworm"),
            "codename hint should land as qualifier: {purl}"
        );
    }

    #[test]
    fn scan_ignores_non_artifact_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("README.md"), b"not a package").unwrap();
        std::fs::write(dir.path().join("build.log"), b"also not").unwrap();

        let result = scan_path(dir.path(), None, 1024, false, true, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();
        assert!(result.components.is_empty());
    }

    #[test]
    fn package_db_entries_appear_with_high_confidence() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Fake rootfs with a dpkg status file.
        let dpkg = dir.path().join("var/lib/dpkg/status");
        std::fs::create_dir_all(dpkg.parent().unwrap()).unwrap();
        std::fs::write(
            &dpkg,
            "\
Package: jq
Status: install ok installed
Version: 1.6-2.1+deb12u1
Architecture: arm64
Depends: libc6, libjq1

Package: libjq1
Status: install ok installed
Version: 1.6-2.1+deb12u1
Architecture: arm64
",
        )
        .unwrap();

        let result = scan_path(dir.path(), Some("bookworm"), 1024, true, true, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();
        // Both packages resolve from the db.
        assert_eq!(result.components.len(), 2, "{:#?}", result.components);
        assert!(result
            .components
            .iter()
            .all(|c| c.evidence.technique == ResolutionTechnique::PackageDatabase));
        assert!(result
            .components
            .iter()
            .all(|c| (c.evidence.confidence - 0.85).abs() < 1e-9));
    }

    #[test]
    fn package_db_relationships_reference_observed_components_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dpkg = dir.path().join("var/lib/dpkg/status");
        std::fs::create_dir_all(dpkg.parent().unwrap()).unwrap();
        // jq depends on libjq1 (installed) AND libc6 (NOT listed as
        // installed in this tiny db). Expect exactly one edge: jq→libjq1.
        std::fs::write(
            &dpkg,
            "\
Package: jq
Status: install ok installed
Version: 1.6
Architecture: arm64
Depends: libc6, libjq1

Package: libjq1
Status: install ok installed
Version: 1.6
Architecture: arm64
",
        )
        .unwrap();

        let result = scan_path(dir.path(), None, 1024, true, true, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();
        assert_eq!(result.relationships.len(), 1);
        let rel = &result.relationships[0];
        assert!(rel.from.contains("jq@1.6"));
        assert!(rel.to.contains("libjq1@1.6"));
        assert_eq!(rel.relationship_type, RelationshipType::DependsOn);
    }

    #[test]
    fn filename_resolved_and_dpkg_resolved_dedupe_into_one_component() {
        // Real-world case: the .deb artefact sits in apt's cache AND
        // the package is also recorded in dpkg's status file. Both
        // code paths fire and must merge into a single component.
        let dir = tempfile::tempdir().expect("tempdir");

        // Filename side: drop the .deb where the apt cache normally lives.
        let cache = dir.path().join("var/cache/apt/archives");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("jq_1.6-2.1+deb12u1_arm64.deb"),
            b"fake deb body",
        )
        .unwrap();

        // dpkg side: status file listing jq + libjq1 with a dependency edge.
        let dpkg = dir.path().join("var/lib/dpkg/status");
        std::fs::create_dir_all(dpkg.parent().unwrap()).unwrap();
        std::fs::write(
            &dpkg,
            "\
Package: jq
Status: install ok installed
Version: 1.6-2.1+deb12u1
Architecture: arm64
Depends: libjq1

Package: libjq1
Status: install ok installed
Version: 1.6-2.1+deb12u1
Architecture: arm64
",
        )
        .unwrap();

        // Deep hash off so we don't depend on .list/.md5sums fixtures.
        let result = scan_path(dir.path(), Some("bookworm"), 1024, true, false, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();

        // Exactly two components: jq (merged) + libjq1. NOT three.
        assert_eq!(
            result.components.len(),
            2,
            "filename+dpkg duplicates should merge: {:#?}",
            result.components
        );

        let jq = result
            .components
            .iter()
            .find(|c| c.name == "jq")
            .expect("jq component present");
        let libjq1 = result
            .components
            .iter()
            .find(|c| c.name == "libjq1")
            .expect("libjq1 component present");

        // PackageDatabase technique (0.85) beats FilePathPattern (0.70) —
        // the deduplicator should keep the dpkg entry's identity fields.
        assert_eq!(jq.evidence.technique, ResolutionTechnique::PackageDatabase);
        assert!((jq.evidence.confidence - 0.85).abs() < 1e-9);

        // Hashes from the filename side must be preserved through the merge.
        assert!(
            !jq.hashes.is_empty(),
            "merged jq should retain the .deb file's SHA-256"
        );

        // Source file paths from both sides merged.
        let paths = &jq.evidence.source_file_paths;
        assert!(paths.iter().any(|p| p.ends_with(".deb")), "{paths:?}");
        assert!(
            paths.iter().any(|p| p.ends_with("dpkg/status")),
            "{paths:?}"
        );

        // Dependency edge must still reference libjq1 after the merge.
        let libjq1_purl = libjq1.purl.as_str();
        assert!(
            result
                .relationships
                .iter()
                .any(|r| r.from == jq.purl.as_str() && r.to == libjq1_purl),
            "jq -> libjq1 edge survives dedup: {:#?}",
            result.relationships
        );
    }

    #[test]
    fn filename_with_percent_encoded_plus_merges_with_dpkg_plain_plus() {
        // Regression: apt names the cache file with `%2B` but dpkg
        // stores the version with a literal `+`. If the path_resolver
        // doesn't decode %2B back to +, the two PURL keys diverge and
        // dedup produces two components instead of one. This is the
        // exact shape we observed on a real debian:bookworm-slim-style
        // scan (libjq1, fd-find, jq, libonig5, ripgrep — all with +bN
        // binNMU suffixes).
        let dir = tempfile::tempdir().expect("tempdir");

        let cache = dir.path().join("var/cache/apt/archives");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("libjq1_1.6-2.1%2Bb1_arm64.deb"),
            b"fake deb body",
        )
        .unwrap();

        let dpkg = dir.path().join("var/lib/dpkg/status");
        std::fs::create_dir_all(dpkg.parent().unwrap()).unwrap();
        std::fs::write(
            &dpkg,
            "\
Package: libjq1
Status: install ok installed
Version: 1.6-2.1+b1
Architecture: arm64
Maintainer: Some Maintainer <m@example.org>
",
        )
        .unwrap();

        let result = scan_path(dir.path(), Some("bookworm"), 1024, true, false, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();

        // One merged component, not two.
        assert_eq!(
            result.components.len(),
            1,
            "%2B filename + plain `+` dpkg must dedup: {:#?}",
            result.components
        );

        let c = &result.components[0];
        assert_eq!(c.name, "libjq1");
        // Human-readable version keeps literal `+` (used in CycloneDX
        // `component.version` and CPE).
        assert_eq!(c.version, "1.6-2.1+b1");
        // dpkg won on confidence.
        assert_eq!(c.evidence.technique, ResolutionTechnique::PackageDatabase);
        // Filename-side SHA-256 survived the merge.
        assert!(!c.hashes.is_empty(), "merged component retains .deb hash");
        // dpkg-side Maintainer propagated.
        assert_eq!(
            c.supplier.as_deref(),
            Some("Some Maintainer <m@example.org>")
        );
        // Canonical PURL encodes `+` as `%2B` per the packageurl-python
        // reference impl. Exactly once, and no stray literal `+` left
        // over from either side of the merge.
        let purl = c.purl.as_str();
        assert!(
            purl.contains("1.6-2.1%2Bb1"),
            "canonical form must carry %2B: {purl}"
        );
        assert!(
            !purl.contains("1.6-2.1+"),
            "no literal + should leak into canonical form: {purl}"
        );
    }

    #[test]
    fn no_package_db_flag_skips_db_read_even_if_db_is_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dpkg = dir.path().join("var/lib/dpkg/status");
        std::fs::create_dir_all(dpkg.parent().unwrap()).unwrap();
        std::fs::write(
            &dpkg,
            "\
Package: foo
Status: install ok installed
Version: 1.0
Architecture: amd64
",
        )
        .unwrap();

        let result = scan_path(dir.path(), None, 1024, /*read_package_db=*/ false, true, false, false, ScanMode::Path, true, None, None, None, &Default::default()).unwrap();
        assert!(
            result.components.is_empty(),
            "db should be ignored when flag is off"
        );
    }

    // Milestone 087 (issue #172): the cargo dual-key insert in
    // `scan_path` builds a `(cargo, "<name> <version>")` lookup key
    // by composing the entry's `name` + `version` fields and passing
    // through `normalize_dep_name`. The same composition runs at
    // edge-emit time on the cargo-emitted dep string. For
    // disambiguation to work, the two paths MUST produce identical
    // keys — i.e. `normalize_dep_name("cargo", "clap_builder 4.5.21")`
    // must be idempotent under repeated application and must not
    // mangle the embedded space.
    #[test]
    fn normalize_dep_name_cargo_preserves_name_version_form() {
        let key = normalize_dep_name("cargo", "clap_builder 4.5.21");
        assert_eq!(key, "clap_builder 4.5.21");
        // Idempotent: applying again is a no-op.
        assert_eq!(normalize_dep_name("cargo", &key), key);
    }

    // ============================================================
    // Milestone 133 US2.2 — tag_components_with_layer_digest tests.
    // ============================================================

    fn make_component(source_path: &str) -> mikebom_common::resolution::ResolvedComponent {
        use mikebom_common::resolution::{
            ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        let purl = mikebom_common::types::purl::Purl::new("pkg:cargo/test@1.0.0").unwrap();
        ResolvedComponent {
            build_inclusion: None,
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![source_path.to_string()],
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
        }
    }

    #[test]
    fn tag_layer_digest_stamps_when_source_path_matches() {
        let mut map = std::collections::HashMap::new();
        map.insert(
            "lib/apk/db/installed".to_string(),
            "sha256:abc123".to_string(),
        );
        let mut comps = vec![make_component("lib/apk/db/installed")];
        tag_components_with_layer_digest(&mut comps, Some(&map));
        assert_eq!(
            comps[0]
                .extra_annotations
                .get("mikebom:layer-digest")
                .and_then(|v| v.as_str()),
            Some("sha256:abc123"),
        );
    }

    #[test]
    fn tag_layer_digest_skips_when_path_not_in_map() {
        let map = std::collections::HashMap::new();
        let mut comps = vec![make_component("unattributed/path")];
        tag_components_with_layer_digest(&mut comps, Some(&map));
        assert!(!comps[0].extra_annotations.contains_key("mikebom:layer-digest"));
    }

    #[test]
    fn tag_layer_digest_no_op_when_map_is_none() {
        // Non-image scans pass `None` — function should be a no-op.
        let mut comps = vec![make_component("any/path")];
        tag_components_with_layer_digest(&mut comps, None);
        assert!(!comps[0].extra_annotations.contains_key("mikebom:layer-digest"));
    }

    #[test]
    fn tag_layer_digest_skips_components_with_no_source_path() {
        let mut map = std::collections::HashMap::new();
        map.insert("foo".to_string(), "sha256:abc".to_string());
        let mut comp = make_component("foo");
        comp.evidence.source_file_paths.clear();
        let mut comps = vec![comp];
        tag_components_with_layer_digest(&mut comps, Some(&map));
        assert!(!comps[0].extra_annotations.contains_key("mikebom:layer-digest"));
    }
}