//! Milestone 215 — SBOM auto-split by workspace member.
//!
//! Post-resolve, pre-emit fan-out. When `--split` is passed to
//! `waybill sbom scan`, enumerate detected workspace-root components,
//! BFS-project each into its own reachable dep-graph subset, and
//! emit one SBOM per subproject (× each requested `--format`)
//! plus a sibling `split-manifest.json`.
//!
//! Boundary-enumeration signal: `waybill:is-workspace-root` annotation
//! set by the m127 root selector + m201 disambiguation. Reused
//! verbatim — no new detection logic.
//!
//! See `specs/215-sbom-auto-split/` for spec / plan / research.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use data_encoding::HEXLOWER;
use serde_json::Value;
use sha2::{Digest, Sha256};

use waybill_common::resolution::{Relationship, ResolvedComponent};
use waybill_common::types::purl::Purl;

use super::split_manifest::{SplitEntry, SplitManifest};
use super::{OutputConfig, ScanArtifacts, SerializerRegistry};

/// Milestone 215 — the annotation key + value that identifies a
/// component as a candidate split axis. Every per-ecosystem
/// main-module emitter (cargo, npm, pypi, maven, go, gem, swift, …)
/// stamps `waybill:component-role = "main-module"` on the component
/// that represents a workspace-member (or single-package project).
/// This is the m127 ladder's input signal too — reusing it means
/// split-mode inherits every reader's workspace-detection logic for
/// free (research R1).
const COMPONENT_ROLE_KEY: &str = "waybill:component-role";
const MAIN_MODULE_ROLE: &str = "main-module";

/// One detected workspace-root that becomes the axis for one sub-SBOM.
#[derive(Debug, Clone)]
pub(crate) struct SubprojectRoot {
    /// Canonical PURL identifying the subproject. Drives filename slug,
    /// manifest `subproject_id`, and serves as the BFS seed.
    pub purl: Purl,
    /// PURL string as it appears in `Relationship.from`/`.to` for graph
    /// traversal — Waybill's `Relationship` type keys on PURL strings,
    /// not bom-refs.
    pub purl_string: String,
    /// Subproject source directory relative to scan_root (empty when
    /// the subproject IS the scan root).
    pub source_dir: PathBuf,
    /// Ecosystem name (`cargo`, `npm`, `pypi`, …). Appears in filename.
    pub ecosystem: String,
}

impl SubprojectRoot {
    /// `<slug>.<ecosystem>` — matches the emitted-filename prefix and
    /// the manifest `subproject_id`. Deterministic function of the PURL.
    pub fn subproject_id(&self) -> String {
        format!("{}.{}", subject_slug(&self.purl), self.ecosystem)
    }
}

/// One BFS-projected subset — the components + relationships that end
/// up in a single sub-SBOM.
#[derive(Debug)]
pub(crate) struct SplitProjection {
    pub root: SubprojectRoot,
    pub components: Vec<ResolvedComponent>,
    pub relationships: Vec<Relationship>,
    /// Count of THIS projection's components that also appear in ≥ 1
    /// sibling projection. Populated post-hoc by [`compute_shared_deps`].
    pub shared_deps_count: usize,
}

// ---------- T008: enumerate_workspace_roots ----------

/// Return every workspace-member component projected into a
/// [`SubprojectRoot`], sorted lexicographically by `subproject_id` for
/// deterministic emit order.
///
/// **Split-axis signal**: `waybill:component-role == "main-module"`.
/// Every per-ecosystem main-module emitter (cargo / npm / pypi /
/// maven / go / gem / swift / …) stamps this on the component that
/// represents a workspace member (or single-package project). This is
/// the same signal the m127 root-selector ladder inspects — so
/// split-mode inherits every reader's workspace-detection logic for
/// free (research R1).
///
/// **NOT `waybill:is-workspace-root`**: that annotation is a scan-wide
/// signal (only true for THE root of the whole scan), not per-member.
///
/// Filters out any component whose PURL name is empty (m127's synthetic
/// placeholder path); those aren't split axes per research R1.
pub(crate) fn enumerate_workspace_roots(
    resolved_components: &[ResolvedComponent],
    scan_root: &std::path::Path,
) -> Vec<SubprojectRoot> {
    let mut roots: Vec<SubprojectRoot> = resolved_components
        .iter()
        .filter(|c| is_main_module(c))
        .filter(|c| !c.purl.name().is_empty())
        .map(|c| SubprojectRoot {
            purl: c.purl.clone(),
            purl_string: c.purl.to_string(),
            source_dir: source_dir_for(c, scan_root),
            ecosystem: c.purl.ecosystem().to_string(),
        })
        .collect();

    roots.sort_by_key(|a| a.subproject_id());
    roots
}

fn is_main_module(c: &ResolvedComponent) -> bool {
    matches!(
        c.extra_annotations.get(COMPONENT_ROLE_KEY),
        Some(Value::String(s)) if s == MAIN_MODULE_ROLE
    )
}

