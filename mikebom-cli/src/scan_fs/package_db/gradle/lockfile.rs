//! Parser for `gradle.lockfile` / `buildscript-gradle.lockfile` files.
//!
//! Format (per Gradle dependency-locking docs, stable since 6.0):
//!
//! ```text
//! # This is a Gradle generated file for dependency locking.
//! # Manual edits can break the build and are not advised.
//! # This file is expected to be part of source control.
//! com.google.guava:guava:32.1.3-jre=compileClasspath,runtimeClasspath,testCompileClasspath,testRuntimeClasspath
//! org.jetbrains.kotlin:kotlin-stdlib:1.9.20=compileClasspath,runtimeClasspath
//! empty=annotationProcessor,kapt
//! ```
//!
//! - `#`-prefixed lines: skipped (file header).
//! - `empty=...` line: skipped (Gradle's marker for "configurations
//!   resolved with zero deps"; not a real coord).
//! - All other lines: `<group>:<name>:<version>=<config1>,<config2>,...`.
//!
//! PURLs emitted as `pkg:maven/<group>/<name>@<version>` — same scheme as
//! the existing `maven.rs` reader, so downstream enrichment via deps.dev
//! applies without changes.
//!
//! Filename selects lifecycle scope: `gradle.lockfile` → runtime (no
//! scope); `buildscript-gradle.lockfile` → `LifecycleScope::Build`.

use std::path::Path;

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::super::PackageDbEntry;

const BUILDSCRIPT_FILENAME: &str = "buildscript-gradle.lockfile";

/// Milestone 184 US2 — detect the "compile-only shape" per lockfile
/// entry. A Gradle dep is classified as `LifecycleScope::Optional` iff
/// it appears on any `*compileClasspath` configuration AND is absent
/// from any `*runtimeClasspath` configuration.
///
/// Per Gradle's naming convention, the main source set uses lowercase
/// (`compileClasspath` / `runtimeClasspath`) while every other source
/// set concatenates with a capital `C`/`R` — `testCompileClasspath`,
/// `debugCompileClasspath` (Android), `main_CompileClasspath`
/// (multi-source-set custom names), etc. Both spellings must be
/// recognized. The check on each config name is: exact match on the
/// lowercase main-set spelling OR suffix match on the capitalized
/// compound-set spelling.
///
/// Per research.md Decision 3 this covers:
///   * `compileClasspath` (main source set)
///   * `testCompileClasspath` (test source set)
///   * `<sourceSet>CompileClasspath` (multi-source-set projects —
///     Kotlin `main`/`test`, Android `debug`/`release`, etc.)
///
/// The check runs on the raw `<config1>,<config2>,...` string that
/// follows the `=` in a lockfile line. Falls out to `false` for empty
/// input.
fn is_compile_only_shape(configs: &str) -> bool {
    let items: Vec<&str> = configs.split(',').map(|s| s.trim()).collect();
    let is_compile_config =
        |c: &&str| *c == "compileClasspath" || c.ends_with("CompileClasspath");
    let is_runtime_config =
        |c: &&str| *c == "runtimeClasspath" || c.ends_with("RuntimeClasspath");
    let has_compile = items.iter().any(is_compile_config);
    let has_runtime = items.iter().any(is_runtime_config);
    has_compile && !has_runtime
}

