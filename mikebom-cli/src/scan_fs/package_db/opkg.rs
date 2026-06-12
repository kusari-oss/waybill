//! opkg installed-package-DB reader (milestone 107 US1, US3, US5;
//! closes the Yocto/OE rootfs + SDK sysroot coverage gap deferred
//! from milestone 105).
//!
//! opkg is the package manager used by virtually every Yocto/OE-based
//! distribution that doesn't explicitly opt into rpm or dpkg
//! (OpenSTLinux, Poky reference images, Toradex BSPs, Variscite BSPs,
//! …). Its installed-DB at `/var/lib/opkg/status` uses byte-identical
//! RFC-822 control-file syntax to dpkg — so this reader delegates
//! stanza parsing to the shared `control_file` helper introduced by
//! milestone 107's foundation refactor.
//!
//! Per-stanza emission:
//! - PURL: `pkg:opkg/<name>@<version>?arch=<arch>`
//! - Lifecycle scope: `LifecycleScope::Build` when the scan target is
//!   detected as an SDK sysroot (`yocto::context::detect_scan_context`)
//!   OR when the stanza is a nativesdk-prefixed / host-arch package
//!   per FR-006.
//! - Claimed files: read from `/usr/lib/opkg/info/<name>.list` and
//!   inserted into the binary walker's claim set (prevents duplicate
//!   `pkg:generic/<basename>` emissions).
//!
//! Filesystem-only — no network, no subprocess. FR-011 audit
//! tracks the new module in milestone 107 polish.

use std::path::Path;

use anyhow::Result;
use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::yocto::context::{detect_scan_context, ScanContext};
use super::PackageDbEntry;

const OPKG_STATUS_PATH: &str = "var/lib/opkg/status";
const OPKG_INFO_DIR: &str = "usr/lib/opkg/info";

/// Host-arch literals that mark a stanza as host-side / build-only
/// per FR-006. Update `contracts/opkg-installed-db.md` when adding
/// new arches (e.g., a future RISC-V dev machine).
const HOST_ARCH_LITERALS: &[&str] = &["x86_64", "i686", "aarch64", "arm64"];

const NATIVESDK_PREFIX: &str = "nativesdk-";

/// Walk the rootfs's opkg installed-DB and emit one `PackageDbEntry`
/// per stanza. Returns empty when `/var/lib/opkg/status` is absent
/// (graceful no-op when scanning a non-Yocto rootfs).
///
/// The second return value is the `ScanContext` produced by the
/// sysroot-vs-rootfs heuristic — the dispatcher uses it to drive the
/// scan-ambiguity diagnostic annotation (FR-005a).
pub fn read(rootfs: &Path) -> (Vec<PackageDbEntry>, ScanContext) {
    let status_path = rootfs.join(OPKG_STATUS_PATH);
    let ctx = detect_scan_context(rootfs);
    if !status_path.is_file() {
        return (Vec::new(), ctx);
    }
    let text = match std::fs::read_to_string(&status_path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                path = %status_path.display(),
                error = %e,
                "failed to read opkg status file (skipping; FR-012)"
            );
            return (Vec::new(), ctx);
        }
    };
    let source_path = status_path.to_string_lossy().into_owned();
    let out = parse(&text, &source_path, &ctx);
    (out, ctx)
}

/// Read per-package `<rootfs>/usr/lib/opkg/info/<pkg>.list` files and
/// insert each enumerated path into the binary walker's claim set.
/// Mirrors `dpkg::collect_claimed_paths`.
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut std::collections::HashSet<(u64, u64)>,
) -> Result<()> {
    let info_dir = rootfs.join(OPKG_INFO_DIR);
    if !info_dir.is_dir() {
        return Ok(());
    }
    let Ok(read_dir) = std::fs::read_dir(&info_dir) else {
        return Ok(());
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let is_list = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("list"))
            .unwrap_or(false);
        if !is_list {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Paths in opkg .list files are absolute (rooted at /);
            // join against rootfs to canonicalize.
            let relative = trimmed.strip_prefix('/').unwrap_or(trimmed);
            let target = rootfs.join(relative);
            #[cfg(unix)]
            {
                if let Ok(meta) = std::fs::metadata(&target) {
                    use std::os::unix::fs::MetadataExt;
                    claimed_inodes.insert((meta.dev(), meta.ino()));
                }
            }
            claimed.insert(target);
        }
    }
    Ok(())
}