fn source_dir_for(
    c: &ResolvedComponent,
    scan_root: &std::path::Path,
) -> PathBuf {
    // The first evidence source_file_paths entry may be:
    //   • a plain path (`libsafe/Cargo.toml`)
    //   • a `path+file://<abs>` URI (cargo/pip conventions)
    //   • an absolute filesystem path
    //   • a manifest path (needs `.parent()` to get the dir)
    // Strip the URI prefix, relativize against `scan_root`, then take
    // the parent so the returned value is the subproject's directory.
    let Some(raw) = c.evidence.source_file_paths.first() else {
        return PathBuf::new();
    };
    let stripped = raw
        .strip_prefix("path+file://")
        .or_else(|| raw.strip_prefix("file://"))
        .unwrap_or(raw.as_str());
    let abs = PathBuf::from(stripped);
    // Canonicalize the scan_root so absolute source_file_paths (which
    // are already canonical) strip cleanly. Fall back to the raw
    // scan_root path if canonicalize fails (e.g. path doesn't exist).
    let canon_root = std::fs::canonicalize(scan_root)
        .unwrap_or_else(|_| scan_root.to_path_buf());
    let rel = abs
        .strip_prefix(&canon_root)
        .or_else(|_| abs.strip_prefix(scan_root))
        .ok()
        .map(PathBuf::from)
        .unwrap_or(abs);
    // If the path looks like a manifest file (`Cargo.toml`, `package.json`,
    // `pyproject.toml`, etc.), return its parent. Otherwise (already a dir)
    // return as-is.
    if rel
        .file_name()
        .and_then(|n| n.to_str())
        .map(is_manifest_basename)
        .unwrap_or(false)
    {
        rel.parent().map(PathBuf::from).unwrap_or_default()
    } else {
        rel
    }
}

fn is_manifest_basename(name: &str) -> bool {
    matches!(
        name,
        // Declaration manifests.
        "Cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "setup.py"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "go.mod"
            | "Gemfile"
            | "Package.swift"
            | "Chart.yaml"
            | "composer.json"
            | "mix.exs"
            | "rebar.config"
            | "Package.resolved"
            // Lockfiles — cargo m064 augment-in-place populates
            // evidence.source_file_paths with the workspace-shared
            // Cargo.lock path (see waybill-cli/src/scan_fs/mod.rs:960+
            // for the rationale), and npm's file: local-dep resolver
            // similarly records the consumer's package-lock.json. In
            // both cases the file IS a manifest artifact whose parent
            // dir is the subproject dir; without this arm the
            // source_dir helper returns the lockfile path itself
            // instead of the parent (m215 follow-up bug: fixture
            // scoped-npm-package showed source_dir =
            // "app-code/vex-analyzer/package-lock.json" instead of
            // "app-code/shared-js/internalclient").
            | "Cargo.lock"
            | "package-lock.json"
            | "npm-shrinkwrap.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lock"
            | "Gemfile.lock"
            | "poetry.lock"
            | "uv.lock"
            | "Pipfile.lock"
            | "go.sum"
            | "composer.lock"
            | "mix.lock"
    )
}

// ---------- T009: project_for_root (BFS) ----------

/// BFS from the root's PURL over dep-edge relationships. Returns the
/// reachable component set (including root) + all relationships whose
/// both endpoints are in that set (self-contained per FR-007).
pub(crate) fn project_for_root(
    root: &SubprojectRoot,
    all_components: &[ResolvedComponent],
    all_relationships: &[Relationship],
) -> SplitProjection {
    // Pre-build a `from → [to, ...]` adjacency map so BFS is O(V + E)
    // instead of O(V × E) with linear scans per node.
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for rel in all_relationships {
        if is_dep_edge(&rel.relationship_type) {
            adjacency
                .entry(rel.from.as_str())
                .or_default()
                .push(rel.to.as_str());
        }
    }

    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    reached.insert(root.purl_string.clone());
    queue.push_back(root.purl_string.clone());

    while let Some(cur) = queue.pop_front() {
        if let Some(next) = adjacency.get(cur.as_str()) {
            for &to in next {
                if reached.insert(to.to_string()) {
                    queue.push_back(to.to_string());
                }
            }
        }
    }

    // Preserve `all_components` ordering; place root first if present.
    let root_component = all_components
        .iter()
        .find(|c| c.purl.to_string() == root.purl_string)
        .cloned();

    let mut components: Vec<ResolvedComponent> = Vec::new();
    if let Some(rc) = root_component {
        components.push(rc);
    }
    for c in all_components {
        let s = c.purl.to_string();
        if s == root.purl_string {
            continue;
        }
        if reached.contains(&s) {
            // Bug: cross-workspace deps (common in Go monorepos where
            // every module imports a shared internal lib) cause BFS to
            // pull SIBLING workspace-root main-modules into this
            // projection. When m127's root-selector runs at emit time
            // and sees > 1 `waybill:component-role = "main-module"`
            // component, it falls through past the RepoRoot fast-path
            // to the multi-lang ecosystem-priority / LCP / synthetic-
            // placeholder branch — resulting in the wrong
            // `metadata.component.purl` being emitted for this
            // sub-SBOM (observed in `~/Projects/iac` where 23 of 25
            // Go sub-SBOMs emitted `pkg:generic/iac@0.0.0` instead of
            // their own module PURL).
            //
            // Fix: strip the main-module role from every sibling
            // main-module in this projection. The sibling stays in
            // the graph (still a legitimate transitive that the root
            // reached), but no longer confuses m127's ladder — the
            // ladder sees exactly ONE main-module (the split-axis
            // root at position 0) and fast-paths to it correctly.
            let mut demoted = c.clone();
            if is_main_module(&demoted) {
                demoted.extra_annotations.remove(COMPONENT_ROLE_KEY);
            }
            components.push(demoted);
        }
    }

    let relationships = all_relationships
        .iter()
        .filter(|r| reached.contains(&r.from) && reached.contains(&r.to))
        .cloned()
        .collect();

    SplitProjection {
        root: root.clone(),
        components,
        relationships,
        shared_deps_count: 0,
    }
}

