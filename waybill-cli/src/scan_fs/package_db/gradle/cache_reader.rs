//! Milestone 235 US2 — Gradle cache reader.
//!
//! When the operator has NOT enabled the US1 subprocess resolver
//! (or when it declined / failed), this reader walks
//! `${GRADLE_USER_HOME:-~/.gradle}/caches/modules-2/files-2.1/` for
//! cached POM files and reconstructs the resolved transitive graph
//! by BFS'ing from the project's directly-declared coordinates.
//!
//! Runs entirely offline; no JDK needed. Requires only that Gradle
//! has resolved the project at least once locally (so the artifact
//! bytes + POMs are cached).
//!
//! Spec: `specs/235-gradle-transitive-ladder/spec.md` FR-004.
//! Contract: `specs/235-gradle-transitive-ladder/contracts/gradle-cache-reader.md`.
//! Research: R4 cache layout + POM parsing.
//!
//! ## Cache layout
//!
//! Gradle 6.x-9.x stores resolved artifact files at
//! `<gradle_user_home>/caches/modules-2/files-2.1/<group>/<artifact>/<version>/<sha>/<artifact>-<version>.pom`.
//! The `metadata-2.<N>/` sibling dir holds Gradle's internal binary
//! descriptors; MVP intentionally reads only the POM files (which
//! Gradle downloads verbatim from the source repository — same
//! bytes as the Maven Central copy).
//!
//! ## Deferred (Phase 4b follow-on)
//!
//! - `.module` Gradle Module Metadata parsing (JSON; carries
//!   variant-aware info the POM lacks for KMP / Android AAR variants).
//! - Strict miss threshold (currently we walk best-effort; a large
//!   fraction of missing seeds should degrade to the next tier).
//!
//! ## Landed follow-on
//!
//! - C149 `waybill:cache-freshness = fresh|stale` per-component
//!   annotation on cache-tier components (see `cache_freshness` below).

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::ladder::{EdgeScope, GradleEdge, GradleResolvedGraph};
use super::static_parser::DirectCoord;
use super::tier::GradleResolutionTier;
use crate::scan_fs::package_db::PackageDbEntry;
use waybill_common::types::purl::{encode_purl_segment, Purl};

/// Resolved Maven coordinate `<group>:<artifact>:<version>`.
///
/// Simpler cousin of `DirectCoord` from `static_parser.rs`; both types
/// exist for MVP simplicity — future refactor may unify.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

impl From<&DirectCoord> for MavenCoord {
    fn from(dc: &DirectCoord) -> Self {
        Self {
            group: dc.group.clone(),
            artifact: dc.artifact.clone(),
            version: dc.version.clone(),
        }
    }
}

/// Cache-reader failure modes.
#[derive(Debug)]
pub enum GradleCacheError {
    /// Neither `$GRADLE_USER_HOME/caches/modules-2/files-2.1` nor
    /// `~/.gradle/caches/modules-2/files-2.1` exists.
    CacheAbsent,
    /// Cache exists but the walker found 0 POM files matching any
    /// of the seed coords. Signals to the ladder that the cache
    /// isn't warm for this project.
    NoSeedsHit,
}

/// Discover the Gradle files cache root.
///
/// Preference order:
/// 1. `WAYBILL_TEST_GRADLE_CACHE` env var (test-only override; the
///    value points at a directory that mirrors the `files-2.1/`
///    layout — no `<gradle_user_home>/caches/modules-2/` prefix
///    required). Simplifies fixture setup.
/// 2. `$GRADLE_USER_HOME/caches/modules-2/files-2.1/`
/// 3. `~/.gradle/caches/modules-2/files-2.1/`
///
/// Returns the first existing directory, or `Err(CacheAbsent)`.
pub(super) fn discover_cache() -> Result<PathBuf, GradleCacheError> {
    if let Ok(test_override) = std::env::var("WAYBILL_TEST_GRADLE_CACHE") {
        let p = PathBuf::from(test_override);
        if p.is_dir() {
            return Ok(p);
        }
        return Err(GradleCacheError::CacheAbsent);
    }
    let candidates: Vec<PathBuf> = std::env::var_os("GRADLE_USER_HOME")
        .map(|home| PathBuf::from(home).join("caches").join("modules-2").join("files-2.1"))
        .into_iter()
        .chain(std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join(".gradle")
                .join("caches")
                .join("modules-2")
                .join("files-2.1")
        }))
        .collect();
    for cand in candidates {
        if cand.is_dir() {
            return Ok(cand);
        }
    }
    Err(GradleCacheError::CacheAbsent)
}