/// Parse a single Gradle lockfile and return one `PackageDbEntry` per
/// resolved coordinate. Returns empty on read failure or when the file
/// contains no resolvable entries (e.g. only the header lines).
pub(super) fn read_gradle_lockfile(path: &Path) -> Vec<PackageDbEntry> {
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read Gradle lockfile (skipping; FR-015)"
            );
            return Vec::new();
        }
    };
    let source_path = path.to_string_lossy().to_string();
    let is_buildscript = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n == BUILDSCRIPT_FILENAME)
        .unwrap_or(false);

    let mut out = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("empty=") {
            continue;
        }
        let Some((coord, configs)) = line.split_once('=') else {
            tracing::warn!(
                path = %path.display(),
                line = %raw_line,
                "Gradle lockfile line missing `=` separator; skipping"
            );
            continue;
        };
        let segments: Vec<&str> = coord.split(':').collect();
        // Need at least 3 segments. For paths with extra `:` chars (rare
        // but legal — Gradle docs allow it for classifier-tagged coords)
        // join the leading parts as group and take the last two as
        // name + version.
        if segments.len() < 3 {
            tracing::warn!(
                path = %path.display(),
                line = %raw_line,
                "Gradle lockfile entry has fewer than 3 colon-separated segments; skipping"
            );
            continue;
        }
        let version = segments[segments.len() - 1].trim();
        let name = segments[segments.len() - 2].trim();
        let group = segments[..segments.len() - 2].join(":");
        let group = group.trim();
        if group.is_empty() || name.is_empty() || version.is_empty() {
            tracing::warn!(
                path = %path.display(),
                line = %raw_line,
                "Gradle lockfile entry has empty group/name/version; skipping"
            );
            continue;
        }

        let Some(purl) = build_maven_purl(group, name, version) else {
            tracing::warn!(
                path = %path.display(),
                line = %raw_line,
                "Gradle lockfile coord produced invalid PURL; skipping"
            );
            continue;
        };

        let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
            Default::default();
        let configs_value = configs.trim();
        if !configs_value.is_empty() {
            extra_annotations.insert(
                "mikebom:gradle-configurations".to_string(),
                serde_json::Value::String(configs_value.to_string()),
            );
        }

        // Milestone 184 US2 — per-entry classification. Buildscript
        // classification (m106) wins over compile-only shape per
        // Decision 2 buildscript-wins-over-optional. Non-buildscript
        // entries with the compile-only shape (compileClasspath
        // present + runtimeClasspath absent, suffix-matched per
        // Decision 3) classify as `LifecycleScope::Optional` +
        // `mikebom:optional-derivation = "gradle-compile-only"`.
        let lifecycle_scope = if is_buildscript {
            Some(LifecycleScope::Build)
        } else if is_compile_only_shape(configs_value) {
            extra_annotations.insert(
                "mikebom:optional-derivation".to_string(),
                serde_json::Value::String("gradle-compile-only".to_string()),
            );
            Some(LifecycleScope::Optional)
        } else {
            None
        };

        out.push(PackageDbEntry {
            build_inclusion: None,
            purl,
            name: name.to_string(),
            version: version.to_string(),
            arch: None,
            source_path: source_path.clone(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope,
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
            sbom_tier: Some("source".to_string()),
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
        });
    }
    out
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

    const HEADER: &str = "\
# This is a Gradle generated file for dependency locking.
# Manual edits can break the build and are not advised.
# This file is expected to be part of source control.
";

    fn write_lockfile(tmp: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = tmp.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn emits_basic_maven_components() {
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
com.google.guava:guava:32.1.3-jre=compileClasspath,runtimeClasspath
org.jetbrains.kotlin:kotlin-stdlib:1.9.20=compileClasspath,runtimeClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:maven/com.google.guava/guava@32.1.3-jre"
        );
        assert_eq!(entries[0].name, "guava");
        assert_eq!(entries[0].version, "32.1.3-jre");
        assert!(entries[0].lifecycle_scope.is_none());
        assert_eq!(
            entries[1].purl.as_str(),
            "pkg:maven/org.jetbrains.kotlin/kotlin-stdlib@1.9.20"
        );
        assert!(entries[1].lifecycle_scope.is_none());
    }

    #[test]
    fn buildscript_tagged_build_lifecycle_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
