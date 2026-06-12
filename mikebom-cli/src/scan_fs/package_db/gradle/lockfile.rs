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
    let lifecycle_scope = if is_buildscript {
        Some(LifecycleScope::Build)
    } else {
        None
    };

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
}
