//! Kotlin DSL `build.gradle.kts` dependency extractor.
//!
//! Surface-syntax regex extraction; not a full Kotlin parser (Constitution
//! Principle I + Strict Boundary 3 prohibit the dependencies). Three regex
//! shapes per contracts/kotlin-dsl-extraction.md § "`build.gradle.kts` dep
//! declaration surface syntax":
//!
//! 1. Fully-qualified string-literal GAV:
//!    `implementation("com.squareup.okhttp3:okhttp:4.12.0")`
//! 2. Catalog-alias: `implementation(libs.okhttp)`
//! 3. Named-args: `implementation(group = "g", name = "n", version = "v")`
//!
//! Source-set tracking via brace-depth counting tags each declaration
//! with the originating KMP source-set name (`commonMain`, `jvmMain`, …)
//! or `None` for top-level `dependencies { ... }` blocks.

use std::path::Path;
use std::sync::LazyLock;

use waybill_common::resolution::LifecycleScope;
use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;
use serde_json::Value as JsonValue;

use super::super::PackageDbEntry;
use super::version_catalog::{self, VersionCatalog};
use super::KmpSourceSetTracker;

#[derive(Debug, Clone)]
pub(super) struct KotlinDslEntry {
    /// Dep configuration name as written (`implementation`, `api`,
    /// `testImplementation`, etc.).
    pub(super) config: String,
    pub(super) raw: KotlinDepRaw,
    pub(super) source_set: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) enum KotlinDepRaw {
    Gav { group: String, name: String, version: String },
    PartialGav { group: String, name: String },
    CatalogAlias { alias: String },
}

static GAV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^\s*(?P<config>implementation|api|testImplementation|androidTestImplementation|debugImplementation|releaseImplementation|kapt|annotationProcessor|ksp|runtimeOnly|compileOnly|testRuntimeOnly|testCompileOnly)\s*\(\s*"(?P<gav>[^"]+)"\s*\)"#,
    )
    .expect("GAV regex compiles")
});

static CATALOG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^\s*(?P<config>implementation|api|testImplementation|androidTestImplementation|debugImplementation|releaseImplementation|kapt|annotationProcessor|ksp|runtimeOnly|compileOnly|testRuntimeOnly|testCompileOnly)\s*\(\s*libs\.(?P<alias>[\w\.]+)\s*\)"#,
    )
    .expect("catalog regex compiles")
});

static NAMED_ARGS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^\s*(?P<config>implementation|api|testImplementation|androidTestImplementation|debugImplementation|releaseImplementation|kapt|annotationProcessor|ksp|runtimeOnly|compileOnly|testRuntimeOnly|testCompileOnly)\s*\(\s*group\s*=\s*"(?P<group>[^"]+)"\s*,\s*name\s*=\s*"(?P<name>[^"]+)"\s*,\s*version\s*=\s*"(?P<version>[^"]+)"\s*\)"#,
    )
    .expect("named-args regex compiles")
});

/// Source-set name as it appears immediately before `{ dependencies {` —
/// e.g., `commonMain`, `jvmMain`, `iosX64Main`. The regex captures the
/// identifier preceding a `{` brace; the brace-depth walker then notes
/// that any `dependencies {` block inside it inherits this name.
static SOURCE_SET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?:\.\s*\w+\s*)?\{"#)
        .expect("source-set regex compiles")
});

