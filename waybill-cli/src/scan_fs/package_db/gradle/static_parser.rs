//! Milestone 235 US3 — Gradle static parser.
//!
//! Regex-scoped DSL extractor for `build.gradle` (Groovy) +
//! `build.gradle.kts` (Kotlin) direct-dependency declarations.
//! Runs when neither US1 subprocess nor US2 cache produces
//! components — the lowest-value but always-available tier of the
//! ladder.
//!
//! MVP scope (Phase 5 core):
//! - Direct string coord patterns: `<config> "g:a:v"` and
//!   `<config>("g:a:v")`
//! - 10 recognized configurations mapped to lifecycle scopes
//! - Multi-subproject enumeration via `settings.gradle(.kts)`
//!   `include(...)` lines
//!
//! Deferred to Phase 5b follow-on:
//! - Version catalog resolution (`libs.foo.bar` → coord lookup via
//!   `gradle/libs.versions.toml`); the m122 kotlin_dsl reader has
//!   the resolver — this milestone will delegate once the
//!   visibility is promoted
//! - Platform BOM detection (`platform(...)`) — emits an annotation
//!   rather than a component
//! - Complex Groovy expressions (helper methods, dynamic `include`,
//!   Kotlin lambda-based dep declarations) — warn-and-skip
//!
//! See contracts/gradle-static-parser.md for the full contract.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::ladder::{EdgeScope, GradleResolvedGraph};
use super::tier::GradleResolutionTier;
use crate::scan_fs::package_db::PackageDbEntry;
use waybill_common::types::purl::{encode_purl_segment, Purl};

/// Lightweight direct-dep coordinate representation.
///
/// Shared with the US2 cache reader (which converts via
/// `impl From<&DirectCoord> for MavenCoord`) as its seed input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct DirectCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

/// Failure modes for the static parser.
#[derive(Debug)]
pub enum GradleStaticError {
    /// No `build.gradle` / `build.gradle.kts` files found under the
    /// project tree — the ladder degrades to lockfile-only.
    NoSourceFiles,
}

/// Extract direct-dep coordinates from `build.gradle(.kts)` in the
/// given project directory (US2 seed input — coords only, no scope).
///
/// Covers ONLY the direct-string-coord patterns (skips version
/// catalog references, platform BOMs, project refs). Enough to seed
/// US2's cache lookup.
pub(super) fn extract_direct_coords(project_dir: &Path) -> Vec<DirectCoord> {
    extract_direct_coords_with_scope(project_dir)
        .into_iter()
        .map(|(coord, _scope)| coord)
        .collect()
}

/// Extract direct-dep coordinates paired with their EdgeScope (US3).
///
/// The scope is derived from the configuration name that declared
/// the dependency (per contracts/gradle-static-parser.md §step 8):
///
/// | Configuration | Scope |
/// |---|---|
/// | `implementation`, `api`, `runtimeOnly`, `compileOnly` | `Runtime` |
/// | `testImplementation`, `testRuntimeOnly`, `testCompileOnly` | `Test` |
/// | `annotationProcessor`, `kapt`, `ksp` | `Buildscript` |
///
/// **DSL scoping.** This function reads ONLY `build.gradle` (Groovy
/// DSL). `build.gradle.kts` (Kotlin DSL) is delegated to the m122
/// `kotlin_dsl` reader, which understands KMP source-set provenance
/// (`waybill:kmp-source-set`), workspace-root synthesis, and version
/// catalog resolution. Emitting Kotlin DSL deps from BOTH readers
/// would cause dedupe-order-dependent annotation loss (see
/// `us3_kmp_workspace_root_and_kmp_source_set_provenance_present`
/// test in `scan_kmp_polyglot.rs`).
pub(super) fn extract_direct_coords_with_scope(
    project_dir: &Path,
) -> Vec<(DirectCoord, EdgeScope)> {
    let mut out: Vec<(DirectCoord, EdgeScope)> = Vec::new();
    // Groovy DSL only — Kotlin DSL is m122's responsibility.
    let path = project_dir.join("build.gradle");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return out;
    };
    for cap in groovy_string_coord_re().captures_iter(&content) {
        let config = &cap[1];
        let coord_str = &cap[2];
        if let Some(coord) = parse_coord_str(coord_str) {
            out.push((coord, config_to_scope(config)));
        }
    }
    out
}

