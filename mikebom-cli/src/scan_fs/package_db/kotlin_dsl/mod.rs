//! Kotlin DSL Gradle source-tree reader (milestone 122 US2).
//!
//! Regex-extracts dependency declarations from `build.gradle.kts` files
//! (the Android-Studio / IntelliJ default since 2023) and resolves
//! `libs.<alias>` references against the workspace's `gradle/libs.versions.toml`
//! version catalog. Emits `pkg:maven/<group>/<name>@<version>` per the
//! existing milestone-106 `pkg:maven/` lane so downstream deps.dev / OSV
//! enrichment applies without changes.
//!
//! Multi-module workspaces declared via `settings.gradle.kts` synthesize a
//! `pkg:generic/<rootProject.name>@0.0.0` workspace-root component per
//! clarification Q4 / FR-007. Only the OUTERMOST `settings.gradle.kts`
//! per scan tree is treated as a workspace root (nested settings files
//! are walked for sibling `build.gradle.kts` discovery only).
//!
//! KMP source-set provenance rides `mikebom:kmp-source-set` as a JSON-
//! encoded array per clarification Q2 / FR-006. The reader emits one
//! `PackageDbEntry` per `(dep × source-set)` tuple pre-dedup, each
//! carrying the SAME merged source-set array; the milestone-105 dedup
//! pipeline collapses them deterministically.
//!
//! `build.gradle.kts`-discovered components are design-tier
//! (`mikebom:sbom-tier = "design"`) gated by the existing
//! `--include-declared-deps` flag per clarification Q5. The dispatcher
//! threads the `include_dev` parameter through to `read()`.

pub(super) mod build_script;
pub(super) mod settings;
pub(super) mod version_catalog;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mikebom_common::types::purl::Purl;
use serde_json::Value as JsonValue;

use super::PackageDbEntry;
use settings::SettingsScript;
use version_catalog::VersionCatalog;

/// Tracks which KMP source-set(s) declared each PURL. Per FR-006
/// timing contract: the reader emits one `PackageDbEntry` per
/// `(dep × source-set)` pre-dedup; this tracker collects the merged
/// source-set array, then the orchestrator stamps the SAME merged
/// array on every duplicate so the milestone-105 dedup preserves the
/// merged value regardless of which duplicate it picks as canonical.
#[derive(Debug, Default)]
pub(super) struct KmpSourceSetTracker {
    /// Keyed by canonical PURL string (`Purl` isn't `Ord`, so we key
    /// by the string the canonical form provides via `Purl::as_str`).
    /// `BTreeSet` preserves lex order on source-set names for
    /// determinism — two scans of the same project produce byte-
    /// identical `mikebom:kmp-source-set` values.
    map: BTreeMap<String, BTreeSet<String>>,
}

impl KmpSourceSetTracker {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn record(&mut self, purl: Purl, source_set: String) {
        self.map
            .entry(purl.as_str().to_string())
            .or_default()
            .insert(source_set);
    }

    /// Finalize the tracker into `(canonical-PURL-string, JSON-array)`
    /// pairs ready to stamp onto matching components' `extra_annotations`
    /// under the `mikebom:kmp-source-set` key.
    pub(super) fn finalize(self) -> Vec<(String, JsonValue)> {
        self.map
            .into_iter()
            .map(|(purl_str, set)| {
                let arr: Vec<JsonValue> = set.into_iter().map(JsonValue::String).collect();
                (purl_str, JsonValue::Array(arr))
            })
            .collect()
    }
}

