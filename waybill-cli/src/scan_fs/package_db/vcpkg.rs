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
use std::sync::{Arc, Mutex};

use waybill_common::types::purl::{encode_purl_segment, Purl};
use serde::Deserialize;

use super::PackageDbEntry;

// Milestone 664 US2 T053: shared-walker marker-detect registration.
use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

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

/// Milestone 664 US2 T053: marker-detect state. Vcpkg has no tree
/// walker (fixed-root scan); its shared-walker registration exists
/// only to record whether ANY `vcpkg.json` was seen during descent.
/// Finalize gates the O(1) `read()` on the flag + defensive fs
/// fallback so non-vcpkg repos save the manifest-path stat.
#[derive(Default, Debug)]
pub(crate) struct VcpkgMarkerState {
    pub(crate) seen: bool,
}

fn on_vcpkg_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    if path.file_name().and_then(|s| s.to_str()) != Some(VCPKG_MANIFEST) {
        return;
    }
    let Some(state) = ctx.state::<Mutex<VcpkgMarkerState>>(ReaderId::VCPKG) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.seen = true;
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/vcpkg.json"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::VCPKG,
        state: Some(Arc::new(Mutex::new(VcpkgMarkerState::default()))),
        patterns,
        on_file: Some(on_vcpkg_file),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_marker(registration: &ReaderRegistration) -> VcpkgMarkerState {
    let Some(state_arc) = registration.state.as_ref() else {
        return VcpkgMarkerState::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<VcpkgMarkerState>>() else {
        return VcpkgMarkerState::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Post-walker entry — gates the O(1) fixed-root read on marker
/// presence + defensive fs-existence fallback (preserves FR-006
/// byte-identity for pathological layouts where the walker missed
/// the manifest via exclusion or symlink resolution).
pub(crate) fn finalize(
    marker: VcpkgMarkerState,
    scan_root: &Path,
) -> Vec<PackageDbEntry> {
    if !marker.seen && !scan_root.join(VCPKG_MANIFEST).is_file() {
        return Vec::new();
    }
    read(scan_root)
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