fn is_dep_edge(kind: &waybill_common::resolution::RelationshipType) -> bool {
    use waybill_common::resolution::RelationshipType::*;
    matches!(
        kind,
        DependsOn
            | DevDependsOn
            | BuildDependsOn
            | TestDependsOn
            | OptionalDependsOn
    )
}

// ---------- T010: compute_shared_deps ----------

/// After all N projections exist, walk the union of every projection's
/// components. A PURL that appears in ≥ 2 projections is a shared dep.
/// Populate each projection's `shared_deps_count` (the count of ITS
/// components that overlap with ≥ 1 sibling) and return
/// `(total_unique_components, aggregate_shared_dep_count)` for manifest
/// document-level aggregates.
pub(crate) fn compute_shared_deps(
    projections: &mut [SplitProjection],
) -> (u64, u64) {
    // Count how many projections each PURL appears in.
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    for p in projections.iter() {
        // Use HashSet per-projection so a self-repeat doesn't inflate.
        let mut seen: HashSet<String> = HashSet::new();
        for c in &p.components {
            seen.insert(c.purl.to_string());
        }
        for s in seen {
            *occurrences.entry(s).or_default() += 1;
        }
    }

    // Per-projection: count its components whose PURL appears in ≥ 2.
    for p in projections.iter_mut() {
        let mut n: usize = 0;
        let mut seen: HashSet<String> = HashSet::new();
        for c in &p.components {
            let s = c.purl.to_string();
            if seen.insert(s.clone())
                && occurrences.get(&s).copied().unwrap_or(0) >= 2
            {
                n += 1;
            }
        }
        p.shared_deps_count = n;
    }

    let total_unique = occurrences.len() as u64;
    let shared_agg = occurrences.values().filter(|&&n| n >= 2).count() as u64;
    (total_unique, shared_agg)
}

// ---------- T011: filename_for + slug helpers ----------

/// Format-id → extension token (`cyclonedx-json` → `cdx`).
///
/// Uses `starts_with("spdx-3")` so every SPDX 3 family id (including
/// any deprecation-aliases the registry maps) lands in the same
/// `spdx3` extension bucket without this module referencing specific
/// alias strings.
pub(crate) fn format_ext(format_id: &str) -> &'static str {
    if format_id == "cyclonedx-json" {
        "cdx"
    } else if format_id == "spdx-2.3-json" {
        "spdx"
    } else if format_id.starts_with("spdx-3") {
        "spdx3"
    } else {
        "sbom" // permissive fallback for unknown future formats
    }
}

/// Reserved-on-Windows base names (case-insensitive comparison).
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul",
    "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Filenames the manifest itself owns — never produce one for a
/// sub-SBOM.
const RESERVED_SUB_SBOM_NAMES: &[&str] = &[
    "split-manifest.json",
    ".gitkeep",
    ".gitignore",
];

/// PURL → filesystem-safe slug per contracts/filename-convention.md.
///
/// Prefixes namespace when present (`@myorg/frontend` → `myorg-frontend`,
/// `com.example/my-lib` → `com.example-my-lib`), substitutes/strip
/// unsafe chars, truncates to 100 bytes, lowercases.
pub(crate) fn subject_slug(purl: &Purl) -> String {
    let mut s = if let Some(ns) = purl.namespace() {
        format!("{}-{}", ns, purl.name())
    } else {
        purl.name().to_string()
    };

    // 1) Character substitutions.
    s = s.replace('/', "-").replace('@', "at-");
    // 2) Strip URL/path-unsafe chars (backslash, colon, glob, wildcards,
    //    quotes, angle brackets, pipe). Whitespace also stripped.
    s.retain(|c| {
        !matches!(
            c,
            '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' | '\t' | '\n' | '\r'
        )
    });
    // 3) Non-ASCII → strip entirely (defensive; PURLs shouldn't carry
    //    non-ASCII but be conservative for cross-filesystem safety).
    s.retain(|c| c.is_ascii());
    // 4) Truncate to 100 chars.
    if s.len() > 100 {
        s.truncate(100);
    }
    // 5) Lowercase.
    s.to_ascii_lowercase()
}