/// Map a Gradle configuration name to the corresponding EdgeScope.
///
/// Unknown configs default to `Runtime` (defensive — new configs
/// added by the Gradle ecosystem are more often runtime-like than
/// test-like).
fn config_to_scope(config: &str) -> EdgeScope {
    match config {
        "testImplementation" | "testRuntimeOnly" | "testCompileOnly" => EdgeScope::Test,
        "annotationProcessor" | "kapt" | "ksp" => EdgeScope::Buildscript,
        _ => EdgeScope::Runtime,
    }
}

fn parse_coord_str(s: &str) -> Option<DirectCoord> {
    let mut parts = s.splitn(3, ':');
    let group = parts.next()?.trim().to_string();
    let artifact = parts.next()?.trim().to_string();
    let version = parts.next()?.trim().to_string();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    Some(DirectCoord { group, artifact, version })
}

// Groovy: `implementation 'g:a:v'` or `implementation "g:a:v"`.
// Captures: [1] = config name, [2] = coord string.
fn groovy_string_coord_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*(implementation|api|runtimeOnly|compileOnly|testImplementation|testRuntimeOnly|testCompileOnly|annotationProcessor|kapt|ksp)\s+['"]([^'"]+)['"]"#).expect("valid regex")
    })
}

// C150: `implementation platform('g:a:v')` / `api(platform("g:a:v"))` etc.
// BOM imports don't produce a component — they're constraint-only
// declarations. But we DO surface them as a component-scope
// annotation so operators can trace which BOMs govern the emitted
// dep set.
//
// Captures: [1] = coord string (`g:a:v`).
fn groovy_platform_import_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*(?:implementation|api|runtimeOnly|compileOnly|testImplementation|testRuntimeOnly|testCompileOnly|annotationProcessor|kapt|ksp)\s*\(?\s*(?:enforcedPlatform|platform)\s*\(\s*['"]([^'"]+)['"]\s*\)"#).expect("valid regex")
    })
}

/// Extract the `platform(...)` / `enforcedPlatform(...)` BOM coord
/// imports declared in the project's `build.gradle`. These are
/// version-constraint declarations that don't contribute a component
/// but DO appear as C150 per-component annotations on every US3
/// component emitted from this project.
///
/// Returns sorted, deduplicated `g:a:v` strings. Empty when no BOM
/// import is declared. Groovy DSL only (Kotlin DSL is m122's turf).
pub(super) fn extract_platform_imports(project_dir: &Path) -> Vec<String> {
    let path = project_dir.join("build.gradle");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for cap in groovy_platform_import_re().captures_iter(&content) {
        let coord_str = &cap[1];
        if parse_coord_str(coord_str).is_some() {
            out.insert(coord_str.to_string());
        }
    }
    out.into_iter().collect()
}

// Kotlin: `implementation("g:a:v")`.
// Captures: [1] = config name, [2] = coord string.
fn kotlin_string_coord_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*(implementation|api|runtimeOnly|compileOnly|testImplementation|testRuntimeOnly|testCompileOnly|annotationProcessor|kapt|ksp)\s*\(\s*"([^"]+)"\s*\)"#).expect("valid regex")
    })
}

