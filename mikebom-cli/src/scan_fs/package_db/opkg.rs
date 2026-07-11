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
use mikebom_common::types::license::SpdxExpression;
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
    // Milestone 169 T033 (US5, FR-010): read the distro tag once at the
    // reader entry point. When absent, emitted PURLs omit the `distro=`
    // qualifier entirely — no hardcoded default.
    let distro_tag = super::super::os_release::read_distro_tag_from_rootfs(rootfs);
    let distro_tag_ref = distro_tag.as_deref();
    if !status_path.is_file() {
        // Milestone 169 (T020, FR-014): status file absent → fall back
        // to enumerating `/var/lib/opkg/info/*.control` per-package
        // files. Emits one entry per control file. Fires an INFO log
        // so operators can tell the fallback was taken.
        let info_dir = rootfs.join(OPKG_INFO_DIR);
        if info_dir.is_dir() {
            tracing::info!(
                info_dir = %info_dir.display(),
                "opkg installed-DB: status file absent; falling back to info/*.control per FR-014"
            );
            let out = parse_info_dir_fallback(&info_dir, &ctx, distro_tag_ref);
            return (out, ctx);
        }
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
    let out = parse(&text, &source_path, &ctx, distro_tag_ref);
    (out, ctx)
}

/// Milestone 169 (T020, FR-014): fallback enumeration of
/// `/var/lib/opkg/info/*.control` files when `/var/lib/opkg/status` is
/// absent. Each `.control` file is a single-stanza subset of
/// `status`-file syntax, parsed via the same `parse_stanzas` helper.
///
/// Adds a `mikebom:opkg-status-fallback = "true"` annotation on every
/// emitted entry so consumers can distinguish primary-parse emissions
/// from fallback-parse emissions.
fn parse_info_dir_fallback(
    info_dir: &Path,
    ctx: &ScanContext,
    distro_tag: Option<&str>,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(info_dir) else {
        return out;
    };
    for dirent in read_dir.flatten() {
        let path = dirent.path();
        let is_control = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("control"))
            .unwrap_or(false);
        if !is_control {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let source_path = path.to_string_lossy().into_owned();
        let stanzas = super::control_file::parse_stanzas(&text);
        for stanza in stanzas {
            if let Some(mut entry) = build_entry(&stanza, &source_path, ctx, distro_tag) {
                entry.extra_annotations.insert(
                    "mikebom:opkg-status-fallback".to_string(),
                    serde_json::Value::String("true".to_string()),
                );
                out.push(entry);
            }
        }
    }
    out
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

fn parse(
    text: &str,
    source_path: &str,
    ctx: &ScanContext,
    distro_tag: Option<&str>,
) -> Vec<PackageDbEntry> {
    let stanzas = super::control_file::parse_stanzas(text);
    let mut out = Vec::with_capacity(stanzas.len());
    for stanza in stanzas {
        if let Some(entry) = build_entry(&stanza, source_path, ctx, distro_tag) {
            out.push(entry);
        }
    }
    out
}

fn build_entry(
    stanza: &super::control_file::ControlStanza,
    source_path: &str,
    ctx: &ScanContext,
    distro_tag: Option<&str>,
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
    // Milestone 169 T021: Q2 alternative-list clarification —
    // `Depends: pkg-a | pkg-b` now emits ONLY `pkg-a` as an edge and
    // records `pkg-b` in `mikebom:dep-alternative-alternates`. Uses
    // the shared parser at `control_file::parse_depends_field_with_alternatives`.
    let depends_raw = stanza.depends().unwrap_or("");
    let super::control_file::DepsWithAlternatives {
        resolved: depends,
        alternates_by_source,
    } = super::control_file::parse_depends_field_with_alternatives(depends_raw);
    let purl = build_opkg_purl(&name, &version, &arch, distro_tag)?;

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
    // Milestone 169 T021 (Q2 clarification): emit alt-list fallbacks
    // as a per-source-component annotation. Key = first-alt name (the
    // one in `depends`); value = JSON array of fallback names.
    if !alternates_by_source.is_empty() {
        let json_map: serde_json::Map<String, serde_json::Value> = alternates_by_source
            .into_iter()
            .map(|(k, v)| {
                let arr: Vec<serde_json::Value> =
                    v.into_iter().map(serde_json::Value::String).collect();
                (k, serde_json::Value::Array(arr))
            })
            .collect();
        extra_annotations.insert(
            "mikebom:dep-alternative-alternates".to_string(),
            serde_json::Value::Object(json_map),
        );
    }

    // Milestone 185 US2 (#539) — extract License from the stanza and
    // normalize through the same pipeline the rpm reader uses, PLUS
    // an m185 4th-pass wholesale-wrap fallback (per FR-014 / Q1
    // clarification). See specs/185-ipk-reader-fixes/contracts/
    // license-pipeline.md for the full pass-by-pass decision matrix.
    let licenses: Vec<SpdxExpression> = stanza
        .license()
        .filter(|l| !l.trim().is_empty())
        .and_then(|raw| {
            // Pass 1: normalize BitBake `&`/`|` → SPDX `AND`/`OR`.
            let normalized = super::rpm_file::normalize_bitbake_license_operators(raw);
            // Pass 2: strict SPDX try_canonical.
            if let Ok(e) = SpdxExpression::try_canonical(&normalized) {
                return Some(e);
            }
            // Pass 3: preserve_known_operands_with_license_ref (rpm's
            // #481 per-operand LicenseRef wrap) + re-canonicalize.
            if let Some(wrapped) =
                super::rpm_file::preserve_known_operands_with_license_ref(&normalized)
            {
                if let Ok(e) = SpdxExpression::try_canonical(&wrapped) {
                    return Some(e);
                }
            }
            // Pass 4 (m185 US2 wholesale-wrap, opkg-only per research
            // Decision 3) — wrap the WHOLE original string as a single
            // LicenseRef-<sanitized> operand. Preserves raw data for
            // downstream license auditors instead of dropping to
            // NOASSERTION.
            let sanitized = super::rpm_file::sanitize_to_license_ref_idstring(raw)?;
            let wrapped = format!("LicenseRef-{sanitized}");
            tracing::warn!(
                source_path = %source_path,
                package = %name,
                raw_license = %raw,
                wrapped = %wrapped,
                "opkg License string failed strict + per-operand SPDX parse; \
                 wholesale-wrapped as LicenseRef per m185 FR-014"
            );
            SpdxExpression::try_canonical(&wrapped).ok()
        })
        .into_iter()
        .collect();

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: Some(arch),
        source_path: source_path.to_string(),
        depends,
        maintainer,
        licenses,
        lifecycle_scope,
        requirement_range: None,
        source_type: None,
        buildinfo_status: None,
        // Milestone 169 T019 (FR-015): opkg installed-DB is the sibling
        // evidence-kind to rpm's `rpmdb-sqlite`. Distinguishes from
        // archive-file emissions (`ipk-file` per FR-009).
        evidence_kind: Some("opkg-status-db".to_string()),
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

/// Build a `pkg:opkg/<name>@<version>?arch=<arch>[&distro=<tag>]` PURL
/// per FR-004 + FR-010 (m169 T033 US5). Mirrors
/// `ipk_file::build_opkg_purl` verbatim so archive-file + installed-DB
/// emissions produce byte-identical PURLs for the same package. When
/// `distro_tag` is `None`, the qualifier is omitted (no default).
fn build_opkg_purl(
    name: &str,
    version: &str,
    arch: &str,
    distro_tag: Option<&str>,
) -> Option<Purl> {
    let mut purl_str = if version.is_empty() {
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
    if let Some(tag) = distro_tag {
        if !tag.is_empty() {
            purl_str.push_str("&distro=");
            purl_str.push_str(&encode_purl_segment(tag));
        }
    }
    Purl::new(&purl_str).ok()
}

// NOTE (Milestone 169 T021): the previous `fn parse_depends` was
// replaced by the shared `control_file::parse_depends_field_with_alternatives`
// helper — see build_entry() above. The Q2 alternative-list treatment
// requires the shared helper's `DepsWithAlternatives` return shape.

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

    // ------------------------------------------------------------
    // Milestone 169 T022 (FR-015): status-DB primary parse emits
    // `opkg-status-db` evidence-kind.
    // ------------------------------------------------------------
    #[test]
    fn t022_status_db_primary_parse_emits_opkg_status_db_evidence_kind() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-lib\n\
             Version: 1.2.3\n\
             Architecture: mikebom-fixture-arch\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].evidence_kind.as_deref(),
            Some("opkg-status-db"),
            "installed-DB emissions MUST carry evidence-kind opkg-status-db per FR-015"
        );
    }

    // ------------------------------------------------------------
    // Milestone 169 T023 (FR-014): info/*.control fallback when
    // status file is absent.
    // ------------------------------------------------------------
    #[test]
    fn t023_info_dir_fallback_fires_when_status_absent() {
        let tmp = tempfile::tempdir().unwrap();
        // Create OPKG_INFO_DIR but NO status file.
        let info_dir = tmp.path().join(OPKG_INFO_DIR);
        std::fs::create_dir_all(&info_dir).unwrap();
        std::fs::write(
            info_dir.join("busybox.control"),
            "Package: busybox\n\
             Version: 1.36.1-r0\n\
             Architecture: core2-64\n\
             License: GPL-2.0\n",
        )
        .unwrap();
        std::fs::write(
            info_dir.join("glibc.control"),
            "Package: glibc\n\
             Version: 2.39-r0\n\
             Architecture: core2-64\n\
             License: LGPL-2.1\n",
        )
        .unwrap();

        let (entries, _ctx) = read(tmp.path());
        assert_eq!(
            entries.len(),
            2,
            "FR-014 fallback should emit one entry per info/*.control file"
        );
        // Every fallback emission carries the marker annotation.
        for e in &entries {
            assert_eq!(
                e.extra_annotations
                    .get("mikebom:opkg-status-fallback")
                    .and_then(|v| v.as_str()),
                Some("true"),
                "fallback emissions must carry mikebom:opkg-status-fallback annotation"
            );
            assert_eq!(
                e.evidence_kind.as_deref(),
                Some("opkg-status-db"),
                "fallback emissions still use opkg-status-db evidence-kind"
            );
        }
    }

    // ------------------------------------------------------------
    // Milestone 169 T024 (Q2 alt-list): opkg reader wires through
    // the shared parse_depends_field_with_alternatives helper.
    // ------------------------------------------------------------
    #[test]
    fn t024_depends_alternative_list_semantic_matches_us1() {
        let tmp = tempfile::tempdir().unwrap();
        write_status(
            tmp.path(),
            "Package: mikebom-fixture-app\n\
             Version: 1.0.0\n\
             Architecture: mikebom-fixture-arch\n\
             Depends: libmbedtls-12 | libssl3\n\
             Status: install user installed\n",
        );
        let (entries, _ctx) = read(tmp.path());
        assert_eq!(entries.len(), 1);
        // Q2: first alt goes to depends[], fallbacks to annotation.
        assert_eq!(
            entries[0].depends,
            vec!["libmbedtls-12".to_string()],
            "first-wins: depends[] carries only the first alt"
        );
        let annotation = entries[0]
            .extra_annotations
            .get("mikebom:dep-alternative-alternates")
            .expect("alt-list annotation must be present");
        let obj = annotation.as_object().expect("annotation is JSON object");
        assert_eq!(
            obj.get("libmbedtls-12")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str()),
            Some("libssl3"),
            "libssl3 recorded as fallback under libmbedtls-12 key"
        );
    }

    // ── Milestone 185 US2 — opkg License extraction (#539) ──────────

    /// Helper for m185 US2 tests — writes an opkg-status file with a
    /// single stanza carrying a specified License field (or none), then
    /// returns the emitted entry for that stanza.
    fn m185_entry_for_license(rootfs: &Path, license_field: Option<&str>) -> PackageDbEntry {
        let body = match license_field {
            Some(l) => format!(
                "Package: m185-pkg\n\
                 Version: 1.0-r0\n\
                 Architecture: mikebom-fixture-arch\n\
                 License: {l}\n\
                 Status: install user installed\n",
            ),
            None => "Package: m185-pkg\n\
                     Version: 1.0-r0\n\
                     Architecture: mikebom-fixture-arch\n\
                     Status: install user installed\n"
                .to_string(),
        };
        write_status(rootfs, &body);
        let (entries, _ctx) = read(rootfs);
        assert_eq!(entries.len(), 1, "expected exactly one stanza emitted");
        entries.into_iter().next().unwrap()
    }

    #[test]
    fn build_entry_extracts_canonical_spdx_license() {
        // US2 acceptance 1 — Pass 2 (strict SPDX) success path.
        let tmp = tempfile::tempdir().unwrap();
        let entry = m185_entry_for_license(tmp.path(), Some("GPL-2.0-only"));
        assert_eq!(entry.licenses.len(), 1);
        assert_eq!(entry.licenses[0].as_str(), "GPL-2.0-only");
    }

    #[test]
    fn build_entry_bitbake_operator_normalizes_and_wraps_unknown_operand() {
        // US2 acceptance 2 — Pass 1 + Pass 3 success path.
        // `GPL-2.0-only & bzip2-1.0.4` → normalize `&` to `AND`, wrap
        // unknown bzip2 operand as LicenseRef-. Uses canonical
        // `GPL-2.0-only` because the rpm reader's normalization
        // pipeline treats non-canonical synonyms (like Yocto's
        // legacy `GPLv2`) as unknown operands and wraps them as
        // LicenseRef- rather than substituting to canonical form.
        let tmp = tempfile::tempdir().unwrap();
        let entry =
            m185_entry_for_license(tmp.path(), Some("GPL-2.0-only & bzip2-1.0.4"));
        assert_eq!(entry.licenses.len(), 1);
        assert_eq!(
            entry.licenses[0].as_str(),
            "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"
        );
    }

    #[test]
    fn build_entry_yocto_synonym_wraps_both_operands() {
        // Yocto's legacy recipes emit non-canonical `GPLv2` (the
        // pre-SPDX-list synonym). The pipeline wraps it as
        // LicenseRef-GPLv2 rather than substituting to GPL-2.0-only.
        // This is intentional — no synonym normalization dictionary
        // in mikebom. Documented behavior; operators who want the
        // canonical form should update the recipe LICENSE variable.
        let tmp = tempfile::tempdir().unwrap();
        let entry = m185_entry_for_license(tmp.path(), Some("GPLv2 & bzip2-1.0.4"));
        assert_eq!(entry.licenses.len(), 1);
        assert_eq!(
            entry.licenses[0].as_str(),
            "LicenseRef-GPLv2 AND LicenseRef-bzip2-1.0.4"
        );
    }

    #[test]
    fn build_entry_absent_license_stays_empty() {
        // FR-007 regression pin — absent License emits Vec::new(),
        // SPDX 2.3 falls through to NOASSERTION.
        let tmp = tempfile::tempdir().unwrap();
        let entry = m185_entry_for_license(tmp.path(), None);
        assert!(
            entry.licenses.is_empty(),
            "expected empty licenses for absent License field, got: {:?}",
            entry.licenses
        );
    }

    #[test]
    fn build_entry_whitespace_only_license_treated_as_absent() {
        // Edge case per Assumptions — whitespace-only License is
        // filtered at Pass 0 as if absent.
        let tmp = tempfile::tempdir().unwrap();
        let entry = m185_entry_for_license(tmp.path(), Some("   "));
        assert!(
            entry.licenses.is_empty(),
            "expected empty licenses for whitespace-only License, got: {:?}",
            entry.licenses
        );
    }

    #[test]
    fn build_entry_unparseable_license_wholesale_wraps() {
        // FR-014 m185 wholesale-wrap fallback — Pass 4 fires for
        // wholly unparseable strings. Emits a single LicenseRef-
        // operand preserving the sanitized raw data.
        let tmp = tempfile::tempdir().unwrap();
        let entry = m185_entry_for_license(tmp.path(), Some("!!! bad syntax &&& random"));
        assert_eq!(entry.licenses.len(), 1);
        let s = entry.licenses[0].as_str();
        assert!(
            s.starts_with("LicenseRef-"),
            "expected wholesale-wrapped LicenseRef-, got: {s}"
        );
        // Sanitized form should contain the alphanumeric fragments of
        // the raw input (per sanitize_to_license_ref_idstring's regex).
        assert!(
            s.contains("bad") && s.contains("syntax") && s.contains("random"),
            "sanitized form should preserve alphanumeric content, got: {s}"
        );
    }

    #[test]
    fn build_entry_unsanitizable_license_falls_through_to_empty() {
        // FR-014 defensive edge — a purely-symbol License produces
        // None from sanitize_to_license_ref_idstring and falls
        // through to licenses: Vec::new() (matches FR-007 shape).
        let tmp = tempfile::tempdir().unwrap();
        let entry = m185_entry_for_license(tmp.path(), Some("!!!"));
        assert!(
            entry.licenses.is_empty(),
            "expected empty licenses for unsanitizable License (all symbols), got: {:?}",
            entry.licenses
        );
    }
}