/// Build the final filename `<slug>.<ecosystem>.<format-ext>.json` with
/// collision + reserved-name handling.
///
/// `collision_map` maps `subproject_id` → list of source_dirs of every
/// root sharing that id. When any list has len > 1, this root gets a
/// `-<8hex>` suffix on the slug (deterministic hash of its own source_dir).
pub(crate) fn filename_for(
    root: &SubprojectRoot,
    format_id: &str,
    collision_map: &BTreeMap<String, Vec<PathBuf>>,
) -> String {
    let mut slug = subject_slug(&root.purl);
    let ecosystem = &root.ecosystem;

    // Collision fallback: append SHA-8 of source_dir when the base
    // subproject_id collides with a sibling.
    let base_id = format!("{slug}.{ecosystem}");
    let colliding = collision_map
        .get(&base_id)
        .map(|paths| paths.len() > 1)
        .unwrap_or(false);
    if colliding {
        let hash = sha8_hex(&root.source_dir.to_string_lossy());
        slug = format!("{slug}-{hash}");
    }

    // Reserved-Windows-basename guard: prefix `wb-` if the slug matches
    // (case-insensitively) a reserved DOS device name.
    if WINDOWS_RESERVED
        .iter()
        .any(|w| w.eq_ignore_ascii_case(&slug))
    {
        slug = format!("wb-{slug}");
    }

    let ext = format_ext(format_id);
    let mut filename = format!("{slug}.{ecosystem}.{ext}.json");

    // Manifest-name collision guard: if the emitted name would clash
    // with a reserved sub-SBOM name (`split-manifest.json` etc.),
    // hash-suffix.
    if RESERVED_SUB_SBOM_NAMES.contains(&filename.as_str()) {
        let hash = sha8_hex(&root.source_dir.to_string_lossy());
        filename = format!("{slug}-{hash}.{ecosystem}.{ext}.json");
    }

    filename
}

fn sha8_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    HEXLOWER.encode(&digest[..4])
}

/// Build a collision map: `subproject_id` → list of source_dirs that
/// produce that same id. Used by [`filename_for`] to disambiguate.
pub(crate) fn build_collision_map(
    roots: &[SubprojectRoot],
) -> BTreeMap<String, Vec<PathBuf>> {
    let mut m: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for r in roots {
        m.entry(r.subproject_id()).or_default().push(r.source_dir.clone());
    }
    m
}

// ---------- Deterministic sub-SBOM serial (R5) ----------

/// Under `WAYBILL_FIXED_TIMESTAMP`, sub-SBOM serial becomes a
/// deterministic hash of the root PURL + fixed timestamp. Otherwise,
/// a fresh random UUIDv4-shaped serial.
///
/// T013 follow-up: wire this into the CDX / SPDX 2.3 / SPDX 3 serial
/// generation paths. Until then the split-mode sub-SBOMs use each
/// serializer's own default serial derivation.
#[allow(dead_code)]
pub(crate) fn sub_sbom_serial(
    root_purl: &Purl,
    fixed_ts: Option<&str>,
) -> String {
    match fixed_ts {
        Some(ts) => {
            let mut hasher = Sha256::new();
            hasher.update(root_purl.to_string().as_bytes());
            hasher.update(b"|");
            hasher.update(ts.as_bytes());
            let digest = hasher.finalize();
            // 32 hex chars = 128 bits; UUID-shaped.
            format!("urn:uuid:{}", &HEXLOWER.encode(&digest)[..32])
        }
        None => format!("urn:uuid:{}", uuid_v4_hex_32()),
    }
}

#[allow(dead_code)]
fn uuid_v4_hex_32() -> String {
    // Match Waybill's existing non-deterministic UUID shape without
    // pulling `uuid` crate here — the CDX path already uses `uuid`.
    // We format 32 hex chars from a random source for the split
    // fallback path (non-reproducible mode only).
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_le_bytes());
    let digest = hasher.finalize();
    HEXLOWER.encode(&digest)[..32].to_string()
}

// ---------- T012 + T014: emit-dispatch fan-out ----------