/// Walks a `build.gradle.kts` file content line-by-line and returns
/// every dep declaration that matches one of the three surface shapes.
/// Source-set tracking via brace-depth counting populates the
/// `source_set` field on each entry; declarations inside a top-level
/// `dependencies { ... }` block have `source_set = None`.
pub(super) fn extract_deps(content: &str) -> Vec<KotlinDslEntry> {
    let mut out: Vec<KotlinDslEntry> = Vec::new();
    let mut brace_stack: Vec<Option<String>> = Vec::new();
    // Track which depth level corresponds to `dependencies { ... }`
    // inside a source-set block. We push the source-set name onto the
    // brace_stack at the matching depth so when we encounter a dep
    // declaration line, the LAST source-set in the stack wins.

    for line in content.lines() {
        // Match dep declarations FIRST so the regexes capture before
        // brace-depth tracking advances. The brace position for `(...)`
        // doesn't affect block-level `{}` tracking.
        let active_source_set = brace_stack
            .iter()
            .rev()
            .find_map(|opt| opt.clone());

        if let Some(c) = GAV_RE.captures(line) {
            if let (Some(config), Some(gav)) = (c.name("config"), c.name("gav")) {
                if let Some(raw) = parse_gav_string(gav.as_str()) {
                    out.push(KotlinDslEntry {
                        config: config.as_str().to_string(),
                        raw,
                        source_set: active_source_set.clone(),
                    });
                }
            }
        } else if let Some(c) = CATALOG_RE.captures(line) {
            if let (Some(config), Some(alias)) = (c.name("config"), c.name("alias")) {
                out.push(KotlinDslEntry {
                    config: config.as_str().to_string(),
                    raw: KotlinDepRaw::CatalogAlias {
                        alias: alias.as_str().to_string(),
                    },
                    source_set: active_source_set.clone(),
                });
            }
        } else if let Some(c) = NAMED_ARGS_RE.captures(line) {
            if let (Some(config), Some(group), Some(name), Some(version)) = (
                c.name("config"),
                c.name("group"),
                c.name("name"),
                c.name("version"),
            ) {
                out.push(KotlinDslEntry {
                    config: config.as_str().to_string(),
                    raw: KotlinDepRaw::Gav {
                        group: group.as_str().to_string(),
                        name: name.as_str().to_string(),
                        version: version.as_str().to_string(),
                    },
                    source_set: active_source_set.clone(),
                });
            }
        }

        // Now advance brace-depth tracking based on this line's braces.
        for c in line.chars() {
            match c {
                '{' => {
                    // Look at what came before `{` on this line for a
                    // source-set name. We look only at the immediate
                    // prefix word.
                    let source_set_name =
                        SOURCE_SET_RE.captures(line).and_then(|c| {
                            c.name("name").map(|m| m.as_str().to_string())
                        });
                    // Only consider it a source-set if it's a known KMP
                    // identifier (heuristic: ends with `Main`/`Test`).
                    let is_kmp_source_set = source_set_name
                        .as_ref()
                        .is_some_and(|n| n.ends_with("Main") || n.ends_with("Test"));
                    brace_stack.push(if is_kmp_source_set {
                        source_set_name
                    } else {
                        None
                    });
                }
                '}' => {
                    brace_stack.pop();
                }
                _ => {}
            }
        }
    }
    out
}

fn parse_gav_string(s: &str) -> Option<KotlinDepRaw> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => Some(KotlinDepRaw::Gav {
            group: parts[0].to_string(),
            name: parts[1].to_string(),
            version: parts[2].to_string(),
        }),
        2 => Some(KotlinDepRaw::PartialGav {
            group: parts[0].to_string(),
            name: parts[1].to_string(),
        }),
        _ => None,
    }
}

/// Project Kotlin dep configuration → mikebom lifecycle scope per
/// contracts/kotlin-dsl-extraction.md § "Dep-configuration →
/// lifecycle-scope mapping".
fn config_to_lifecycle_scope(config: &str) -> Option<LifecycleScope> {
    match config {
        "testImplementation"
        | "androidTestImplementation"
        | "testRuntimeOnly"
        | "testCompileOnly" => Some(LifecycleScope::Test),
        "debugImplementation" | "releaseImplementation" => Some(LifecycleScope::Development),
        "kapt" | "annotationProcessor" | "ksp" => Some(LifecycleScope::Build),
        _ => None,
    }
}

