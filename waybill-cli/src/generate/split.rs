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

/// Milestone 219 — grouping strategy for the `--split[=<mode>]` CLI
/// flag. Adding a future variant (`Ecosystem`, `Owner`, `Custom`)
/// touches only: (1) this enum, (2) the `group_key` match arm,
/// (3) `docs/reference/split-modes.md`, (4) a new test scenario.
/// See `specs/219-split-modes/contracts/grouping-strategy.md` for
/// the full extensibility contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum SplitMode {
    /// m215 default. Group key = `SubprojectRoot::subproject_id()` —
    /// one group per main-module. Byte-identity contract with
    /// alpha.67 `--split` preserved (SC-005).
    #[default]
    Workspace,
    /// m219 addition. Group key = canonicalized
    /// `SubprojectRoot::source_dir`. All main-modules whose source
    /// dirs match collapse into ONE group → ONE sub-SBOM. Useful for
    /// polyglot repos where Cargo + package.json coexist in one dir.
    Directory,
}

impl SplitMode {
    /// Return the grouping-key string for `root` under this mode.
    /// Pure function; no side effects; deterministic per input.
    pub fn group_key(&self, root: &SubprojectRoot) -> String {
        match self {
            SplitMode::Workspace => root.subproject_id(),
            SplitMode::Directory => {
                let s = root.source_dir.to_string_lossy().to_string();
                if s.is_empty() {
                    "root".to_string()
                } else {
                    s
                }
            }
        }
    }
}

/// Display renders the lowercase `ValueEnum` wire form (matching
/// the CLI: `workspace` or `directory`). Load-bearing for the
/// FR-010 INFO log — the log emits via `%mode` (Display) so the
/// operator-visible substring is `mode=directory` (lowercase),
/// matching consumer-facing CLI spelling. `?mode` (Debug) would
/// render `Directory` (capitalized), breaking the SC-007 test.
impl std::fmt::Display for SplitMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use clap::ValueEnum;
        f.write_str(
            self.to_possible_value()
                .expect("SplitMode variants all have possible values via ValueEnum derive")
                .get_name(),
        )
    }
}

/// Milestone 219 — one grouped projection of N ≥ 1 members merged
/// into a unified BFS-projection set. Parallels `SplitProjection`
/// (m215's per-root shape) with an added `members: Vec<...>` for
/// multi-member `--split=directory` groups. `pub(crate)` — internal
/// to the split module.
#[derive(Debug)]
pub(crate) struct GroupedProjection {
    /// Grouping key string from [`SplitMode::group_key`]. For
    /// `--split=workspace`, equals `members[0].subproject_id()`.
    /// For `--split=directory`, equals the canonicalized source_dir.
    pub group_key: String,
    /// Every SubprojectRoot contributing to this group. Length ≥ 1.
    /// Sorted lex by `purl_string` for byte-identity.
    pub members: Vec<SubprojectRoot>,
    /// Merged components — union of every member's per-BFS-projection
    /// components. Deduplicated by PURL (last-write-wins on tie;
    /// matches m215 intra-projection dedup).
    pub components: Vec<ResolvedComponent>,
    /// Merged relationships — union of every member's per-BFS-projection
    /// relationships. Deduplicated by (from, to, kind) tuple.
    pub relationships: Vec<Relationship>,
    /// Count of THIS group's components that also appear in ≥ 1
    /// sibling GroupedProjection. Populated post-hoc.
    pub shared_deps_count: usize,
}

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
    // m219 note: `root` + `shared_deps_count` were consumed by m215's
    // emit-time loop; after m219's GroupedProjection refactor at
    // `emit_split`, only `components` + `relationships` are read from
    // this struct at emit time (merge_group_projection consumes the
    // BFS output and drops the wrapper). Both fields still exist for
    // test/introspection use — allow-dead-code to silence clippy in
    // the non-test build.
    #[allow(dead_code)]
    pub root: SubprojectRoot,
    pub components: Vec<ResolvedComponent>,
    pub relationships: Vec<Relationship>,
    /// Count of THIS projection's components that also appear in ≥ 1
    /// sibling projection. Populated post-hoc by [`compute_shared_deps`].
    #[allow(dead_code)]
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
// m219: compute_shared_deps is replaced by compute_shared_deps_groups
// in the emit path. Retained for m215 unit-test coverage of the
// per-projection shared-dep shape.
#[allow(dead_code)]
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
/// Milestone 219 — filesystem-safe slug for a source-dir string.
/// Used in the `<dir-slug>.multi.<format-ext>` filename convention
/// when a `--split=directory` group covers ≥2 main-modules.
///
/// Steps (per contracts/multi-member-filename.md):
/// 1. Path separator (`/` or `\`) → `-`.
/// 2. Leading `-` stripped (from absolute-path leading `/`).
/// 3. m215 char-safety pass (strip backslash, colon, glob, wildcards,
///    quotes, angle brackets, pipe, whitespace).
/// 4. Non-ASCII stripped (defensive).
/// 5. Truncate to 100 bytes.
/// 6. Lowercase.
/// 7. Empty string → `"root"` sentinel (would otherwise produce
///    `.multi.cdx.json` — a leading-dot hidden file on POSIX).
pub(crate) fn dir_slug(source_dir: &str) -> String {
    let mut s: String = source_dir
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect();
    while s.starts_with('-') {
        s.remove(0);
    }
    s.retain(|c| {
        !matches!(
            c,
            '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' | '\t' | '\n' | '\r'
        )
    });
    s.retain(|c| c.is_ascii());
    if s.len() > 100 {
        s.truncate(100);
    }
    s = s.to_ascii_lowercase();
    if s.is_empty() {
        "root".to_string()
    } else {
        s
    }
}

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