/// Find a POM file for a specific coord in the cache.
///
/// Layout: `<cache_root>/<group>/<artifact>/<version>/<sha>/<artifact>-<version>.pom`.
/// The `<sha>` layer is Gradle's per-file hash — we walk whatever
/// subdirs exist under the version dir and pick the first `.pom` file
/// matching `<artifact>-<version>.pom` (or any `.pom` if no exact
/// match, defensive against filename variations).
pub(super) fn resolve_pom_path(
    cache_root: &Path,
    coord: &MavenCoord,
) -> Option<PathBuf> {
    let version_dir = cache_root
        .join(&coord.group)
        .join(&coord.artifact)
        .join(&coord.version);
    if !version_dir.is_dir() {
        return None;
    }
    let expected = format!("{}-{}.pom", coord.artifact, coord.version);
    // Walk one directory level (the <sha> layer) looking for the POM.
    let entries = std::fs::read_dir(&version_dir).ok()?;
    let mut fallback: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let sha_dir = entry.path();
        if !sha_dir.is_dir() {
            continue;
        }
        let inner = std::fs::read_dir(&sha_dir).ok()?;
        for file_entry in inner.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("pom") {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if file_name == expected {
                return Some(path);
            }
            if fallback.is_none() {
                fallback = Some(path);
            }
        }
    }
    fallback
}