/// Resolve each `KotlinDslEntry` against the optional catalog and emit
/// `PackageDbEntry` records. Records source-set hits on the tracker; the
/// caller stamps the merged JSON-array onto every duplicate after
/// `tracker.finalize()`.
pub(super) fn resolve_and_emit(
    entries: Vec<KotlinDslEntry>,
    catalog: Option<&VersionCatalog>,
    source_path: &Path,
    tracker: &mut KmpSourceSetTracker,
) -> Vec<PackageDbEntry> {
    let source_path_str = source_path.to_string_lossy().into_owned();
    let mut out: Vec<PackageDbEntry> = Vec::new();
    for entry in entries {
        let resolved = match &entry.raw {
            KotlinDepRaw::Gav { group, name, version } => Some(ResolvedDep {
                group: group.clone(),
                name: name.clone(),
                version: version.clone(),
            }),
            KotlinDepRaw::PartialGav { group, name } => {
                // Look up version via catalog where the library matches
                // group:name. v0.1 simple lookup: scan all catalog entries.
                catalog.and_then(|cat| {
                    cat.libraries
                        .values()
                        .find(|r| r.group == *group && r.name == *name)
                        .map(|r| ResolvedDep {
                            group: group.clone(),
                            name: name.clone(),
                            version: r.version.clone(),
                        })
                })
            }
            KotlinDepRaw::CatalogAlias { alias } => match catalog {
                Some(cat) => version_catalog::lookup(cat, alias).map(|r| ResolvedDep {
                    group: r.group.clone(),
                    name: r.name.clone(),
                    version: r.version.clone(),
                }),
                None => None,
            },
        };
        let Some(dep) = resolved else {
            tracing::warn!(
                source = %source_path_str,
                config = %entry.config,
                raw = ?entry.raw,
                "kotlin_dsl: dep declaration could not be resolved; dropping"
            );
            continue;
        };
        let Some(purl) = build_maven_purl(&dep.group, &dep.name, &dep.version) else {
            tracing::warn!(
                source = %source_path_str,
                group = %dep.group,
                name = %dep.name,
                version = %dep.version,
                "kotlin_dsl: PURL construction failed; dropping entry"
            );
            continue;
        };
        let mut extra_annotations: std::collections::BTreeMap<String, JsonValue> =
            Default::default();
        extra_annotations.insert(
            "mikebom:source-files".to_string(),
            JsonValue::String(source_path_str.clone()),
        );
        // Record the source-set hit BEFORE pushing the entry; the
        // tracker's finalize() runs later and stamps the merged array
        // onto every duplicate.
        if let Some(set) = entry.source_set.as_ref() {
            tracker.record(purl.clone(), set.clone());
        }
        let lifecycle_scope = config_to_lifecycle_scope(&entry.config);
        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: dep.name,
            version: dep.version,
            arch: None,
            source_path: source_path_str.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
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
            sbom_tier: Some("design".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
    }
    out
}

struct ResolvedDep {
    group: String,
    name: String,
    version: String,
}

fn build_maven_purl(group: &str, name: &str, version: &str) -> Option<Purl> {
    Purl::new(&format!(
        "pkg:maven/{}/{}@{}",
        encode_purl_segment(group),
        encode_purl_segment(name),
        encode_purl_segment(version),
    ))
    .ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extracts_string_literal_gav() {
        let entries = extract_deps(
            r#"dependencies {
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
}"#,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].config, "implementation");
        match &entries[0].raw {
            KotlinDepRaw::Gav { group, name, version } => {
                assert_eq!(group, "com.squareup.okhttp3");
                assert_eq!(name, "okhttp");
                assert_eq!(version, "4.12.0");
            }
            _ => panic!("expected Gav"),
        }
    }

    #[test]
    fn extracts_catalog_alias() {
        let entries = extract_deps(
            r#"dependencies {
    implementation(libs.okhttp)
    api(libs.ktor.client.cio)
}"#,
        );
        assert_eq!(entries.len(), 2);
        match &entries[0].raw {
            KotlinDepRaw::CatalogAlias { alias } => assert_eq!(alias, "okhttp"),
            _ => panic!("expected CatalogAlias"),
        }
        match &entries[1].raw {
            KotlinDepRaw::CatalogAlias { alias } => assert_eq!(alias, "ktor.client.cio"),
            _ => panic!("expected CatalogAlias"),
        }
    }

    #[test]
    fn extracts_named_args() {
        let entries = extract_deps(
            r#"dependencies {
    implementation(group = "g", name = "n", version = "1.0.0")
}"#,
        );
        assert_eq!(entries.len(), 1);
        match &entries[0].raw {
            KotlinDepRaw::Gav { group, name, version } => {
                assert_eq!(group, "g");
                assert_eq!(name, "n");
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("expected Gav"),
        }
    }

    #[test]
    fn assigns_kmp_source_set_for_nested_block() {
        let entries = extract_deps(
            r#"kotlin {
    sourceSets {
        commonMain {
            dependencies {
                implementation("io.example:lib:1.0.0")
            }
        }
        jvmMain {
            dependencies {
                implementation("io.example:jvm-only:2.0.0")
            }
        }
    }
}"#,
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source_set.as_deref(), Some("commonMain"));
        assert_eq!(entries[1].source_set.as_deref(), Some("jvmMain"));
    }

    #[test]
    fn top_level_dependencies_block_has_no_source_set() {
        let entries = extract_deps(
            r#"dependencies {
    implementation("io.example:lib:1.0.0")
}"#,
        );
        assert_eq!(entries.len(), 1);
        assert!(entries[0].source_set.is_none());
    }

    #[test]
    fn config_to_lifecycle_scope_maps_all_families() {
        assert!(config_to_lifecycle_scope("implementation").is_none());
        assert!(config_to_lifecycle_scope("api").is_none());
        assert_eq!(
            config_to_lifecycle_scope("testImplementation"),
            Some(LifecycleScope::Test)
        );
        assert_eq!(
            config_to_lifecycle_scope("androidTestImplementation"),
            Some(LifecycleScope::Test)
        );
        assert_eq!(
            config_to_lifecycle_scope("debugImplementation"),
            Some(LifecycleScope::Development)
        );
        assert_eq!(
            config_to_lifecycle_scope("kapt"),
            Some(LifecycleScope::Build)
        );
        assert_eq!(
            config_to_lifecycle_scope("ksp"),
            Some(LifecycleScope::Build)
        );
    }
}