/// Enumerate subprojects declared in `settings.gradle(.kts)`
/// `include(...)` lines.
///
/// Recognized patterns:
/// - Groovy: `include 'app', 'core'`, `include ":app"`
/// - Kotlin: `include("app", "core")`, `include(":app")`
///
/// Returns absolute paths of subproject directories. Empty when no
/// `settings.gradle(.kts)` present or `include(...)` has no
/// recognized args.
pub(super) fn enumerate_subprojects_static(project_dir: &Path) -> Vec<PathBuf> {
    let mut names: Vec<String> = Vec::new();
    for name in ["settings.gradle", "settings.gradle.kts"] {
        let path = project_dir.join(name);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for cap in include_line_re().captures_iter(&content) {
            let args = &cap[1];
            for token in args.split(',') {
                let raw = token.trim();
                // Strip surrounding quotes.
                let unquoted = raw
                    .trim_start_matches('\'')
                    .trim_end_matches('\'')
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .trim_start_matches(':')
                    .trim();
                if !unquoted.is_empty() && !unquoted.contains(char::is_whitespace) {
                    names.push(unquoted.to_string());
                }
            }
        }
    }
    names
        .into_iter()
        .map(|n| project_dir.join(n.replace(':', "/")))
        .filter(|p| p.is_dir())
        .collect()
}

// Matches `include(...)` or `include ...` — captures the arg tuple/list.
fn include_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Groovy: `include 'a', 'b'` or Kotlin: `include("a", "b")`.
        // Non-greedy inner match to stay on one line.
        Regex::new(r#"(?m)^\s*include\s*\(?\s*(['"][^\n]+?)\s*\)?\s*$"#)
            .expect("valid regex")
    })
}

/// Top-level US3 entry point — parses ONLY the project_dir's own
/// `build.gradle(.kts)`.
///
/// The multi-subproject case is handled by the outer walker
/// (`gradle::read` visits each subproject directory in turn), NOT
/// by recursing into subprojects here. The `enumerate_subprojects_static`
/// function is kept for the future US4 per-subproject annotation
/// emitter — it isn't invoked from this entry point.
///
/// Returns `Err(NoSourceFiles)` when no `build.gradle(.kts)` is
/// found in `project_dir` so the ladder falls through to
/// lockfile-only. `Ok(graph)` with an empty component list is a
/// valid outcome (build file exists but declares no direct deps)
/// — the tier annotation still emits `static`.
pub fn resolve_via_static_parse(
    project_dir: &Path,
) -> Result<GradleResolvedGraph, GradleStaticError> {
    // Groovy DSL only. Kotlin `.kts` files are delegated to the m122
    // `kotlin_dsl` reader (see `extract_direct_coords_with_scope`'s
    // docstring for the rationale).
    let has_groovy_build = project_dir.join("build.gradle").is_file();
    if !has_groovy_build {
        return Err(GradleStaticError::NoSourceFiles);
    }

    let pairs = extract_direct_coords_with_scope(project_dir);
    let source_path = project_dir.to_string_lossy().to_string();
    // C150: sorted, comma-joined `g:a:v` list of BOM imports for this
    // project. Empty (→ None) when no platform imports.
    let platform_imports = extract_platform_imports(project_dir);
    let platform_import_value = if platform_imports.is_empty() {
        None
    } else {
        Some(platform_imports.join(","))
    };
    let components: Vec<PackageDbEntry> = pairs
        .into_iter()
        .filter_map(|(coord, scope)| build_entry(&coord, &source_path, scope))
        .map(|mut entry| {
            if let Some(ref v) = platform_import_value {
                entry.extra_annotations.insert(
                    "waybill:gradle-platform-import".to_string(),
                    serde_json::Value::String(v.clone()),
                );
            }
            entry
        })
        .collect();

    // US3 emits COMPONENTS ONLY — no transitive edges (that's the
    // domain of US1 subprocess + US2 cache reader). `edges` is
    // intentionally empty.
    Ok(GradleResolvedGraph {
        components,
        edges: Vec::new(),
        tier: GradleResolutionTier::Static,
        fallback_history: Vec::new(),
    })
}