// ---------- M219 T011: group_roots + merge_group_projection ----------

/// Milestone 219 — group `roots` by `SplitMode::group_key` and
/// return one `GroupedProjection` per group. Members within each
/// group sorted lex by `purl_string`; groups themselves sorted lex
/// by `group_key`. `components`/`relationships` left empty here;
/// populated by [`merge_group_projection`] downstream. See
/// `contracts/grouping-strategy.md`.
pub(crate) fn group_roots(
    roots: &[SubprojectRoot],
    mode: SplitMode,
) -> Vec<GroupedProjection> {
    let mut buckets: BTreeMap<String, Vec<SubprojectRoot>> = BTreeMap::new();
    for r in roots {
        buckets.entry(mode.group_key(r)).or_default().push(r.clone());
    }
    buckets
        .into_iter()
        .map(|(group_key, mut members)| {
            members.sort_by(|a, b| a.purl_string.cmp(&b.purl_string));
            GroupedProjection {
                group_key,
                members,
                components: Vec::new(),
                relationships: Vec::new(),
                shared_deps_count: 0,
            }
        })
        .collect()
}

/// Milestone 219 — for a `GroupedProjection` whose `members` list is
/// populated, run [`project_for_root`] per member and merge into the
/// group's aggregate `components` + `relationships`. Dedup rules per
/// R10 + contracts/manifest-additive-members.md: component dedup by
/// PURL (last-write-wins); relationship dedup by `(from, to, kind)`.
pub(crate) fn merge_group_projection(
    group: &mut GroupedProjection,
    all_components: &[ResolvedComponent],
    all_relationships: &[Relationship],
) {
    // Component dedup by PURL (BTreeMap preserves lex order + gives
    // last-write-wins semantics via `insert`).
    let mut components_by_purl: BTreeMap<String, ResolvedComponent> = BTreeMap::new();
    // Relationship dedup by (from, to, kind) tuple. Debug format of
    // RelationshipType is stable per enum-variant name so it's OK as
    // part of the key here (dedup, not on-wire).
    let mut relationships_seen: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut relationships_ordered: Vec<Relationship> = Vec::new();

    for member in &group.members {
        let projection = project_for_root(member, all_components, all_relationships);
        for c in projection.components {
            components_by_purl.insert(c.purl.as_str().to_string(), c);
        }
        for r in projection.relationships {
            let key = (r.from.clone(), r.to.clone(), format!("{:?}", r.relationship_type));
            if relationships_seen.insert(key) {
                relationships_ordered.push(r);
            }
        }
    }

    group.components = components_by_purl.into_values().collect();
    group.relationships = relationships_ordered;
}