/// Parse a POM file's `<dependencies>/<dependency>` blocks into
/// resolved `MavenCoord` list.
///
/// Skips test / provided / system scopes (runtime + compile deps
/// only). Skips deps with unresolved `${…}` version placeholders
/// (they'd need parent-POM traversal which is out of scope for
/// MVP — those coords silently drop, and the walker records a
/// warn).
pub(super) fn parse_pom_deps(pom_path: &Path) -> Vec<MavenCoord> {
    use quick_xml::events::Event;
    let Ok(bytes) = std::fs::read(pom_path) else {
        return Vec::new();
    };
    let mut reader = quick_xml::Reader::from_reader(bytes.as_slice());
    reader.trim_text(true);

    let mut out: Vec<MavenCoord> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut current_text = String::new();

    // In-progress dependency fields.
    let mut dep_g: Option<String> = None;
    let mut dep_a: Option<String> = None;
    let mut dep_v: Option<String> = None;
    let mut dep_scope: Option<String> = None;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                stack.push(name);
                current_text.clear();
            }
            Ok(Event::End(_)) => {
                let popped = stack.pop().unwrap_or_default();
                let parent = stack.last().cloned().unwrap_or_default();
                let grand = stack.iter().rev().nth(1).cloned().unwrap_or_default();

                // project/dependencies/dependency/{groupId,artifactId,version,scope}
                if parent == "dependency" {
                    match popped.as_str() {
                        "groupId" => dep_g = Some(current_text.clone()),
                        "artifactId" => dep_a = Some(current_text.clone()),
                        "version" => dep_v = Some(current_text.clone()),
                        "scope" => dep_scope = Some(current_text.clone()),
                        _ => {}
                    }
                }

                // End of <dependency> — commit the accumulated fields
                // if they represent an includable dep.
                if popped == "dependency" && parent == "dependencies" && grand == "project" {
                    let g = dep_g.take().unwrap_or_default();
                    let a = dep_a.take().unwrap_or_default();
                    let v = dep_v.take().unwrap_or_default();
                    let scope = dep_scope.take().unwrap_or_else(|| "compile".to_string());
                    if !g.is_empty()
                        && !a.is_empty()
                        && !v.is_empty()
                        && !v.contains("${")
                        && matches!(scope.as_str(), "compile" | "runtime" | "")
                    {
                        out.push(MavenCoord { group: g, artifact: a, version: v });
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let text = String::from_utf8_lossy(t.as_ref()).to_string();
                current_text.push_str(&text);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

/// Walk transitives BFS-style from the seed coord set.
///
/// For each coord popped from the frontier, resolve its POM in the
/// cache, parse its dependencies, and enqueue newly-seen coords.
/// Cycle detection via `HashSet<MavenCoord>`.
pub(super) fn walk_transitives(
    cache_root: &Path,
    seeds: &[MavenCoord],
) -> (Vec<MavenCoord>, Vec<(MavenCoord, MavenCoord)>) {
    let mut resolved: Vec<MavenCoord> = Vec::new();
    let mut edges: Vec<(MavenCoord, MavenCoord)> = Vec::new();
    let mut seen: HashSet<MavenCoord> = HashSet::new();
    let mut frontier: Vec<MavenCoord> = seeds.to_vec();

    while let Some(coord) = frontier.pop() {
        if !seen.insert(coord.clone()) {
            continue;
        }
        resolved.push(coord.clone());
        let Some(pom_path) = resolve_pom_path(cache_root, &coord) else {
            // Missing from cache — record the coord but no edges.
            continue;
        };
        let deps = parse_pom_deps(&pom_path);
        for dep in deps {
            edges.push((coord.clone(), dep.clone()));
            if !seen.contains(&dep) {
                frontier.push(dep);
            }
        }
    }
    (resolved, edges)
}

/// Build a `PackageDbEntry` from a resolved Maven coord.
///
/// Field set matches the m106 lockfile reader + m235 US1 subprocess
/// entry construction. `depends` is populated by the caller with the
/// child coordinates' `group:artifact` names so the scan_fs pipeline
/// at `mod.rs:868` synthesizes the SBOM Relationship edges.
/// C149 wire enum. Per-component cache freshness signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CacheFreshness {
    /// Newest resolved cache entry is newer than the project's
    /// build.gradle(.kts) — cache reflects the currently-declared deps.
    Fresh,
    /// build.gradle(.kts) is newer than the newest resolved cache
    /// entry (or the mtimes couldn't be read). The cache MAY have
    /// missed newer deps the operator added; downstream tools should
    /// treat cache-tier components as potentially incomplete.
    Stale,
}

impl CacheFreshness {
    pub(super) fn as_annotation_str(&self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
        }
    }
}

/// Compare the newest resolved cache-entry mtime vs the project's
/// build.gradle(.kts) mtime. Returns `Fresh` iff at least one cache
/// entry is strictly newer than the newest build script. Missing
/// mtimes on either side → `Stale` (conservative default: prefer
/// false-flagging over silently claiming cache freshness).
pub(super) fn cache_freshness(
    cache_root: &Path,
    project_dir: &Path,
    coords: &[MavenCoord],
) -> CacheFreshness {
    use std::time::SystemTime;

    let build_script_mtime = {
        let mut newest: Option<SystemTime> = None;
        for name in ["build.gradle", "build.gradle.kts"] {
            let path = project_dir.join(name);
            if let Ok(meta) = std::fs::metadata(&path) {
                if let Ok(mtime) = meta.modified() {
                    newest = Some(newest.map_or(mtime, |cur| cur.max(mtime)));
                }
            }
        }
        newest
    };
    let Some(build_mtime) = build_script_mtime else {
        return CacheFreshness::Stale;
    };

    // For each coord, look up its .pom in the cache and take that
    // file's mtime; track the newest across all resolved coords.
    let newest_cache_mtime: Option<SystemTime> = coords
        .iter()
        .filter_map(|c| resolve_pom_path(cache_root, c))
        .filter_map(|p| std::fs::metadata(&p).ok())
        .filter_map(|m| m.modified().ok())
        .max();
    let Some(cache_mtime) = newest_cache_mtime else {
        return CacheFreshness::Stale;
    };

    if cache_mtime > build_mtime {
        CacheFreshness::Fresh
    } else {
        CacheFreshness::Stale
    }
}

fn build_entry(
    coord: &MavenCoord,
    source_path: &str,
    depends: Vec<String>,
) -> Option<PackageDbEntry> {
    let purl = Purl::new(&format!(
        "pkg:maven/{}/{}@{}",
        encode_purl_segment(&coord.group),
        encode_purl_segment(&coord.artifact),
        encode_purl_segment(&coord.version),
    ))
    .ok()?;
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: format!("{}:{}", coord.group, coord.artifact),
        version: coord.version.clone(),
        arch: None,
        source_path: source_path.to_string(),
        depends,
        maintainer: None,
        licenses: Vec::new(),
        // Runtime scope by default — POM parser only extracts
        // compile/runtime deps for MVP.
        lifecycle_scope: Some(waybill_common::resolution::LifecycleScope::Runtime),
        requirement_ranges: Vec::new(),
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
        // "source" tier — same as m106 lockfile + m235 subprocess.
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations: std::collections::BTreeMap::new(),
        binary_role: None,
    })
}