/// Build a `PackageDbEntry` from a resolved Maven coord.
///
/// Field set mirrors the m106 lockfile reader + m235 US1/US2 entry
/// construction. Scope determines the `LifecycleScope` value on the
/// emitted component; scan_fs's downstream emission path maps this
/// to CDX `scope` and SPDX relationship types.
fn build_entry(
    coord: &DirectCoord,
    source_path: &str,
    scope: EdgeScope,
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
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(scope.into()),
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
        // "design" tier — US3 static-parse output is manifest-only
        // (no resolution took place); mirrors m106 lockfile-tier
        // semantics for the design/source distinction.
        sbom_tier: Some("design".to_string()),
        shade_relocation: None,
        // Milestone 236 (C151): US3 static parser emits design-tier
        // when the m235 US2 cache reader didn't hit for these seeds.
        extra_annotations: {
            let mut m: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            m.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(
                    "declared in build.gradle; US2 cache reader had no matching seed".to_string(),
                ),
            );
            m
        },
        binary_role: None,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn extract_groovy_direct_string_coord() {
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "build.gradle",
            r#"
dependencies {
    implementation 'com.example:foo:1.0.0'
    api "com.example:bar:2.0.0"
    testImplementation 'com.example:baz:3.0.0'
}
"#,
        );
        let coords = extract_direct_coords(td.path());
        assert_eq!(coords.len(), 3);
        assert!(coords.iter().any(|c| c.artifact == "foo" && c.version == "1.0.0"));
        assert!(coords.iter().any(|c| c.artifact == "bar" && c.version == "2.0.0"));
        assert!(coords.iter().any(|c| c.artifact == "baz" && c.version == "3.0.0"));
    }

    #[test]
    fn kotlin_dsl_delegated_to_m122_reader() {
        // US3 static parser is Groovy-DSL-only. Kotlin `.kts` files
        // are delegated to the m122 `kotlin_dsl` reader. This test
        // verifies that a project with ONLY a `build.gradle.kts`
        // (no Groovy sibling) returns zero coords from US3 — m122
        // handles those separately when `--include-declared-deps` is
        // set. See `extract_direct_coords_with_scope` docstring.
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "build.gradle.kts",
            r#"
dependencies {
    implementation("com.example:foo:1.0.0")
    testImplementation("com.example:bar:2.0.0")
}
"#,
        );
        let coords = extract_direct_coords(td.path());
        assert!(
            coords.is_empty(),
            ".kts extraction is delegated to m122; got: {coords:?}"
        );
    }

    #[test]
    fn empty_when_no_build_files() {
        let td = TempDir::new().unwrap();
        assert!(extract_direct_coords(td.path()).is_empty());
    }

    #[test]
    fn config_to_scope_maps_test_configs_to_test_scope() {
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "build.gradle",
            r#"
dependencies {
    implementation 'com.example:runtime-dep:1.0.0'
    testImplementation 'com.example:test-dep:2.0.0'
    annotationProcessor 'com.example:proc-dep:3.0.0'
}
"#,
        );
        let pairs = extract_direct_coords_with_scope(td.path());
        assert_eq!(pairs.len(), 3);
        let runtime = pairs
            .iter()
            .find(|(c, _)| c.artifact == "runtime-dep")
            .expect("runtime dep");
        assert_eq!(runtime.1, EdgeScope::Runtime);
        let test = pairs
            .iter()
            .find(|(c, _)| c.artifact == "test-dep")
            .expect("test dep");
        assert_eq!(test.1, EdgeScope::Test);
        let proc = pairs
            .iter()
            .find(|(c, _)| c.artifact == "proc-dep")
            .expect("proc dep");
        assert_eq!(proc.1, EdgeScope::Buildscript);
    }

    #[test]
    fn enumerate_subprojects_from_kotlin_include() {
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "settings.gradle.kts",
            r#"
rootProject.name = "myapp"
include("app")
include(":core")
"#,
        );
        // Create the subproject dirs so is_dir() filter passes.
        std::fs::create_dir(td.path().join("app")).unwrap();
        std::fs::create_dir(td.path().join("core")).unwrap();
        let subs = enumerate_subprojects_static(td.path());
        assert_eq!(subs.len(), 2);
        assert!(subs.iter().any(|p| p.ends_with("app")));
        assert!(subs.iter().any(|p| p.ends_with("core")));
    }

    #[test]
    fn resolve_via_static_parse_single_project() {
        let td = TempDir::new().unwrap();
        write_file(
            td.path(),
            "build.gradle",
            r#"
plugins { id 'java' }
dependencies {
    implementation 'com.example:root-dep:1.0.0'
    testImplementation 'com.example:root-test-dep:2.0.0'
}
"#,
        );
        let graph = resolve_via_static_parse(td.path()).expect("single-project static parse");
        assert_eq!(graph.tier, GradleResolutionTier::Static);
        assert!(graph.edges.is_empty(), "US3 emits no transitive edges");
        assert_eq!(graph.components.len(), 2);
    }

    #[test]
    fn resolve_via_static_parse_no_source_files_errors() {
        let td = TempDir::new().unwrap();
        assert!(matches!(
            resolve_via_static_parse(td.path()),
            Err(GradleStaticError::NoSourceFiles)
        ));
    }

    #[test]
    fn extract_platform_imports_matches_platform_and_enforced_platform() {
        let td = TempDir::new().unwrap();
        let build = td.path().join("build.gradle");
        std::fs::write(
            &build,
            r#"
plugins { id 'java' }
dependencies {
    implementation platform('com.example.waybillfixture:bom-a:1.0.0')
    implementation platform("com.example.waybillfixture:bom-b:2.0.0")
    api(enforcedPlatform('com.example.waybillfixture:bom-c:3.0.0'))
    implementation 'com.example.waybillfixture:app-dep:4.0.0'
}
"#,
        )
        .unwrap();
        let mut got = extract_platform_imports(td.path());
        got.sort();
        assert_eq!(
            got,
            vec![
                "com.example.waybillfixture:bom-a:1.0.0".to_string(),
                "com.example.waybillfixture:bom-b:2.0.0".to_string(),
                "com.example.waybillfixture:bom-c:3.0.0".to_string(),
            ],
            "expected 3 BOM imports from platform() + enforcedPlatform(); regular dep NOT included"
        );
    }

    #[test]
    fn m236_gradle_static_design_tier_carries_unresolved_reason() {
        // Milestone 236 (C151): every gradle US3 static component
        // carries the reason string.
        let td = TempDir::new().unwrap();
        std::fs::write(
            td.path().join("build.gradle"),
            r#"
plugins { id 'java' }
dependencies {
    implementation 'com.example.waybillfixture:app-dep:1.0.0'
}
"#,
        )
        .unwrap();
        let graph = resolve_via_static_parse(td.path()).expect("static parse");
        assert!(!graph.components.is_empty());
        for entry in &graph.components {
            assert_eq!(entry.sbom_tier.as_deref(), Some("design"));
            let reason = entry
                .extra_annotations
                .get("waybill:unresolved-reason")
                .expect("C151 annotation present");
            assert_eq!(
                reason.as_str().unwrap(),
                "declared in build.gradle; US2 cache reader had no matching seed",
            );
        }
    }

    #[test]
    fn resolve_via_static_parse_tags_components_with_platform_import() {
        let td = TempDir::new().unwrap();
        std::fs::write(
            td.path().join("build.gradle"),
            r#"
plugins { id 'java' }
dependencies {
    implementation platform('com.example.waybillfixture:bom-parent:1.0.0')
    implementation 'com.example.waybillfixture:app-dep:2.0.0'
}
"#,
        )
        .unwrap();
        let graph = resolve_via_static_parse(td.path()).expect("static parse");
        assert_eq!(graph.components.len(), 1, "BOM MUST NOT appear as a component");
        let entry = &graph.components[0];
        let annotation = entry
            .extra_annotations
            .get("waybill:gradle-platform-import")
            .expect("C150 annotation present");
        assert_eq!(
            annotation.as_str().unwrap(),
            "com.example.waybillfixture:bom-parent:1.0.0",
        );
    }
}
