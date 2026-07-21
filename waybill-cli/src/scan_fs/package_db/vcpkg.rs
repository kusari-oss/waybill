//! vcpkg manifest-mode reader (milestone 102 US3).
//!
//! Parses `vcpkg.json` at the scan root and emits one `pkg:vcpkg/<name>`
//! (or `pkg:vcpkg/<name>@<version>`) component per `dependencies[]` entry.
//! Both the string-form (`"zlib"`) and the object-form
//! (`{"name": "openssl", "version>=": "3.0.0"}`) are supported.
//! `overrides[]` entries substitute the version of an existing dep.
//!
//! Per spec FR-007 + Contract 7. Parse failures (truncated/invalid JSON)
//! emit a `tracing::warn!` and return zero components per FR-015.
//! Cross-platform (no `#[cfg(unix)]` per FR-013).
//!
//! No new Cargo deps — uses workspace `serde` + `serde_json`.

use std::path::Path;

use waybill_common::types::purl::{encode_purl_segment, Purl};
use serde::Deserialize;

use super::PackageDbEntry;

const VCPKG_MANIFEST: &str = "vcpkg.json";

/// vcpkg.json schema — just the fields milestone 102 consumes.
#[derive(Debug, Deserialize)]
struct VcpkgManifest {
    #[serde(default)]
    dependencies: Vec<Dependency>,
    #[serde(default)]
    overrides: Vec<Override>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Dependency {
    /// `"zlib"` — string-form, no version.
    Simple(String),
    /// `{"name": "openssl", "version>=": "3.0.0", ...}` — object-form.
    Detailed {
        name: String,
        /// vcpkg uses `version>=` in JSON. The `>=` characters survive
        /// the serde rename because JSON object keys are arbitrary
        /// Unicode strings.
        #[serde(rename = "version>=")]
        version_ge: Option<String>,
        // Other vcpkg fields (features, host, default-features, etc.)
        // are accepted-but-ignored — milestone 102 only consumes
        // name + version-floor.
    },
}

#[derive(Debug, Deserialize)]
struct Override {
    name: String,
    version: String,
}

/// Walk `scan_root` for `vcpkg.json` and emit one `PackageDbEntry`
/// per declared dependency. Returns empty when no manifest is present
/// or when parsing fails (parse errors logged via `tracing::warn!`).
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let manifest_path = scan_root.join(VCPKG_MANIFEST);
    if !manifest_path.is_file() {
        return Vec::new();
    }
    let source_path = manifest_path.to_string_lossy().to_string();
    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "failed to read vcpkg.json"
            );
            return Vec::new();
        }
    };
    let manifest: VcpkgManifest = match serde_json::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "failed to parse vcpkg.json (skipping; FR-015)"
            );
            return Vec::new();
        }
    };

    // Index overrides by name for O(1) post-process lookup.
    let mut override_versions: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for o in &manifest.overrides {
        override_versions.insert(o.name.clone(), o.version.clone());
    }

    let mut entries = Vec::new();
    for dep in &manifest.dependencies {
        let (name, declared_version) = match dep {
            Dependency::Simple(n) => (n.clone(), None),
            Dependency::Detailed { name, version_ge } => {
                (name.clone(), version_ge.clone())
            }
        };
        // Override wins per spec Edge Cases.
        let version = override_versions
            .get(&name)
            .cloned()
            .or(declared_version)
            .unwrap_or_default();
        if let Some(entry) = build_entry(&name, &version, &source_path) {
            entries.push(entry);
        }
    }
    entries
}

fn build_vcpkg_purl(name: &str, version: &str) -> Option<Purl> {
    let purl_str = if version.is_empty() {
        format!("pkg:vcpkg/{}", encode_purl_segment(name))
    } else {
        format!(
            "pkg:vcpkg/{}@{}",
            encode_purl_segment(name),
            encode_purl_segment(version)
        )
    };
    Purl::new(&purl_str).ok()
}

fn build_entry(name: &str, version: &str, source_path: &str) -> Option<PackageDbEntry> {
    let purl = build_vcpkg_purl(name, version)?;
    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: source_path.to_string(),
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
        extra_annotations: {
            // C/C++ provenance: explicit source-mechanism annotation
            // (closed-enum value `vcpkg-manifest`). See cmake.rs for
            // the full rationale + enum docs.
            let mut a: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            a.insert(
                "waybill:source-mechanism".to_string(),
                serde_json::json!("vcpkg-manifest"),
            );
            a
        },
        binary_role: None,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn empty_when_no_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path()).is_empty());
    }

    #[test]
    fn simple_string_dependency_emits_no_version() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("vcpkg.json"),
            r#"{"dependencies": ["zlib"]}"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:vcpkg/zlib");
    }

    #[test]
    fn detailed_dependency_with_version_ge_emits_version() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("vcpkg.json"),
            r#"{"dependencies": [{"name": "openssl", "version>=": "3.0.0"}]}"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:vcpkg/openssl@3.0.0");
    }

    #[test]
    fn override_substitutes_version() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("vcpkg.json"),
            r#"{
                "dependencies": [{"name": "openssl", "version>=": "3.0.0"}],
                "overrides": [{"name": "openssl", "version": "3.2.1"}]
            }"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:vcpkg/openssl@3.2.1");
    }

    #[test]
    fn malformed_json_skips_silently_with_warn() {
        let tmp = tempfile::tempdir().unwrap();
        // Truncated — unbalanced braces.
        std::fs::write(
            tmp.path().join("vcpkg.json"),
            r#"{"dependencies": ["zlib""#,
        )
        .unwrap();
        // No panic; zero components per FR-015.
        assert!(read(tmp.path()).is_empty());
    }

    #[test]
    fn source_mechanism_annotation_vcpkg_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("vcpkg.json"),
            r#"{"dependencies":["zlib","openssl"]}"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 2);
        for e in &entries {
            assert_eq!(
                e.extra_annotations
                    .get("waybill:source-mechanism")
                    .and_then(|v| v.as_str()),
                Some("vcpkg-manifest"),
                "every vcpkg entry should carry source-mechanism: vcpkg-manifest; got: {:?}",
                e.extra_annotations.get("waybill:source-mechanism"),
            );
        }
    }
}