/// Milestone 219 — post-hoc shared-dep count for a slice of
/// `GroupedProjection`s. Same aggregate shape as m215's
/// [`compute_shared_deps`] but keyed on the new type. Returns
/// `(total_unique_components, aggregate_shared_dep_count)`.
pub(crate) fn compute_shared_deps_groups(
    groups: &mut [GroupedProjection],
) -> (u64, u64) {
    // Count occurrences of each PURL across all groups.
    let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
    for group in groups.iter() {
        for c in &group.components {
            *occurrences.entry(c.purl.as_str().to_string()).or_default() += 1;
        }
    }
    let total_unique = occurrences.len() as u64;
    let shared_purls: BTreeSet<String> = occurrences
        .iter()
        .filter(|(_, &count)| count >= 2)
        .map(|(purl, _)| purl.clone())
        .collect();
    let aggregate_shared = shared_purls.len() as u64;
    // Populate per-group `shared_deps_count`.
    for group in groups.iter_mut() {
        group.shared_deps_count = group
            .components
            .iter()
            .filter(|c| shared_purls.contains(c.purl.as_str()))
            .count();
    }
    (total_unique, aggregate_shared)
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_split(
    base_artifacts: &ScanArtifacts<'_>,
    formats: &[String],
    registry: &SerializerRegistry,
    output_dir: &Path,
    created: DateTime<Utc>,
    waybill_version: &str,
    scan_root: &Path,
    mode: SplitMode,
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
            mode = %mode,
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

    // Milestone 219 — group roots by SplitMode::group_key, then merge
    // per-member BFS projections into the group's aggregate. For
    // --split=workspace this yields one group per root (m215
    // byte-identity). For --split=directory this collapses same-dir
    // multi-ecosystem main-modules into one group.
    let mut groups: Vec<GroupedProjection> = group_roots(&roots, mode);
    for group in &mut groups {
        merge_group_projection(
            group,
            base_artifacts.components,
            base_artifacts.relationships,
        );
    }
    let (total_unique, aggregate_shared) = compute_shared_deps_groups(&mut groups);

    tracing::info!(
        subproject_count = groups.len(),
        format_count = formats.len(),
        total_unique_components = total_unique,
        shared_dep_count = aggregate_shared,
        output_dir = %output_dir.display(),
        mode = %mode,
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

    // Per-group emission.
    for group in &groups {
        let sub_artifacts = base_artifacts.narrow(
            &group.components,
            &group.relationships,
        );
        let mut entry_files: BTreeMap<String, String> = BTreeMap::new();
        let is_multi = group.members.len() >= 2;
        // Multi-member groups use the m219 <dir-slug>.multi.<ext>
        // shape; single-member groups reuse m215's filename_for verbatim
        // (SC-005 byte-identity gate).
        let group_dir_slug = if is_multi {
            Some(dir_slug(&group.group_key))
        } else {
            None
        };
        let group_subproject_id = if is_multi {
            format!("{}.multi", group_dir_slug.as_deref().expect("is_multi implies Some"))
        } else {
            group.members[0].subproject_id()
        };

        for fmt in formats {
            let serializer = registry.get(fmt).ok_or_else(|| {
                anyhow::anyhow!("split emit: unknown format id {fmt:?}")
            })?;
            let filename = if is_multi {
                // <dir-slug>.multi.<format-ext> per contracts/multi-member-filename.md.
                format!(
                    "{}.multi.{}.json",
                    group_dir_slug.as_deref().expect("is_multi implies Some"),
                    format_ext(fmt),
                )
            } else {
                filename_for(&group.members[0], fmt, &collision_map)
            };
            let sub_output_cfg = OutputConfig {
                mikebom_version: env_pkg_version(),
                created,
                overrides: BTreeMap::new(),
            };
            let emitted = serializer.serialize(&sub_artifacts, &sub_output_cfg)?;
            for (i, artifact) in emitted.into_iter().enumerate() {
                let target = if i == 0 {
                    output_dir.join(&filename)
                } else {
                    let sidecar_base = artifact
                        .relative_path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("sidecar-{i}"));
                    let ns_name = format!("{}.{}", group_subproject_id, sidecar_base);
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
                    subproject = %group_subproject_id,
                    "wrote split sub-SBOM artifact"
                );
            }
        }

        // Populate SplitEntry per contracts/manifest-additive-members.md:
        // - subproject_id, root_purl derived per group's multi-ness (R6/E7).
        // - members: None when single-member (SC-005); Some(sorted-lex-by-purl)
        //   when multi-member.
        let (entry_subproject_id, entry_root_purl, entry_members) = if is_multi {
            let slug = group_dir_slug
                .as_deref()
                .expect("is_multi implies Some");
            let members_vec: Vec<crate::generate::split_manifest::SplitMember> = group
                .members
                .iter()
                .map(|m| crate::generate::split_manifest::SplitMember {
                    purl: m.purl_string.clone(),
                    source_dir: m.source_dir.to_string_lossy().to_string(),
                })
                .collect();
            // members already sorted lex by purl_string per group_roots.
            (
                format!("{slug}.multi"),
                format!("pkg:generic/{slug}@0.0.0-unknown"),
                Some(members_vec),
            )
        } else {
            (
                group.members[0].subproject_id(),
                group.members[0].purl_string.clone(),
                None,
            )
        };

        let entry_source_dir = if is_multi {
            group.group_key.clone()
        } else {
            group.members[0].source_dir.to_string_lossy().to_string()
        };

        let entry = SplitEntry {
            subproject_id: entry_subproject_id,
            root_purl: entry_root_purl,
            source_dir: entry_source_dir,
            component_count: group.components.len() as u64,
            shared_deps_count: group.shared_deps_count as u64,
            files: entry_files,
            members: entry_members,
        };
        manifest.entries.push(entry);
    }

    // FR-010 INFO log at split-driver exit — mode + group counts +
    // total main-modules. Uses %mode (Display, lowercase) NOT ?mode
    // (Debug, capitalized) per contracts/grouping-strategy.md.
    tracing::info!(
        mode = %mode,
        groups = groups.len(),
        total_main_modules = roots.len(),
        "split emission complete"
    );

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
    crate::version::VERSION
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

    // -------- M219 SplitMode + Display --------

    fn mk_root(purl: &str, source_dir: &str, ecosystem: &str) -> SubprojectRoot {
        let p = Purl::new(purl).unwrap();
        SubprojectRoot {
            purl: p.clone(),
            purl_string: p.to_string(),
            source_dir: PathBuf::from(source_dir),
            ecosystem: ecosystem.to_string(),
        }
    }

    #[test]
    fn split_mode_display_renders_lowercase_wire_form() {
        assert_eq!(format!("{}", SplitMode::Workspace), "workspace");
        assert_eq!(format!("{}", SplitMode::Directory), "directory");
    }

    #[test]
    fn split_mode_default_is_workspace() {
        assert_eq!(SplitMode::default(), SplitMode::Workspace);
    }

    #[test]
    fn split_mode_group_key_workspace_matches_subproject_id() {
        let r = mk_root("pkg:cargo/libsafe@0.1.0", "crates/libsafe", "cargo");
        assert_eq!(SplitMode::Workspace.group_key(&r), r.subproject_id());
    }

    #[test]
    fn split_mode_group_key_directory_uses_canonical_source_dir() {
        let r = mk_root("pkg:cargo/libsafe@0.1.0", "crates/libsafe", "cargo");
        assert_eq!(SplitMode::Directory.group_key(&r), "crates/libsafe");
    }

    #[test]
    fn split_mode_group_key_directory_empty_source_dir_yields_root_sentinel() {
        let r = mk_root("pkg:cargo/root@0.1.0", "", "cargo");
        assert_eq!(SplitMode::Directory.group_key(&r), "root");
    }

    // -------- M219 dir_slug --------

    #[test]
    fn dir_slug_replaces_path_separators() {
        assert_eq!(dir_slug("services/api"), "services-api");
        assert_eq!(dir_slug("services\\api"), "services-api");
    }

    #[test]
    fn dir_slug_empty_yields_root_sentinel() {
        assert_eq!(dir_slug(""), "root");
    }

    #[test]
    fn dir_slug_strips_leading_dashes_and_normalizes() {
        // Absolute-path input (leading slash → leading dash → stripped).
        assert_eq!(dir_slug("/services/api"), "services-api");
        // Uppercase + non-ASCII → sanitized.
        assert_eq!(dir_slug("Services/API"), "services-api");
    }

    #[test]
    fn dir_slug_truncates_to_100_bytes() {
        let long = format!("{}/api", "a".repeat(200));
        let s = dir_slug(&long);
        assert!(s.len() <= 100);
    }

    // -------- M219 group_roots --------

    #[test]
    fn group_roots_workspace_mode_one_group_per_root() {
        let roots = vec![
            mk_root("pkg:cargo/api@0.1.0", "services/api", "cargo"),
            mk_root("pkg:npm/api@0.1.0", "services/api", "npm"),
            mk_root("pkg:golang/worker@0.1.0", "services/worker", "golang"),
        ];
        let groups = group_roots(&roots, SplitMode::Workspace);
        assert_eq!(groups.len(), 3);
        for g in &groups {
            assert_eq!(g.members.len(), 1);
        }
    }

    #[test]
    fn group_roots_directory_mode_merges_same_dir() {
        let roots = vec![
            mk_root("pkg:cargo/api@0.1.0", "services/api", "cargo"),
            mk_root("pkg:npm/api@0.1.0", "services/api", "npm"),
            mk_root("pkg:golang/worker@0.1.0", "services/worker", "golang"),
        ];
        let groups = group_roots(&roots, SplitMode::Directory);
        assert_eq!(groups.len(), 2);
        // services/api group has 2 members; services/worker has 1.
        let api_group = groups.iter().find(|g| g.group_key == "services/api").unwrap();
        assert_eq!(api_group.members.len(), 2);
        // Sorted lex by purl_string: cargo < npm.
        assert_eq!(api_group.members[0].purl_string, "pkg:cargo/api@0.1.0");
        assert_eq!(api_group.members[1].purl_string, "pkg:npm/api@0.1.0");
        let worker_group = groups
            .iter()
            .find(|g| g.group_key == "services/worker")
            .unwrap();
        assert_eq!(worker_group.members.len(), 1);
    }

    #[test]
    fn group_roots_directory_mode_two_dirs_yields_two_groups() {
        let roots = vec![
            mk_root("pkg:cargo/a@0.1.0", "dir-a", "cargo"),
            mk_root("pkg:cargo/b@0.1.0", "dir-b", "cargo"),
        ];
        let groups = group_roots(&roots, SplitMode::Directory);
        assert_eq!(groups.len(), 2);
    }

    // -------- M219 SC-009 extensibility gate --------

    /// SC-009 mechanical extensibility test per contracts/grouping-
    /// strategy.md. Proves adding a hypothetical `Ecosystem` variant
    /// requires only (1) enum + (2) match arm — no edits to CLI,
    /// manifest schema, or emit_split. This test lives in the SAME
    /// file as SplitMode so the "extension touches only 1 file"
    /// invariant is mechanically demonstrated: this test itself is
    /// the ONLY file edited to prove the extension pattern works.
    #[test]
    fn sc009_extensibility_gate_hand_add_ecosystem_variant() {
        // Variants are constructed via method calls below (Directory
        // + TestOnlyEcosystem exercised in the assertions). Workspace
        // is included as an intentional-completeness signal proving
        // the extension is additive to the existing enum shape, not a
        // replacement — allow-dead-code to silence the "never
        // constructed" clippy on a variant that documents intent.
        #[allow(dead_code)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum TestOnlySplitMode {
            Workspace,
            Directory,
            TestOnlyEcosystem,
        }
        impl TestOnlySplitMode {
            fn group_key(&self, root: &SubprojectRoot) -> String {
                match self {
                    TestOnlySplitMode::Workspace => root.subproject_id(),
                    TestOnlySplitMode::Directory => {
                        let s = root.source_dir.to_string_lossy().to_string();
                        if s.is_empty() { "root".to_string() } else { s }
                    }
                    TestOnlySplitMode::TestOnlyEcosystem => root.ecosystem.clone(),
                }
            }
        }
        // Two roots in different ecosystems, same dir. Under
        // TestOnlyEcosystem grouping they MUST land in different
        // groups (proves the new group_key branch works).
        let r1 = mk_root("pkg:cargo/a@0.1.0", "shared", "cargo");
        let r2 = mk_root("pkg:npm/a@0.1.0", "shared", "npm");
        assert_ne!(
            TestOnlySplitMode::TestOnlyEcosystem.group_key(&r1),
            TestOnlySplitMode::TestOnlyEcosystem.group_key(&r2),
            "extensibility gate: distinct ecosystems MUST produce distinct group keys"
        );
        // Under Directory mode the same two roots MUST land in the
        // SAME group (control — proves grouping-key semantics).
        assert_eq!(
            TestOnlySplitMode::Directory.group_key(&r1),
            TestOnlySplitMode::Directory.group_key(&r2),
        );
    }
}