fn parse(text: &str, source_path: &str, ctx: &ScanContext) -> Vec<PackageDbEntry> {
    let stanzas = super::control_file::parse_stanzas(text);
    let mut out = Vec::with_capacity(stanzas.len());
    for stanza in stanzas {
        if let Some(entry) = build_entry(&stanza, source_path, ctx) {
            out.push(entry);
        }
    }
    out
}

fn build_entry(
    stanza: &super::control_file::ControlStanza,
    source_path: &str,
    ctx: &ScanContext,
) -> Option<PackageDbEntry> {
    let name = stanza.name()?.to_string();
    if name.is_empty() {
        tracing::warn!(source = %source_path, "opkg stanza missing Package: field; skipping");
        return None;
    }
    let version_raw = stanza.version().unwrap_or("");
    let version_missing = version_raw.is_empty();
    let version = if version_missing {
        // Per data-model.md: emit with `mikebom:version-status: "missing"`
        // annotation rather than dropping.
        String::new()
    } else {
        version_raw.to_string()
    };
    let arch = stanza.architecture().unwrap_or("all").to_string();
    let maintainer = stanza
        .maintainer()
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let depends = stanza
        .depends()
        .map(parse_depends)
        .unwrap_or_default();
    let purl = build_opkg_purl(&name, &version, &arch)?;

    // FR-006 + FR-005a lifecycle-scope decision.
    let is_nativesdk = name.starts_with(NATIVESDK_PREFIX);
    let is_host_arch = HOST_ARCH_LITERALS
        .iter()
        .any(|literal| literal.eq_ignore_ascii_case(&arch));
    let lifecycle_scope = if is_nativesdk || is_host_arch || ctx.applies_build_scope() {
        Some(LifecycleScope::Build)
    } else {
        None
    };

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("opkg-installed".to_string()),
    );
    if version_missing {
        extra_annotations.insert(
            "mikebom:version-status".to_string(),
            serde_json::Value::String("missing".to_string()),
        );
    }

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: Some(arch),
        source_path: source_path.to_string(),
        depends,
        maintainer,
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
        sbom_tier: Some("deployed".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

fn build_opkg_purl(name: &str, version: &str, arch: &str) -> Option<Purl> {
    let purl_str = if version.is_empty() {
        format!(
            "pkg:opkg/{}?arch={}",
            encode_purl_segment(name),
            encode_purl_segment(arch)
        )
    } else {
        format!(
            "pkg:opkg/{}@{}?arch={}",
            encode_purl_segment(name),
            encode_purl_segment(version),
            encode_purl_segment(arch)
        )
    };
    Purl::new(&purl_str).ok()
}

/// Tokenize opkg's `Depends:` field. Same shape as dpkg —
/// comma-separated dep names with optional version constraints in
/// parens that we strip.
fn parse_depends(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|tok| {
            let tok = tok.trim();
            // Strip version constraint: "libc (>= 2.38)" -> "libc"
            let name = tok.split_whitespace().next().unwrap_or("").trim();
            name.to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn write_status(rootfs: &Path, body: &str) {
        let p = rootfs.join(OPKG_STATUS_PATH);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn write_list(rootfs: &Path, pkg: &str, paths: &str) {
        let p = rootfs.join(OPKG_INFO_DIR).join(format!("{pkg}.list"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, paths).unwrap();
    }

    #[test]
    fn emits_basic_components() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-lib\n\
             Version: 1.2.3\n\
             Architecture: mikebom-fixture-arch\n\
             Maintainer: Mikebom Fixture <fixture@example.invalid>\n\
             Status: install user installed\n\
             \n\
             Package: mikebom-fixture-other\n\
             Version: 2.3.4\n\
             Architecture: mikebom-fixture-arch\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:opkg/mikebom-fixture-lib@1.2.3?arch=mikebom-fixture-arch"
        );
        assert_eq!(entries[0].maintainer.as_deref(), Some("Mikebom Fixture <fixture@example.invalid>"));
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("opkg-installed"),
        );
    }

    #[test]
    fn claims_files_from_info_dot_list() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-lib\n\
             Version: 1.0\n\
             Architecture: mikebom-fixture-arch\n\
             Status: install user installed\n",
        );
        // Create the actual file so the inode lookup succeeds.
        let target = tmp.path().join("usr/bin/mikebom-fixture-binary");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "").unwrap();
        write_list(
            tmp.path(),
            "mikebom-fixture-lib",
            "/usr/bin/mikebom-fixture-binary\n/usr/share/doc/mikebom-fixture-lib/README\n",
        );
        let mut claimed = std::collections::HashSet::new();
        #[cfg(unix)]
        let mut claimed_inodes = std::collections::HashSet::new();
        collect_claimed_paths(
            tmp.path(),
            &mut claimed,
            #[cfg(unix)]
            &mut claimed_inodes,
        )
        .unwrap();
        assert!(claimed.contains(&target));
        assert_eq!(claimed.len(), 2);
    }

    #[test]
    fn nativesdk_prefix_forces_build_scope_even_in_rootfs_context() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: nativesdk-mikebom-fixture-tool\n\
             Version: 1.0\n\
             Architecture: mikebom-fixture-arch\n\
             Status: install user installed\n",
        );
        let (entries, ctx) = read(tmp.path());
        assert!(matches!(ctx, ScanContext::Rootfs));
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].lifecycle_scope, Some(LifecycleScope::Build)));
    }

    #[test]
    fn host_arch_forces_build_scope_in_rootfs_context() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-tool\n\
             Version: 1.0\n\
             Architecture: x86_64\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].lifecycle_scope, Some(LifecycleScope::Build)));
    }

    #[test]
    fn target_arch_in_rootfs_context_has_no_lifecycle_scope() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-lib\n\
             Version: 1.0\n\
             Architecture: cortexa7t2hf-mikebom-fixture\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].lifecycle_scope.is_none());
    }

    #[test]
    fn sysroot_context_applies_build_scope_to_target_arch() {
        let tmp = tempfile::tempdir().unwrap();
        // Set up sysroot signal: env-script in parent.
        let parent = tmp.path();
        let sysroot = parent.join("sysroot");
        std::fs::create_dir_all(&sysroot).unwrap();
        std::fs::write(
            parent.join("environment-setup-mikebom-fixture-target"),
            "",
        )
        .unwrap();
        write_status(
            &sysroot,
            "Package: mikebom-fixture-lib\n\
             Version: 1.0\n\
             Architecture: cortexa7t2hf-mikebom-fixture\n\
             Status: install user installed\n",
        );
        let (entries, ctx) = read(&sysroot);
        assert!(matches!(ctx, ScanContext::Sysroot { primary_signal: true, .. }));
        assert_eq!(entries.len(), 1);
        // Target arch in a sysroot context → build scope (compile-time only).
        assert!(matches!(entries[0].lifecycle_scope, Some(LifecycleScope::Build)));
    }

    #[test]
    fn missing_version_emits_status_annotation() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-noversion\n\
             Architecture: mikebom-fixture-arch\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:version-status")
                .and_then(|v| v.as_str()),
            Some("missing"),
        );
    }

    #[test]
    fn unknown_fields_silently_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-lib\n\
             Version: 1.0\n\
             Architecture: mikebom-fixture-arch\n\
             Vendor-Extension: ignored\n\
             Installed-Time: 1234567890\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        // Unknown fields do not contaminate extra_annotations.
        assert!(!entries[0]
            .extra_annotations
            .contains_key("vendor-extension"));
    }

    #[test]
    fn depends_field_tokenized_with_version_constraints_stripped() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-parent\n\
             Version: 1.0\n\
             Architecture: mikebom-fixture-arch\n\
             Depends: mikebom-fixture-child (>= 2.0), mikebom-fixture-other (= 3.0.1)\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].depends,
            vec![
                "mikebom-fixture-child".to_string(),
                "mikebom-fixture-other".to_string()
            ]
        );
    }
}