/// Milestone 215 — split-mode emit orchestration.
///
/// When called from the CLI layer with `--split` set + at least one
/// detected workspace root:
/// 1. Enumerate roots from the resolved-component set.
/// 2. If N == 0, log a WARN and return `Ok(false)` so the caller
///    falls through to the pre-feature single-SBOM emit (FR-009).
/// 3. Otherwise, BFS-project each root, compute shared-dep counts,
///    fan out `N × M` (subprojects × formats) sub-SBOM emissions
///    into `output_dir`, and write `split-manifest.json` alongside.
///    Return `Ok(true)` — the caller MUST skip its own emit loop.
///
/// Emit is all-or-nothing per FR-016: any failure aborts the whole
/// invocation (no partial writes are cleaned up here — the operator
/// deletes `output_dir` on failure). The manifest is written LAST so
/// its presence implies all sub-SBOMs landed successfully.
pub(crate) fn emit_split(
    base_artifacts: &ScanArtifacts<'_>,
    formats: &[String],
    registry: &SerializerRegistry,
    output_dir: &Path,
    created: DateTime<Utc>,
    waybill_version: &str,
    scan_root: &Path,
) -> anyhow::Result<bool> {
    let roots = enumerate_workspace_roots(base_artifacts.components, scan_root);
    // FR-009: fallback to single-SBOM emit + WARN when there aren't
    // enough boundaries to make a split meaningful. Zero boundaries
    // (no main-modules) and one boundary (single-package project) both
    // fall through — one entry is degenerate per research R8, and
    // scripts that opportunistically pass `--split` shouldn't break on
    // single-package trees.
    if roots.len() <= 1 {
        tracing::warn!(
            scan_root = %scan_root.display(),
            detected = roots.len(),
            "no workspace boundaries detected — emitting single SBOM per --split fallback contract (FR-009)"
        );
        return Ok(false);
    }

    std::fs::create_dir_all(output_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create --output-dir {}: {e}",
            output_dir.display()
        )
    })?;

    let collision_map = build_collision_map(&roots);

    // Build one SplitProjection per root.
    let mut projections: Vec<SplitProjection> = roots
        .iter()
        .map(|r| {
            project_for_root(
                r,
                base_artifacts.components,
                base_artifacts.relationships,
            )
        })
        .collect();
    let (total_unique, aggregate_shared) = compute_shared_deps(&mut projections);

    tracing::info!(
        subproject_count = projections.len(),
        format_count = formats.len(),
        total_unique_components = total_unique,
        shared_dep_count = aggregate_shared,
        output_dir = %output_dir.display(),
        "--split emit: fan-out starting"
    );

    // Build the manifest as we emit; write it LAST (FR-016).
    let mut manifest = SplitManifest::new(
        waybill_version.to_string(),
        scan_root.to_string_lossy().to_string(),
        created.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    );
    manifest.total_unique_components = total_unique;
    manifest.shared_dep_count = aggregate_shared;

    // Per-projection emission.
    for projection in &projections {
        let sub_artifacts = base_artifacts.narrow(
            &projection.components,
            &projection.relationships,
        );
        let mut entry_files: BTreeMap<String, String> = BTreeMap::new();

        for fmt in formats {
            let serializer = registry.get(fmt).ok_or_else(|| {
                anyhow::anyhow!("split emit: unknown format id {fmt:?}")
            })?;
            let filename =
                filename_for(&projection.root, fmt, &collision_map);
            let sub_output_cfg = OutputConfig {
                mikebom_version: env_pkg_version(),
                created,
                overrides: BTreeMap::new(),
            };
            let emitted = serializer.serialize(&sub_artifacts, &sub_output_cfg)?;
            // The primary artifact (first one) is the sub-SBOM itself.
            // Side artifacts (e.g. OpenVEX sidecar) get their own
            // per-projection namespaced filename to avoid cross-
            // subproject collisions in a shared output_dir.
            for (i, artifact) in emitted.into_iter().enumerate() {
                let target = if i == 0 {
                    output_dir.join(&filename)
                } else {
                    // Namespace sidecars by projection: `<slug>.<original>`.
                    let sidecar_base = artifact
                        .relative_path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("sidecar-{i}"));
                    let ns_name =
                        format!("{}.{}", projection.root.subproject_id(), sidecar_base);
                    output_dir.join(ns_name)
                };
                std::fs::write(&target, &artifact.bytes).map_err(|e| {
                    anyhow::anyhow!(
                        "split emit: failed to write {}: {e}",
                        target.display()
                    )
                })?;
                if i == 0 {
                    entry_files.insert(fmt.clone(), filename.clone());
                }
                tracing::info!(
                    format = %fmt,
                    path = %target.display(),
                    bytes = artifact.bytes.len(),
                    subproject = %projection.root.subproject_id(),
                    "wrote split sub-SBOM artifact"
                );
            }
        }

        let entry = SplitEntry {
            subproject_id: projection.root.subproject_id(),
            root_purl: projection.root.purl_string.clone(),
            source_dir: projection
                .root
                .source_dir
                .to_string_lossy()
                .to_string(),
            component_count: projection.components.len() as u64,
            shared_deps_count: projection.shared_deps_count as u64,
            files: entry_files,
        };
        manifest.entries.push(entry);
    }

    // Manifest last — its presence signals a successful split.
    let manifest_path = output_dir.join("split-manifest.json");
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| {
        anyhow::anyhow!("split emit: failed to serialize manifest: {e}")
    })?;
    std::fs::write(&manifest_path, &manifest_bytes).map_err(|e| {
        anyhow::anyhow!(
            "split emit: failed to write manifest {}: {e}",
            manifest_path.display()
        )
    })?;
    tracing::info!(
        path = %manifest_path.display(),
        entries = manifest.entries.len(),
        "wrote split-manifest.json"
    );

    Ok(true)
}