/// Top-level US2 entry point.
///
/// Discovers the cache, extracts direct-dep seeds from the project's
/// `build.gradle(.kts)`, walks transitives BFS, and assembles the
/// resulting components and edges into a `GradleResolvedGraph`.
/// Returns `Err(NoSeedsHit)` when zero seeds resolved in the cache
/// so the ladder degrades to US3 / lockfile-only.
pub fn resolve_via_cache(
    project_dir: &Path,
) -> Result<GradleResolvedGraph, GradleCacheError> {
    let cache_root = discover_cache()?;

    let direct_coords = super::static_parser::extract_direct_coords(project_dir);
    let seeds: Vec<MavenCoord> = direct_coords.iter().map(MavenCoord::from).collect();
    if seeds.is_empty() {
        return Err(GradleCacheError::NoSeedsHit);
    }

    let (resolved_coords, edge_pairs) = walk_transitives(&cache_root, &seeds);
    // If none of the seeds resolved to an actual POM in the cache,
    // the cache doesn't cover this project.
    let any_seed_hit = seeds
        .iter()
        .any(|s| resolve_pom_path(&cache_root, s).is_some());
    if !any_seed_hit {
        return Err(GradleCacheError::NoSeedsHit);
    }

    let source_path = project_dir.to_string_lossy().to_string();

    // C149 per-project cache-freshness signal: is the newest cache
    // entry across the resolved coords older or newer than the
    // project's build.gradle(.kts)? Attached identically to every
    // cache-derived component so downstream tools can flag scans
    // where the cache no longer reflects the declared deps.
    let freshness = cache_freshness(&cache_root, project_dir, &resolved_coords);

    // Group edges by source coord for `depends` population.
    use std::collections::HashMap;
    let mut source_to_depends: HashMap<MavenCoord, Vec<String>> = HashMap::new();
    for (src, dst) in &edge_pairs {
        source_to_depends
            .entry(src.clone())
            .or_default()
            .push(format!("{}:{}", dst.group, dst.artifact));
    }

    let mut components: Vec<PackageDbEntry> = Vec::new();
    let mut edges: Vec<GradleEdge> = Vec::new();
    for coord in &resolved_coords {
        let depends = source_to_depends.remove(coord).unwrap_or_default();
        if let Some(mut entry) = build_entry(coord, &source_path, depends) {
            entry.extra_annotations.insert(
                "waybill:cache-freshness".to_string(),
                serde_json::Value::String(freshness.as_annotation_str().to_string()),
            );
            components.push(entry);
        }
    }
    for (src, dst) in edge_pairs {
        let Ok(src_purl) = Purl::new(&format!(
            "pkg:maven/{}/{}@{}",
            encode_purl_segment(&src.group),
            encode_purl_segment(&src.artifact),
            encode_purl_segment(&src.version),
        )) else {
            continue;
        };
        let Ok(dst_purl) = Purl::new(&format!(
            "pkg:maven/{}/{}@{}",
            encode_purl_segment(&dst.group),
            encode_purl_segment(&dst.artifact),
            encode_purl_segment(&dst.version),
        )) else {
            continue;
        };
        edges.push(GradleEdge {
            source: src_purl,
            target: dst_purl,
            edge_scope: EdgeScope::Runtime,
        });
    }

    Ok(GradleResolvedGraph {
        components,
        edges,
        tier: GradleResolutionTier::Cache,
        fallback_history: Vec::new(),
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_pom(cache_root: &Path, g: &str, a: &str, v: &str, deps: &[(&str, &str, &str)]) {
        let sha_dir = cache_root.join(g).join(a).join(v).join("dummysha");
        std::fs::create_dir_all(&sha_dir).unwrap();
        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\"?>\n<project>\n<dependencies>\n");
        for (dg, da, dv) in deps {
            xml.push_str(&format!(
                "<dependency><groupId>{dg}</groupId><artifactId>{da}</artifactId><version>{dv}</version></dependency>\n"
            ));
        }
        xml.push_str("</dependencies>\n</project>\n");
        let pom_path = sha_dir.join(format!("{a}-{v}.pom"));
        let mut f = std::fs::File::create(&pom_path).unwrap();
        f.write_all(xml.as_bytes()).unwrap();
    }

    #[test]
    fn resolve_pom_path_finds_pom() {
        let td = TempDir::new().unwrap();
        write_pom(td.path(), "com.example", "root", "1.0.0", &[]);
        let coord = MavenCoord {
            group: "com.example".to_string(),
            artifact: "root".to_string(),
            version: "1.0.0".to_string(),
        };
        let path = resolve_pom_path(td.path(), &coord);
        assert!(path.is_some());
        assert!(path.unwrap().to_string_lossy().ends_with("root-1.0.0.pom"));
    }

    #[test]
    fn parse_pom_deps_extracts_dependencies() {
        let td = TempDir::new().unwrap();
        write_pom(
            td.path(),
            "com.example",
            "root",
            "1.0.0",
            &[("com.example", "leaf", "2.0.0")],
        );
        let coord = MavenCoord {
            group: "com.example".to_string(),
            artifact: "root".to_string(),
            version: "1.0.0".to_string(),
        };
        let path = resolve_pom_path(td.path(), &coord).unwrap();
        let deps = parse_pom_deps(&path);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].artifact, "leaf");
        assert_eq!(deps[0].version, "2.0.0");
    }

    #[test]
    // walker-audit: false-positive — #[test] function name shares the walk_ prefix of the unit under test
    fn walk_transitives_bfs_two_levels() {
        let td = TempDir::new().unwrap();
        write_pom(
            td.path(),
            "com.example",
            "root",
            "1.0.0",
            &[("com.example", "leaf", "2.0.0")],
        );
        write_pom(td.path(), "com.example", "leaf", "2.0.0", &[]);
        let seeds = vec![MavenCoord {
            group: "com.example".to_string(),
            artifact: "root".to_string(),
            version: "1.0.0".to_string(),
        }];
        let (coords, edges) = walk_transitives(td.path(), &seeds);
        assert_eq!(coords.len(), 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].0.artifact, "root");
        assert_eq!(edges[0].1.artifact, "leaf");
    }

    #[test]
    // walker-audit: false-positive — #[test] function name shares the walk_ prefix of the unit under test
    fn walk_transitives_cycle_detection() {
        // A → B → A cycle — walker must terminate.
        let td = TempDir::new().unwrap();
        write_pom(
            td.path(),
            "com.example",
            "a",
            "1.0.0",
            &[("com.example", "b", "1.0.0")],
        );
        write_pom(
            td.path(),
            "com.example",
            "b",
            "1.0.0",
            &[("com.example", "a", "1.0.0")],
        );
        let seeds = vec![MavenCoord {
            group: "com.example".to_string(),
            artifact: "a".to_string(),
            version: "1.0.0".to_string(),
        }];
        let (coords, _edges) = walk_transitives(td.path(), &seeds);
        assert_eq!(coords.len(), 2);
    }
}
