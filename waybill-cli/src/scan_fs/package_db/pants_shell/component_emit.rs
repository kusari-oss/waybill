//! Milestone 225: shell-script + pinned-tool → `PackageDbEntry` conversion.
//!
//! Two entry points:
//! - [`script_to_package_db_entry`] emits ONE file-tier component per
//!   `.sh` file discovered via BUILD-file target ownership.
//! - [`tool_to_package_db_entry`] emits ONE design-tier component per
//!   operator-pinned tool version from `pants.toml`.
//!
//! Both emit `pkg:generic/*` PURLs (per research §R3 + §R4) with the
//! `waybill:pants-target` (C145, new this feature) OR
//! `waybill:source-file` (m080 row) annotations. See
//! `contracts/build-file-dsl-schema.md` for the full field mapping.

use std::io::Read;
use std::path::Path;

use serde_json::json;
use sha2::Digest;
use waybill_common::resolution::LifecycleScope;
use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::ShellTargetKind;
use crate::scan_fs::package_db::PackageDbEntry;

/// Stream SHA-256 the file's bytes in 8 KB chunks. Returns
/// lowercase-hex. `None` on any I/O failure.
fn streaming_sha256_hex(abs_path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(abs_path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 8 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let out = hasher.finalize();
    let mut hex = String::with_capacity(out.len() * 2);
    for b in out.iter() {
        hex.push_str(&format!("{b:02x}"));
    }
    Some(hex)
}

/// Convert a resolved shell script file into a `PackageDbEntry`.
/// `target_addresses` is a slice of ALL owning target addresses;
/// they'll be sorted lexically + comma-joined into the
/// `waybill:pants-target` annotation.
///
/// Returns `None` + WARN on I/O error or PURL construction failure
/// (fail-open at emit grain).
pub(crate) fn script_to_package_db_entry(
    file: &Path,
    target_addresses: &[String],
    kind: ShellTargetKind,
    scan_root: &Path,
) -> Option<PackageDbEntry> {
    let sha256_full = match streaming_sha256_hex(file) {
        Some(h) => h,
        None => {
            tracing::warn!(
                path = %file.display(),
                "pants-shell reader: could not hash file; skipping"
            );
            return None;
        }
    };
    let basename = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if basename.is_empty() {
        tracing::warn!(
            path = %file.display(),
            "pants-shell reader: file has no basename; skipping"
        );
        return None;
    }
    let sha256_prefix: String = sha256_full.chars().take(12).collect();
    let purl_str = format!(
        "pkg:generic/{}@{}",
        encode_purl_segment(basename),
        encode_purl_segment(&sha256_prefix),
    );
    let purl = Purl::new(&purl_str)
        .map_err(|e| {
            tracing::warn!(
                path = %file.display(),
                purl_str = %purl_str,
                error = %e,
                "pants-shell reader: PURL construction failed; skipping"
            );
        })
        .ok()?;

    // Lex-sort + comma-join target addresses (SC-006 dedup contract).
    let mut sorted = target_addresses.to_vec();
    sorted.sort();
    sorted.dedup();
    let addresses_joined = sorted.join(",");

    // Relative path for waybill:source-files (scan-root-relative).
    let rel_path = file
        .strip_prefix(scan_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.display().to_string());

    let mut extra_annotations = std::collections::BTreeMap::new();
    extra_annotations.insert(
        "waybill:pants-target".to_string(),
        json!(addresses_joined),
    );
    extra_annotations.insert(
        "waybill:source-files".to_string(),
        json!(vec![rel_path]),
    );

    let hash = ContentHash::sha256(&sha256_full).ok()?;

    Some(PackageDbEntry {
        purl,
        name: basename.to_string(),
        version: sha256_prefix,
        arch: None,
        source_path: file.display().to_string(),
        depends: Vec::new(),
        maintainer: None,
        lifecycle_scope: Some(kind.lifecycle_scope()),
        requirement_ranges: Vec::new(),
        source_type: None,
        licenses: Vec::new(),
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
        hashes: vec![hash],
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

/// Convert a `pants.toml`-pinned tool into a design-tier
/// `pkg:generic/<tool>@<version>` component. Version is preserved
/// verbatim (leading `v` prefix kept).
///
/// Returns `None` on empty version or PURL construction failure.
pub(crate) fn tool_to_package_db_entry(
    tool_name: &str,
    version: &str,
    pants_toml_path: &Path,
    scan_root: &Path,
) -> Option<PackageDbEntry> {
    if tool_name.is_empty() || version.is_empty() {
        tracing::warn!(
            tool = %tool_name,
            version = %version,
            "pants-shell reader: tool pin has empty name/version; skipping"
        );
        return None;
    }
    let purl_str = format!(
        "pkg:generic/{}@{}",
        encode_purl_segment(tool_name),
        encode_purl_segment(version),
    );
    let purl = Purl::new(&purl_str)
        .map_err(|e| {
            tracing::warn!(
                tool = %tool_name,
                version = %version,
                error = %e,
                "pants-shell reader: PURL construction failed for tool pin; skipping"
            );
        })
        .ok()?;

    let rel = pants_toml_path
        .strip_prefix(scan_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "pants.toml".to_string());

    let mut extra_annotations = std::collections::BTreeMap::new();
    extra_annotations.insert(
        "waybill:source-file".to_string(),
        json!(rel),
    );
    // Milestone 236 (C151): pants_shell tool-pin design-tier reason.
    extra_annotations.insert(
        "waybill:unresolved-reason".to_string(),
        json!("pants shell tool pin without version specifier"),
    );

    Some(PackageDbEntry {
        purl,
        name: tool_name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: pants_toml_path.display().to_string(),
        depends: Vec::new(),
        maintainer: None,
        lifecycle_scope: Some(LifecycleScope::Development),
        requirement_ranges: Vec::new(),
        source_type: None,
        licenses: Vec::new(),
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
        build_inclusion: None,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_script(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    // Script tests
    #[test]
    fn script_happy_path_shell_source() {
        let dir = tempdir().unwrap();
        let file = write_script(dir.path(), "deploy.sh", b"#!/bin/sh\necho hello\n");
        let out = script_to_package_db_entry(
            &file,
            &["scripts:deploy".to_string()],
            ShellTargetKind::ShellSource,
            dir.path(),
        )
        .expect("emit ok");
        assert_eq!(out.name, "deploy.sh");
        assert!(out.purl.as_str().starts_with("pkg:generic/deploy.sh@"));
        assert_eq!(out.hashes.len(), 1);
        assert_eq!(out.sbom_tier.as_deref(), Some("source"));
        assert_eq!(out.lifecycle_scope, Some(LifecycleScope::Runtime));
        assert_eq!(
            out.extra_annotations
                .get("waybill:pants-target")
                .and_then(|v| v.as_str()),
            Some("scripts:deploy"),
        );
    }

    #[test]
    fn script_shunit2_tags_development() {
        let dir = tempdir().unwrap();
        let file = write_script(dir.path(), "test.sh", b"#!/bin/sh\n");
        let out = script_to_package_db_entry(
            &file,
            &["tests:unit".to_string()],
            ShellTargetKind::Shunit2Test,
            dir.path(),
        )
        .expect("emit ok");
        assert_eq!(out.lifecycle_scope, Some(LifecycleScope::Development));
    }

    #[test]
    fn script_multi_target_annotation_lex_sorted() {
        let dir = tempdir().unwrap();
        let file = write_script(dir.path(), "x.sh", b"content");
        let out = script_to_package_db_entry(
            &file,
            &["scripts:single".to_string(), "scripts:glob".to_string()],
            ShellTargetKind::ShellSource,
            dir.path(),
        )
        .expect("emit ok");
        // Lex-sorted: "scripts:glob" < "scripts:single".
        assert_eq!(
            out.extra_annotations
                .get("waybill:pants-target")
                .and_then(|v| v.as_str()),
            Some("scripts:glob,scripts:single"),
        );
    }

    #[test]
    fn script_purl_is_content_addressed_stable() {
        let dir = tempdir().unwrap();
        let file_a = write_script(dir.path(), "a.sh", b"same content");
        let out_a = script_to_package_db_entry(
            &file_a,
            &["scripts:a".to_string()],
            ShellTargetKind::ShellSource,
            dir.path(),
        )
        .unwrap();
        let file_b = write_script(dir.path(), "b.sh", b"same content");
        let out_b = script_to_package_db_entry(
            &file_b,
            &["scripts:b".to_string()],
            ShellTargetKind::ShellSource,
            dir.path(),
        )
        .unwrap();
        // Same content → same sha256 prefix → same "version" segment.
        assert_eq!(out_a.version, out_b.version);
        // Different basenames → different PURLs.
        assert_ne!(out_a.purl.as_str(), out_b.purl.as_str());
    }

    #[test]
    fn script_missing_file_returns_none() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope.sh");
        let out = script_to_package_db_entry(
            &missing,
            &["scripts:gone".to_string()],
            ShellTargetKind::ShellSource,
            dir.path(),
        );
        assert!(out.is_none());
    }

    // Tool tests
    #[test]
    fn tool_shellcheck_v_prefix_preserved() {
        let dir = tempdir().unwrap();
        let pants_toml = dir.path().join("pants.toml");
        std::fs::write(&pants_toml, "").unwrap();
        let out = tool_to_package_db_entry("shellcheck", "v0.9.0", &pants_toml, dir.path())
            .expect("emit ok");
        assert_eq!(out.purl.as_str(), "pkg:generic/shellcheck@v0.9.0");
        assert_eq!(out.sbom_tier.as_deref(), Some("design"));
        assert_eq!(out.lifecycle_scope, Some(LifecycleScope::Development));
        assert_eq!(
            out.extra_annotations
                .get("waybill:source-file")
                .and_then(|v| v.as_str()),
            Some("pants.toml"),
        );
    }

    #[test]
    fn tool_shfmt_no_v_prefix_preserved() {
        let dir = tempdir().unwrap();
        let pants_toml = dir.path().join("pants.toml");
        std::fs::write(&pants_toml, "").unwrap();
        let out = tool_to_package_db_entry("shfmt", "3.7.0", &pants_toml, dir.path())
            .expect("emit ok");
        assert_eq!(out.purl.as_str(), "pkg:generic/shfmt@3.7.0");
    }

    #[test]
    fn m236_pants_shell_design_tier_carries_unresolved_reason() {
        // Milestone 236 (C151): pants_shell tool pins carry the reason string.
        let dir = tempdir().unwrap();
        let pants_toml = dir.path().join("pants.toml");
        std::fs::write(&pants_toml, "").unwrap();
        let out = tool_to_package_db_entry("shellcheck", "v0.9.0", &pants_toml, dir.path())
            .expect("emit ok");
        assert_eq!(out.sbom_tier.as_deref(), Some("design"));
        let reason = out
            .extra_annotations
            .get("waybill:unresolved-reason")
            .expect("C151 annotation present");
        assert_eq!(
            reason.as_str().unwrap(),
            "pants shell tool pin without version specifier",
        );
    }

    #[test]
    fn tool_empty_version_returns_none() {
        let dir = tempdir().unwrap();
        let pants_toml = dir.path().join("pants.toml");
        std::fs::write(&pants_toml, "").unwrap();
        let out = tool_to_package_db_entry("shfmt", "", &pants_toml, dir.path());
        assert!(out.is_none());
    }
}