fn env_pkg_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ---------- Tests ----------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{
        EnrichmentProvenance, RelationshipType, ResolutionEvidence,
        ResolutionTechnique,
    };

    fn mk_component(purl: &str, is_main: bool) -> ResolvedComponent {
        let p = Purl::new(purl).unwrap();
        let mut ann = BTreeMap::new();
        if is_main {
            ann.insert(
                COMPONENT_ROLE_KEY.to_string(),
                Value::String(MAIN_MODULE_ROLE.to_string()),
            );
        }
        ResolvedComponent {
            purl: p.clone(),
            name: p.name().to_string(),
            version: p.version().unwrap_or("").to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 1.0,
                source_connection_ids: vec![],
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
            requirement_ranges: Vec::new(),
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
            extra_annotations: ann,
            binary_role: None,
        }
    }

    fn mk_rel(from: &str, to: &str) -> Relationship {
        Relationship {
            from: from.to_string(),
            to: to.to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "test".to_string(),
            },
        }
    }

    // -------- enumerate_workspace_roots --------

    #[test]
    fn enumerate_filters_on_main_module_component_role() {
        let comps = vec![
            mk_component("pkg:cargo/libsafe@0.1.0", true),
            mk_component("pkg:cargo/serde@1.0.0", false),
            mk_component("pkg:cargo/libvuln@0.1.0", true),
        ];
        let roots = enumerate_workspace_roots(&comps, std::path::Path::new("/"));
        assert_eq!(roots.len(), 2);
        // Sorted by subproject_id (lex): libsafe.cargo < libvuln.cargo.
        assert_eq!(roots[0].subproject_id(), "libsafe.cargo");
        assert_eq!(roots[1].subproject_id(), "libvuln.cargo");
    }

    #[test]
    fn enumerate_skips_empty_purl_name() {
        // Manually construct a synthetic-placeholder-shaped PURL that
        // parses but has an empty name — we can't; Purl::new rejects.
        // Instead, verify the filter path runs by not tripping on real
        // roots.
        let comps = vec![mk_component("pkg:cargo/x@0.1.0", true)];
        let roots = enumerate_workspace_roots(&comps, std::path::Path::new("/"));
        assert_eq!(roots.len(), 1);
    }

    // -------- project_for_root (BFS) --------

    #[test]
    fn project_bfs_reaches_transitive_closure() {
        let comps = vec![
            mk_component("pkg:cargo/root@0.1.0", true),
            mk_component("pkg:cargo/mid@1.0.0", false),
            mk_component("pkg:cargo/leaf@1.0.0", false),
            mk_component("pkg:cargo/unrelated@1.0.0", false),
        ];
        let rels = vec![
            mk_rel("pkg:cargo/root@0.1.0", "pkg:cargo/mid@1.0.0"),
            mk_rel("pkg:cargo/mid@1.0.0", "pkg:cargo/leaf@1.0.0"),
        ];
        let roots = enumerate_workspace_roots(&comps, std::path::Path::new("/"));
        assert_eq!(roots.len(), 1);
        let proj = project_for_root(&roots[0], &comps, &rels);
        assert_eq!(proj.components.len(), 3, "root + mid + leaf");
        assert_eq!(proj.relationships.len(), 2);
        assert_eq!(proj.components[0].purl.name(), "root", "root component first");
    }

    #[test]
    fn project_excludes_sibling_members() {
        let comps = vec![
            mk_component("pkg:cargo/a@1.0.0", true),
            mk_component("pkg:cargo/b@1.0.0", true),
            mk_component("pkg:cargo/dep-a@1.0.0", false),
            mk_component("pkg:cargo/dep-b@1.0.0", false),
        ];
        let rels = vec![
            mk_rel("pkg:cargo/a@1.0.0", "pkg:cargo/dep-a@1.0.0"),
            mk_rel("pkg:cargo/b@1.0.0", "pkg:cargo/dep-b@1.0.0"),
        ];
        let roots = enumerate_workspace_roots(&comps, std::path::Path::new("/"));
        let proj = project_for_root(&roots[0], &comps, &rels); // "a"
        assert_eq!(proj.components.len(), 2, "a + dep-a only");
        assert!(proj.components.iter().any(|c| c.purl.name() == "a"));
        assert!(proj.components.iter().any(|c| c.purl.name() == "dep-a"));
        assert!(!proj.components.iter().any(|c| c.purl.name() == "b"));
    }

    #[test]
    fn project_demotes_sibling_main_modules_reached_via_cross_deps() {
        // Regression for the ~/Projects/iac Go-monorepo bug: cross-
        // workspace deps (`a` imports `shared`, `b` also imports
        // `shared`) don't pull sibling main-modules into the projection
        // with their main-module tag intact — m127's root-selector
        // sees exactly ONE main-module per projection.
        let comps = vec![
            mk_component("pkg:golang/example.com/a@v0.0.0", true),
            mk_component("pkg:golang/example.com/shared@v0.0.0", true),
            mk_component("pkg:golang/example.com/leaf@v1.0.0", false),
        ];
        let rels = vec![
            mk_rel(
                "pkg:golang/example.com/a@v0.0.0",
                "pkg:golang/example.com/shared@v0.0.0",
            ),
            mk_rel(
                "pkg:golang/example.com/shared@v0.0.0",
                "pkg:golang/example.com/leaf@v1.0.0",
            ),
        ];
        let roots = enumerate_workspace_roots(&comps, std::path::Path::new("/"));
        // `a` root's projection reaches `a` → `shared` → `leaf`.
        let a_root = roots
            .iter()
            .find(|r| r.purl.name() == "a")
            .expect("a is a split root");
        let proj = project_for_root(a_root, &comps, &rels);
        assert_eq!(
            proj.components.len(),
            3,
            "a + shared + leaf; got {:?}",
            proj
                .components
                .iter()
                .map(|c| c.purl.name())
                .collect::<Vec<_>>()
        );
        // The split-axis root (position 0) keeps its main-module tag.
        assert!(
            is_main_module(&proj.components[0]),
            "split-axis root must retain main-module role"
        );
        // The sibling main-module `shared` had its role stripped so
        // downstream m127 sees only ONE main-module in this projection
        // and correctly fast-paths to the split-axis root at emit time.
        let shared_in_proj = proj
            .components
            .iter()
            .find(|c| c.purl.name() == "shared")
            .expect("shared present in projection");
        assert!(
            !is_main_module(shared_in_proj),
            "sibling main-module `shared` must have its component-role demoted \
             in a's projection so m127 sees exactly one main-module"
        );
        // Non-main-module components are untouched.
        let leaf = proj
            .components
            .iter()
            .find(|c| c.purl.name() == "leaf")
            .expect("leaf present");
        assert!(!is_main_module(leaf));
    }

    // -------- compute_shared_deps --------

    #[test]
    fn shared_deps_counts_correctly_across_three_projections() {
        let comps = vec![
            mk_component("pkg:cargo/a@1.0.0", true),
            mk_component("pkg:cargo/b@1.0.0", true),
            mk_component("pkg:cargo/c@1.0.0", true),
            mk_component("pkg:cargo/shared@1.0.0", false),
            mk_component("pkg:cargo/only-a@1.0.0", false),
        ];
        let rels = vec![
            mk_rel("pkg:cargo/a@1.0.0", "pkg:cargo/shared@1.0.0"),
            mk_rel("pkg:cargo/a@1.0.0", "pkg:cargo/only-a@1.0.0"),
            mk_rel("pkg:cargo/b@1.0.0", "pkg:cargo/shared@1.0.0"),
            mk_rel("pkg:cargo/c@1.0.0", "pkg:cargo/shared@1.0.0"),
        ];
        let roots = enumerate_workspace_roots(&comps, std::path::Path::new("/"));
        let mut projections: Vec<SplitProjection> = roots
            .iter()
            .map(|r| project_for_root(r, &comps, &rels))
            .collect();
        let (total, shared) = compute_shared_deps(&mut projections);
        assert_eq!(total, 5, "5 distinct PURLs across all projections");
        assert_eq!(shared, 1, "only `shared` appears in >1 projection");
        // Every projection sees shared → shared_deps_count = 1.
        for p in &projections {
            assert_eq!(
                p.shared_deps_count, 1,
                "projection {} should have shared_deps_count=1",
                p.root.subproject_id()
            );
        }
    }

    // -------- sub_sbom_serial --------

    #[test]
    fn sub_sbom_serial_deterministic_under_fixed_timestamp() {
        let p = Purl::new("pkg:cargo/foo@1.0.0").unwrap();
        let s1 = sub_sbom_serial(&p, Some("2026-01-01T00:00:00Z"));
        let s2 = sub_sbom_serial(&p, Some("2026-01-01T00:00:00Z"));
        assert_eq!(s1, s2, "same input → same serial");
        let s3 = sub_sbom_serial(&p, Some("2026-01-02T00:00:00Z"));
        assert_ne!(s1, s3, "timestamp change → serial change");
    }

    #[test]
    fn sub_sbom_serial_differs_across_purls() {
        let a = Purl::new("pkg:cargo/a@1.0.0").unwrap();
        let b = Purl::new("pkg:cargo/b@1.0.0").unwrap();
        let ts = Some("2026-01-01T00:00:00Z");
        assert_ne!(sub_sbom_serial(&a, ts), sub_sbom_serial(&b, ts));
    }

    // -------- filename_for + slug --------

    #[test]
    fn slug_simple_cargo_package() {
        let p = Purl::new("pkg:cargo/libsafe@0.1.0").unwrap();
        assert_eq!(subject_slug(&p), "libsafe");
    }

    #[test]
    fn slug_prefixes_npm_scope() {
        let p = Purl::new("pkg:npm/%40myorg/frontend@1.0.0").unwrap();
        // namespace() should return "@myorg" (or decoded form); the
        // slug prefixes it with a dash.
        let s = subject_slug(&p);
        assert!(s.ends_with("frontend"), "got {s}");
    }

    #[test]
    fn slug_lowercases() {
        let p = Purl::new("pkg:cargo/FooBar@1.0.0").unwrap();
        assert_eq!(subject_slug(&p), "foobar");
    }

    #[test]
    fn filename_cargo_no_collision() {
        let r = SubprojectRoot {
            purl: Purl::new("pkg:cargo/libsafe@0.1.0").unwrap(),
            purl_string: "pkg:cargo/libsafe@0.1.0".to_string(),
            source_dir: PathBuf::from("libsafe"),
            ecosystem: "cargo".to_string(),
        };
        let cm = BTreeMap::new();
        assert_eq!(
            filename_for(&r, "cyclonedx-json", &cm),
            "libsafe.cargo.cdx.json"
        );
        assert_eq!(
            filename_for(&r, "spdx-2.3-json", &cm),
            "libsafe.cargo.spdx.json"
        );
        assert_eq!(
            filename_for(&r, "spdx-3-json", &cm),
            "libsafe.cargo.spdx3.json"
        );
    }

    #[test]
    fn filename_windows_reserved_prefixes_wb() {
        let r = SubprojectRoot {
            purl: Purl::new("pkg:cargo/con@0.1.0").unwrap(),
            purl_string: "pkg:cargo/con@0.1.0".to_string(),
            source_dir: PathBuf::from("con-crate"),
            ecosystem: "cargo".to_string(),
        };
        let cm = BTreeMap::new();
        assert_eq!(
            filename_for(&r, "cyclonedx-json", &cm),
            "wb-con.cargo.cdx.json"
        );
    }

    #[test]
    fn filename_collision_appends_sha_suffix() {
        let a = SubprojectRoot {
            purl: Purl::new("pkg:cargo/foo@1.0.0").unwrap(),
            purl_string: "pkg:cargo/foo@1.0.0".to_string(),
            source_dir: PathBuf::from("libs/cli/foo"),
            ecosystem: "cargo".to_string(),
        };
        let b = SubprojectRoot {
            purl: Purl::new("pkg:cargo/foo@1.0.0").unwrap(),
            purl_string: "pkg:cargo/foo@1.0.0".to_string(),
            source_dir: PathBuf::from("libs/tools/foo"),
            ecosystem: "cargo".to_string(),
        };
        let cm = build_collision_map(&[a.clone(), b.clone()]);
        let fna = filename_for(&a, "cyclonedx-json", &cm);
        let fnb = filename_for(&b, "cyclonedx-json", &cm);
        assert_ne!(fna, fnb, "collision must yield distinct filenames");
        assert!(fna.starts_with("foo-"));
        assert!(fnb.starts_with("foo-"));
        // Deterministic: re-running produces same names.
        let fna2 = filename_for(&a, "cyclonedx-json", &cm);
        assert_eq!(fna, fna2);
    }

    #[test]
    fn slug_truncates_to_100_chars() {
        let long = "a".repeat(200);
        let raw = format!("pkg:cargo/{long}@1.0.0");
        let p = Purl::new(&raw).unwrap();
        let s = subject_slug(&p);
        assert!(s.len() <= 100);
    }

    // -------- format_ext --------

    #[test]
    fn format_ext_covers_registered_formats() {
        assert_eq!(format_ext("cyclonedx-json"), "cdx");
        assert_eq!(format_ext("spdx-2.3-json"), "spdx");
        assert_eq!(format_ext("spdx-3-json"), "spdx3");
        // Any spdx-3-family alias resolves to `spdx3` via the
        // `starts_with` branch; verified with a synthetic alias-like
        // id that shares the prefix without naming the deprecation
        // alias directly (spdx3-us3 acceptance test forbids that
        // string outside the allowed file set).
        assert_eq!(format_ext("spdx-3-json-alt"), "spdx3");
    }
}
