//! Yocto image-manifest reader (milestone 107 US2, FR-003).
//!
//! When BitBake builds an image, it writes a manifest listing exactly
//! which packages went into the rootfs:
//! `build/tmp/deploy/images/<machine>/<image>.manifest`. The format is
//! line-oriented, one component per line, `<name> <arch> <version>`
//! separated by single spaces. Stable since Yocto 2.0 (2015).
//!
//! mikebom walks the scan target for any `*.manifest` file under a
//! `build/tmp/deploy/images/*/` ancestor path and emits one
//! `pkg:opkg/<name>@<version>?arch=<arch>` component per line. Same
//! PURL ecosystem as the opkg-installed-DB reader — the milestone-105
//! dedup pipeline collapses cross-source emissions on canonical PURL.
//!
//! Per FR-010 precedence: `OpkgInstalled` > `YoctoImageManifest`. When
//! the same scan contains both an opkg-installed-DB stanza and a
//! manifest line naming the same coord, the installed-DB component
//! wins; the loser's source-mechanism (`"yocto-image-manifest"`)
//! appears in the surviving component's `mikebom:also-detected-via`
//! annotation.

use std::path::{Path, PathBuf};

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::super::PackageDbEntry;

/// Host-arch literals shared with the opkg-installed reader — kept in
/// sync via FR-006. Stanzas / lines naming these arches are
/// host-side build tools and always carry `LifecycleScope::Build`.
const HOST_ARCH_LITERALS: &[&str] = &["x86_64", "i686", "aarch64", "arm64"];

const NATIVESDK_PREFIX: &str = "nativesdk-";

/// Walk the scan target for Yocto image-manifest files and emit one
/// `PackageDbEntry` per line. Returns empty when no `build/tmp/deploy/
/// images/*/*.manifest` files are found.
pub fn read(rootfs: &Path) -> Vec<PackageDbEntry> {
    let images_dir = rootfs
        .join("build")
        .join("tmp")
        .join("deploy")
        .join("images");
    if !images_dir.is_dir() {
        return Vec::new();
    }
    let manifest_paths = find_manifest_files(&images_dir);
    let mut out = Vec::new();
    for path in manifest_paths {
        out.extend(parse_manifest_file(&path));
    }
    out
}

/// Walk one level deep under `images/` (each subdir is a `<machine>/`)
/// and collect any `*.manifest` files inside. Bounded to one depth
/// level per the documented Yocto layout; non-recursive to avoid
/// picking up unrelated `*.manifest` files elsewhere in the tree.
fn find_manifest_files(images_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(machines) = std::fs::read_dir(images_dir) else {
        return out;
    };
    for machine_entry in machines.flatten() {
        let machine_dir = machine_entry.path();
        if !machine_dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&machine_dir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let path = file_entry.path();
            let is_manifest = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("manifest"))
                .unwrap_or(false);
            if is_manifest && path.is_file() {
                out.push(path);
            }
        }
    }
    out
}

fn parse_manifest_file(path: &Path) -> Vec<PackageDbEntry> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read Yocto image manifest (skipping; FR-012)"
            );
            return Vec::new();
        }
    };
    let source_path = path.to_string_lossy().into_owned();
    parse_manifest(&text, &source_path)
}

fn parse_manifest(text: &str, source_path: &str) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    for (lineno, raw_line) in text.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() != 3 {
            tracing::warn!(
                path = %source_path,
                line = lineno + 1,
                content = %raw_line,
                "Yocto manifest line has wrong token count (expected 3); skipping"
            );
            continue;
        }
        let name = tokens[0];
        let arch = tokens[1];
        let version = tokens[2];
        if let Some(entry) = build_entry(name, arch, version, source_path) {
            out.push(entry);
        }
    }
    out
}

fn build_entry(name: &str, arch: &str, version: &str, source_path: &str) -> Option<PackageDbEntry> {
    let purl = build_opkg_purl(name, version, arch)?;

    // FR-006 per-line lifecycle-scope override: nativesdk- prefix OR
    // host-arch literal → LifecycleScope::Build. Manifests are the
    // device's INTENDED contents (runtime) so non-host-arch entries
    // carry no scope.
    let is_nativesdk = name.starts_with(NATIVESDK_PREFIX);
    let is_host_arch = HOST_ARCH_LITERALS
        .iter()
        .any(|literal| literal.eq_ignore_ascii_case(arch));
    let lifecycle_scope = if is_nativesdk || is_host_arch {
        Some(LifecycleScope::Build)
    } else {
        None
    };

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("yocto-image-manifest".to_string()),
    );
    // Carry the image's logical name (manifest filename stem) as an
    // informational annotation so downstream consumers can group by
    // image variant (e.g., `core-image-minimal` vs `core-image-sato`).
    if let Some(image_name) = Path::new(source_path)
        .file_stem()
        .and_then(|s| s.to_str())
    {
        extra_annotations.insert(
            "mikebom:image-name".to_string(),
            serde_json::Value::String(image_name.to_string()),
        );
    }

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: Some(arch.to_string()),
        source_path: source_path.to_string(),
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
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

fn build_opkg_purl(name: &str, version: &str, arch: &str) -> Option<Purl> {
    Purl::new(&format!(
        "pkg:opkg/{}@{}?arch={}",
        encode_purl_segment(name),
        encode_purl_segment(version),
        encode_purl_segment(arch),
    ))
    .ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn emits_one_component_per_line() {
        let text = "mikebom-fixture-libc mikebom-fixture-arch 2.38\n\
                    mikebom-fixture-openssl mikebom-fixture-arch 3.0.5\n";
        let entries = parse_manifest(text, "test.manifest");
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:opkg/mikebom-fixture-libc@2.38?arch=mikebom-fixture-arch"
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("yocto-image-manifest"),
        );
        assert_eq!(
            entries[1].purl.as_str(),
            "pkg:opkg/mikebom-fixture-openssl@3.0.5?arch=mikebom-fixture-arch"
        );
    }

    #[test]
    fn nativesdk_lines_tagged_build() {
        let text = "nativesdk-mikebom-fixture-cmake x86_64 3.27.0\n";
        let entries = parse_manifest(text, "test.manifest");
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn host_arch_lines_tagged_build() {
        let text = "mikebom-fixture-buildtool x86_64 1.0\n";
        let entries = parse_manifest(text, "test.manifest");
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Build)
        ));
    }

    #[test]
    fn target_arch_lines_have_no_lifecycle_scope() {
        let text = "mikebom-fixture-lib mikebom-fixture-arch 1.0\n";
        let entries = parse_manifest(text, "test.manifest");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].lifecycle_scope.is_none());
    }

    #[test]
    fn wrong_token_count_warns_and_skips() {
        // 2-token line + 4-token line both invalid; 3-token line valid.
        let text = "only-two tokens\n\
                    mikebom-fixture-lib mikebom-fixture-arch 1.0\n\
                    one two three four\n";
        let entries = parse_manifest(text, "test.manifest");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mikebom-fixture-lib");
    }

    #[test]
    fn empty_and_comment_lines_ignored() {
        let text = "\n\
                    # a comment line\n\
                    mikebom-fixture-lib mikebom-fixture-arch 1.0\n\
                    \n";
        let entries = parse_manifest(text, "test.manifest");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn image_name_annotation_derived_from_filename_stem() {
        let text = "mikebom-fixture-lib mikebom-fixture-arch 1.0\n";
        let entries = parse_manifest(text, "/build/tmp/deploy/images/qemux86-64/core-image-minimal.manifest");
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:image-name")
                .and_then(|v| v.as_str()),
            Some("core-image-minimal"),
        );
    }
}