/// Walk `rootfs` for `settings.gradle.kts` + `build.gradle.kts` +
/// `gradle/libs.versions.toml` files. Outermost `settings.gradle.kts`
/// per scan tree synthesizes a workspace-root component;
/// `build.gradle.kts` files emit design-tier `pkg:maven/...` components.
///
/// `include_dev` per clarification Q5: when `false`, design-tier
/// components drop from the output; when `true`, they emit. The
/// dispatcher threads the `--include-declared-deps` flag here (auto-on
/// for `--path`, opt-in for `--image`).
pub fn read(
    rootfs: &Path,
    include_dev: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: 8,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };

    // First pass: collect every `settings.gradle.kts` + `build.gradle.kts`
    // + `gradle/libs.versions.toml` location into separate vectors so we
    // can process workspace-root determination before resolving deps.
    let mut settings_files: Vec<PathBuf> = Vec::new();
    let mut build_files: Vec<PathBuf> = Vec::new();
    let mut catalog_files: Vec<PathBuf> = Vec::new();

    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |project_dir| {
        if !project_dir.is_dir() {
            return;
        }
        let s = project_dir.join("settings.gradle.kts");
        if s.is_file() {
            settings_files.push(s);
        }
        let b = project_dir.join("build.gradle.kts");
        if b.is_file() {
            build_files.push(b);
        }
        let cat = project_dir.join("gradle").join("libs.versions.toml");
        if cat.is_file() {
            catalog_files.push(cat);
        }
    });

    if settings_files.is_empty() && build_files.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<PackageDbEntry> = Vec::new();

    // Workspace-root synthesis — only the OUTERMOST settings.gradle.kts
    // (shortest path) becomes a workspace root. Nested settings files
    // are walked for build.gradle.kts content via the standard walker
    // path but DO NOT emit additional workspace-root components.
    settings_files.sort_by_key(|p| p.components().count());
    if let Some(outer) = settings_files.first() {
        if let Ok(s) = settings::parse(outer) {
            out.push(synthesize_workspace_root(&s));
        }
    }

    // Parse all catalogs once into a fast lookup by their parent
    // workspace directory. Each `build.gradle.kts` consults the
    // nearest catalog by walking up.
    let catalogs: Vec<VersionCatalog> = catalog_files
        .iter()
        .filter_map(|p| match version_catalog::parse(p) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "kotlin_dsl: libs.versions.toml parse failed; treating catalog as empty"
                );
                None
            }
        })
        .collect();

    let mut tracker = KmpSourceSetTracker::new();
    let mut emitted: Vec<PackageDbEntry> = Vec::new();

    for build_path in &build_files {
        let Ok(content) = std::fs::read_to_string(build_path) else {
            tracing::warn!(
                path = %build_path.display(),
                "kotlin_dsl: build.gradle.kts unreadable; skipping"
            );
            continue;
        };
        let entries = build_script::extract_deps(&content);
        let catalog = nearest_catalog(build_path, &catalogs);
        let mut emitted_for_file =
            build_script::resolve_and_emit(entries, catalog, build_path, &mut tracker);
        emitted.append(&mut emitted_for_file);
    }

    // Stamp the merged source-set arrays onto every matching duplicate
    // per the FR-006 timing contract — every pre-dedup duplicate of the
    // same canonical PURL gets the SAME merged array so the milestone-
    // 105 dedup pipeline preserves it regardless of which duplicate it
    // picks as canonical.
    let source_set_pairs = tracker.finalize();
    for (purl_str, arr) in source_set_pairs {
        for entry in emitted.iter_mut() {
            if entry.purl.as_str() == purl_str {
                entry
                    .extra_annotations
                    .insert("mikebom:kmp-source-set".to_string(), arr.clone());
            }
        }
    }

    // Design-tier gating per Q5.
    if include_dev {
        out.extend(emitted);
    } else {
        out.extend(
            emitted
                .into_iter()
                .filter(|e| e.sbom_tier.as_deref() != Some("design")),
        );
    }

    out
}

/// Synthesize the workspace-root `PackageDbEntry` for a parsed
/// `SettingsScript`. PURL `pkg:generic/<rootProject.name>@0.0.0` per
/// clarification Q4 + FR-007. Falls back to the workspace directory
/// name when `rootProject.name` is `None`.
fn synthesize_workspace_root(s: &SettingsScript) -> PackageDbEntry {
    let dir_name = s
        .source_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("workspace-root")
        .to_string();
    let root_name = s.root_project_name.clone().unwrap_or(dir_name);
    let purl = Purl::new(&format!(
        "pkg:generic/{}@0.0.0",
        mikebom_common::types::purl::encode_purl_segment(&root_name)
    ))
    .expect("workspace-root PURL is always valid (generic ecosystem + encoded segment)");
    let source_path_str = s.source_path.to_string_lossy().into_owned();
    let mut extra_annotations: BTreeMap<String, JsonValue> = Default::default();
    extra_annotations.insert(
        "mikebom:component-role".to_string(),
        JsonValue::String("workspace-root".to_string()),
    );
    extra_annotations.insert(
        "mikebom:source-files".to_string(),
        JsonValue::String(source_path_str.clone()),
    );
    PackageDbEntry {
        build_inclusion: None,
        purl,
        name: root_name,
        version: "0.0.0".to_string(),
        arch: None,
        source_path: source_path_str,
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
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
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    }
}

/// Find the catalog whose `gradle/libs.versions.toml` lives nearest
/// (highest path component count match) to `build_path`'s parent.
/// Returns `None` when no catalog applies (single-module project
/// without a catalog).
fn nearest_catalog<'a>(
    build_path: &Path,
    catalogs: &'a [VersionCatalog],
) -> Option<&'a VersionCatalog> {
    let mut best: Option<&VersionCatalog> = None;
    let mut best_depth: usize = 0;
    for cat in catalogs {
        // The catalog's "owning directory" is two parents up from the
        // catalog file (file → gradle/ → workspace).
        let owning_dir = cat.source_path.parent().and_then(|p| p.parent());
        let Some(owning_dir) = owning_dir else { continue };
        if build_path.starts_with(owning_dir) {
            let depth = owning_dir.components().count();
            if depth >= best_depth {
                best_depth = depth;
                best = Some(cat);
            }
        }
    }
    best
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn tracker_records_and_finalizes_lex_sorted() {
        let mut t = KmpSourceSetTracker::new();
        let purl = Purl::new("pkg:maven/io.example/lib@1.0.0").unwrap();
        t.record(purl.clone(), "jvmMain".to_string());
        t.record(purl.clone(), "commonMain".to_string());
        t.record(purl.clone(), "jvmMain".to_string()); // dup absorbed
        let out = t.finalize();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "pkg:maven/io.example/lib@1.0.0");
        let arr = out[0].1.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("commonMain"));
        assert_eq!(arr[1].as_str(), Some("jvmMain"));
    }

    #[test]
    fn tracker_keeps_cross_purl_records_independent() {
        let mut t = KmpSourceSetTracker::new();
        let a = Purl::new("pkg:maven/g/a@1.0.0").unwrap();
        let b = Purl::new("pkg:maven/g/b@1.0.0").unwrap();
        t.record(a.clone(), "commonMain".to_string());
        t.record(b.clone(), "jvmMain".to_string());
        let out = t.finalize();
        assert_eq!(out.len(), 2);
    }
}