org.springframework.boot:org.springframework.boot.gradle.plugin:3.2.0=classpath
"
        );
        let path = write_lockfile(tmp.path(), "buildscript-gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Build)
        ));
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:maven/org.springframework.boot/org.springframework.boot.gradle.plugin@3.2.0"
        );
    }

    #[test]
    fn header_lines_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Only the header — should yield zero entries, not parse-fail.
        let path = write_lockfile(tmp.path(), "gradle.lockfile", HEADER);
        let entries = read_gradle_lockfile(&path);
        assert!(entries.is_empty());
    }

    #[test]
    fn empty_configs_line_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // `empty=...` lines indicate a resolved configuration with zero
        // deps — must not be misparsed into a `pkg:maven/empty` coord.
        let body = format!(
            "{HEADER}\
empty=annotationProcessor,kapt
com.google.guava:guava:32.1.3-jre=runtimeClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "guava");
    }

    #[test]
    fn configurations_recorded_in_annotation() {
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
com.google.guava:guava:32.1.3-jre=compileClasspath,runtimeClasspath,testCompileClasspath,testRuntimeClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        let configs = entries[0]
            .extra_annotations
            .get("mikebom:gradle-configurations")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(
            configs,
            "compileClasspath,runtimeClasspath,testCompileClasspath,testRuntimeClasspath"
        );
    }

    #[test]
    fn malformed_lines_warned_and_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Mixed valid + malformed entries; the valid one must still emit.
        let body = format!(
            "{HEADER}\
not-a-real-coord
group-only:=runtimeClasspath
com.google.guava:guava:32.1.3-jre=runtimeClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "guava");
    }

    // ── Milestone 184 US2 — is_compile_only_shape helper tests ──────

    #[test]
    fn is_compile_only_shape_detects_compile_only() {
        assert!(is_compile_only_shape(
            "compileClasspath,testCompileClasspath"
        ));
    }

    #[test]
    fn is_compile_only_shape_rejects_compile_and_runtime() {
        // Presence on BOTH classpaths → transitive dep, not compile-only.
        assert!(!is_compile_only_shape("compileClasspath,runtimeClasspath"));
    }

    #[test]
    fn is_compile_only_shape_rejects_runtime_only() {
        // Runtime-only shape (Gradle `runtimeOnly` config) is semantic
        // Runtime, not Optional.
        assert!(!is_compile_only_shape(
            "runtimeClasspath,testRuntimeClasspath"
        ));
    }

    #[test]
    fn is_compile_only_shape_detects_test_compile_only() {
        // Suffix-match per Decision 3 — testCompileClasspath alone
        // still counts as compile-only.
        assert!(is_compile_only_shape("testCompileClasspath"));
    }

    #[test]
    fn is_compile_only_shape_detects_source_set_variants() {
        // Custom source sets use Gradle's `<name>CompileClasspath`
        // CamelCase compound convention (Android debug/release, Kotlin
        // main/test, user-declared sets like `integrationTest`).
        // Suffix-match on `"CompileClasspath"` covers all of them.
        assert!(is_compile_only_shape(
            "debugCompileClasspath,releaseCompileClasspath"
        ));
        assert!(is_compile_only_shape("integrationTestCompileClasspath"));
        // Mix compile-only classpaths with annotation processor —
        // still classifies (Edge Cases: annotation-processor + compile
        // is compile-only in m184's initial delivery).
        assert!(is_compile_only_shape("annotationProcessor,compileClasspath"));
        // A source-set with BOTH compile + runtime → NOT compile-only.
        assert!(!is_compile_only_shape(
            "debugCompileClasspath,debugRuntimeClasspath"
        ));
    }

    // ── Milestone 184 US2 — classifier integration tests ────────────

    #[test]
    fn read_gradle_lockfile_compile_only_classifies_as_optional() {
        // US2 acceptance 1+2 end-to-end. The dep appears on compile-
        // only classpaths → LifecycleScope::Optional + derivation
        // annotation. The existing `mikebom:gradle-configurations`
        // annotation is preserved.
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
com.example:lombok:1.18.30=compileClasspath,testCompileClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "lombok");
        assert_eq!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Optional),
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:optional-derivation"),
            Some(&serde_json::Value::String(
                "gradle-compile-only".to_string()
            )),
        );
        // Existing `mikebom:gradle-configurations` annotation preserved.
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:gradle-configurations"),
            Some(&serde_json::Value::String(
                "compileClasspath,testCompileClasspath".to_string()
            )),
        );
    }

    #[test]
    fn read_gradle_lockfile_buildscript_compile_only_stays_build() {
        // US2 acceptance 5 + Decision 2 buildscript-wins pin: same
        // compile-only shape but the file is `buildscript-gradle.lockfile`
        // — classification stays `Build`, NO derivation annotation.
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
com.example:build-tool:1.0=compileClasspath,testCompileClasspath
"
        );
        let path = write_lockfile(tmp.path(), "buildscript-gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lifecycle_scope, Some(LifecycleScope::Build));
        assert!(!entries[0]
            .extra_annotations
            .contains_key("mikebom:optional-derivation"));
    }

    #[test]
    fn m184_optional_classified_entry_is_filtered_by_include_dev_false() {
        // FR-008 boundary pin (per /speckit-analyze R2 remediation for U1):
        // an m184-Optional-classified Gradle entry MUST expose
        // `LifecycleScope::Optional.is_non_runtime() == true` so the
        // emitter-layer `--include-dev=false` filter (m179 extension)
        // drops it alongside Test/Dev/Build entries.
        //
        // This test verifies the boundary between the classifier
        // (gradle/lockfile.rs — reader-level) and the emitter filter
        // (m179 is_non_runtime() extension — emitter-level). If m179's
        // is_non_runtime() were ever changed to exclude Optional, this
        // test would fail loudly.
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
com.example:lombok:1.18.30=compileClasspath,testCompileClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        let scope = entries[0].lifecycle_scope.expect("classified");
        assert_eq!(scope, LifecycleScope::Optional);
        assert!(
            scope.is_non_runtime(),
            "LifecycleScope::Optional must return true from \
             is_non_runtime() so the emitter's --include-dev=false \
             filter drops m184-classified entries alongside Test/Dev/Build"
        );
    }

    #[test]
    fn read_gradle_lockfile_runtime_stays_none() {
        // Regression pin: pre-m184 behavior for a non-buildscript entry
        // with the transitive shape (compile + runtime both present)
        // MUST be byte-identical — lifecycle_scope stays None, NO
        // derivation annotation.
        let tmp = tempfile::tempdir().unwrap();
        let body = format!(
            "{HEADER}\
com.example:transitive-dep:1.0=compileClasspath,runtimeClasspath
"
        );
        let path = write_lockfile(tmp.path(), "gradle.lockfile", &body);
        let entries = read_gradle_lockfile(&path);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].lifecycle_scope.is_none());
        assert!(!entries[0]
            .extra_annotations
            .contains_key("mikebom:optional-derivation"));
    }
}
