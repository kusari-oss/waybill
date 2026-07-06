//! Milestone 169 (issue #500) — ipk archive-file package-database reader.
//!
//! Closes the 0-component-cliff bug filed at
//! <https://github.com/kusari-oss/mikebom/issues/500>: mikebom's
//! file-tier walker skips every `.ipk` file it sees because the suffix
//! isn't in the recognized-artifact allowlist. Post-169, `.ipk` files
//! are handed to this reader; each well-formed archive emits one
//! `PackageDbEntry` per package with `pkg:opkg/<name>@<version>?arch=<arch>`
//! PURL identity.
//!
//! ## Format (verified 2026-07-06 during Phase 1 T001)
//!
//! Modern `opkg-utils/opkg-build` produces `.ipk` archives as gzipped
//! tarballs (**NOT** ar envelopes — that was a legacy pre-2015
//! convention). Structure:
//!
//! ```text
//! gzip( tar { ./debian-binary, ./control.tar.gz, ./data.tar.gz } )
//! ```
//!
//! Inside the outer tarball:
//! - `debian-binary` — text file containing `2.0\n` (format-version
//!   marker; ignored)
//! - `control.tar.gz` — inner gzipped tarball with `./control` +
//!   optional maintainer scripts (`postinst`, `prerm`, etc.); the
//!   `./control` file is the RFC-822 metadata source
//! - `data.tar.gz` — inner gzipped tarball with the on-target
//!   filesystem payload
//!
//! Empirically verified against OpenWrt 23.05.5 x86_64 base feed
//! fixtures at `mikebom-cli/tests/fixtures/ipk-files/`. Legacy ar-format
//! `.ipk` files (pre-2015 opkg-build) fall through to filename-only
//! parsing per research §R2b.
//!
//! ## Sibling reader
//!
//! `opkg.rs` (milestone 107) covers the INSTALLED-package-DB tier
//! (`/var/lib/opkg/status` + `info/*.control`). This module is the
//! archive-file tier (`tmp/deploy/ipk/*.ipk` build outputs and OpenWrt
//! feed downloads). Both readers share the RFC-822 stanza parser at
//! `control_file::parse_stanzas` (m107 refactor).

use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use mikebom_common::types::purl::{encode_purl_segment, Purl};

use super::control_file::{
    parse_depends_field_with_alternatives, parse_stanzas, DepsWithAlternatives,
};
use super::PackageDbEntry;

/// Depth cap for the walker per m069 rpm_file precedent. Deep-nested
/// `tmp/deploy/ipk/<arch>/**` layouts still resolve well within 12.
const MAX_WALK_DEPTH: usize = 12;

/// Reader configuration for the ipk archive-file scanner. Env-var
/// override plumbing (mirroring m069's `RpmReaderConfig`) is deferred
/// to a future task if a knob turns out to be needed at scan time.
#[derive(Debug, Clone)]
pub(crate) struct IpkReaderConfig {
    /// Cap on the uncompressed size of the inner `control.tar.gz` per
    /// FR-012. When exceeded, the ipk emits filename-only components
    /// with a `mikebom:archive-size-skipped` annotation instead of
    /// attempting full extraction. Default 16 MB — matches m069's
    /// rpm cap.
    pub(crate) max_control_size: u64,
}

impl Default for IpkReaderConfig {
    fn default() -> Self {
        Self {
            max_control_size: 16 * 1024 * 1024,
        }
    }
}

/// Errors surfaced by the ipk archive-file parser. Every variant is
/// paired with a tracing WARN at the call site per FR-006/FR-007.
/// The `read()` main loop matches on the variant to decide between
/// filename-fallback emission (US2) vs total skip.
#[derive(Debug)]
pub(crate) enum IpkParseError {
    /// Outer `gzip( tar )` envelope failed to parse — malformed
    /// gzip header, truncated tar stream, or non-tar body inside
    /// the gzip layer. Message provides a short reason.
    OuterMalformed(String),
    /// The outer tarball didn't contain `control.tar.gz`. Modern
    /// opkg-build archives always include it; this signals a
    /// non-conforming or hand-crafted `.ipk` file.
    ControlMissing,
    /// The `.ipk` filename didn't match the canonical
    /// `<name>_<version>_<arch>.ipk` layout — reserved for a future
    /// caller that wants explicit "filename fallback also failed"
    /// signaling. Today the filename fallback is handled by returning
    /// `None` from `parse_ipk_filename`, and the main read() loop
    /// converts that to a WARN + skip; this variant is retained for
    /// forward-compat.
    #[allow(dead_code)]
    FilenameNonConforming,
    /// The archive's first 8 bytes match the legacy ar magic
    /// (`!<arch>\n`) rather than modern gzip magic. Per research
    /// §R2b, mikebom treats this as "legacy pre-2015 opkg-build
    /// format" and falls through to filename-only emission with a
    /// tracing WARN. Adding a real ar-format parser is deferred
    /// until empirical evidence surfaces the pattern.
    LegacyArFormat,
    /// The inner `control.tar.gz`'s uncompressed size exceeded
    /// `IpkReaderConfig::max_control_size` per FR-012.
    ControlOversize { actual: u64, cap: u64 },
}

impl std::fmt::Display for IpkParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OuterMalformed(msg) => {
                write!(f, "outer gzip/tar envelope malformed: {msg}")
            }
            Self::ControlMissing => write!(f, "control.tar.gz missing"),
            Self::FilenameNonConforming => {
                write!(f, "filename does not match <name>_<version>_<arch>.ipk")
            }
            Self::LegacyArFormat => write!(
                f,
                "legacy ar-format .ipk (pre-2015 opkg-build); falling back to filename-only per research §R2b"
            ),
            Self::ControlOversize { actual, cap } => write!(
                f,
                "control.tar.gz uncompressed size {actual} bytes exceeds cap {cap} bytes"
            ),
        }
    }
}

/// Walk `<rootfs>` for `.ipk` files and emit one `PackageDbEntry` per
/// well-formed OR filename-parseable archive. Per FR-006/FR-007 every
/// skipped file fires a `tracing::warn!` line — zero silent drops.
///
/// Milestone 169 T033 (US5, FR-010): when `<rootfs>/etc/os-release` is
/// present, its `<ID>-<VERSION_ID>` tag is read once here and appended
/// to every emitted PURL as a `distro=` qualifier. When absent (headless
/// ipk-directory scan), the qualifier is omitted — no hardcoded default.
pub fn read(rootfs: &Path, config: &IpkReaderConfig) -> Vec<PackageDbEntry> {
    let distro_tag = super::super::os_release::read_distro_tag_from_rootfs(rootfs);
    let distro_tag_ref = distro_tag.as_deref();
    let mut out = Vec::new();
    for path in discover_ipk_files(rootfs) {
        match parse_ipk_file(&path, config, distro_tag_ref) {
            Ok(entry) => out.push(entry),
            Err(err) => {
                // FR-006 / FR-007: every skipped file surfaces a WARN.
                match err {
                    IpkParseError::FilenameNonConforming => {
                        tracing::warn!(
                            path = %path.display(),
                            "skipping .ipk file: filename does not match <name>_<version>_<arch>.ipk convention"
                        );
                    }
                    other => {
                        // US2 filename fallback: try to salvage a
                        // PURL from the filename even when the
                        // archive body is malformed / legacy /
                        // oversize.
                        match filename_fallback_entry(&path, distro_tag_ref) {
                            Some(entry) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    reason = %other,
                                    "salvaging .ipk via filename fallback"
                                );
                                out.push(entry);
                            }
                            None => {
                                tracing::warn!(
                                    path = %path.display(),
                                    reason = %other,
                                    "skipping .ipk file: parse failed and filename fallback unavailable"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Milestone 169 T028 (US4, FR-011): walk `<rootfs>` for `.ipk`
/// archives; extract each `data.tar.gz` payload's file-list; insert
/// every declared path into the binary walker's `claimed` set +
/// (on unix) also into `claimed_inodes` for cross-hardlink dedup.
/// Mirrors `dpkg::collect_claimed_paths` + `opkg::collect_claimed_paths`
/// (m107).
///
/// The scan pipeline uses this to prevent a duplicate emission
/// pattern where the same on-disk file (e.g. `/usr/bin/busybox`) would
/// emit as both `pkg:opkg/busybox@...` (from the ipk archive-file
/// reader here) AND `pkg:generic/busybox` (from binary-tier analysis
/// of the file at its final location on the rootfs).
///
/// Idempotent: safe to call before or after `read()`.
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut std::collections::HashSet<std::path::PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut std::collections::HashSet<(u64, u64)>,
) {
    for ipk_path in discover_ipk_files(rootfs) {
        let Ok(bytes) = std::fs::read(&ipk_path) else {
            continue;
        };
        // Skip legacy ar-format ipks per research §R2b (no ar parser
        // yet; the archive-file reader falls back to filename-only
        // emission which doesn't populate a claim set).
        if bytes.len() >= 8 && &bytes[..8] == b"!<arch>\n" {
            continue;
        }
        let outer_reader = GzDecoder::new(std::io::Cursor::new(&bytes));
        let mut outer_tar = tar::Archive::new(outer_reader);
        let Ok(entries) = outer_tar.entries() else {
            continue;
        };
        for entry_res in entries {
            let Ok(mut entry) = entry_res else { continue };
            let Ok(entry_path) = entry.path() else { continue };
            let is_data = entry_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s == "data.tar.gz")
                .unwrap_or(false);
            if !is_data {
                continue;
            }
            let inner = GzDecoder::new(&mut entry);
            let mut inner_tar = tar::Archive::new(inner);
            let Ok(inner_entries) = inner_tar.entries() else { continue };
            for inner_entry in inner_entries.flatten() {
                let Ok(p) = inner_entry.path() else { continue };
                let s = p.to_string_lossy();
                // Ipk data.tar.gz paths are conventionally rooted at
                // `./` (relative to the on-target filesystem root).
                // Convert to `<rootfs>/<rel>` so lookups against
                // `metadata()` work when the operator scans a mounted
                // rootfs.
                let cleaned = s.trim_start_matches("./").trim_start_matches('/');
                if cleaned.is_empty() {
                    continue;
                }
                let target = rootfs.join(cleaned);
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
    }
}

/// Discover every `.ipk` file under `rootfs` via the shared walker
/// (m114 `safe_walk`). Mirrors `rpm_file::discover_rpm_files`.
fn discover_ipk_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if root.is_file() {
        if is_ipk_candidate(root) {
            found.push(root.to_path_buf());
        }
        return found;
    }
    if !root.is_dir() {
        return found;
    }
    let empty = super::exclude_path::ExclusionSet::default();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let name = candidate.file_name().and_then(|s| s.to_str()).unwrap_or("");
            matches!(
                name,
                ".git" | "target" | "node_modules" | ".cargo" | "__pycache__" | ".venv"
            )
        },
        exclude_set: &empty,
    };
    crate::scan_fs::walk::safe_walk(root, &cfg, |path| {
        if path.is_file() && is_ipk_candidate(path) {
            found.push(path.to_path_buf());
        }
    });
    found
}

fn is_ipk_candidate(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("ipk"))
        .unwrap_or(false)
}

/// Parse one `.ipk` file into a `PackageDbEntry`. Returns
/// `Err(IpkParseError)` when the archive body fails; the caller
/// decides whether to invoke the filename fallback or skip entirely.
fn parse_ipk_file(
    path: &Path,
    config: &IpkReaderConfig,
    distro_tag: Option<&str>,
) -> Result<PackageDbEntry, IpkParseError> {
    // Format sniff: check magic bytes. Modern .ipk = gzip; legacy = ar.
    let bytes = std::fs::read(path).map_err(|e| {
        IpkParseError::OuterMalformed(format!("read failed: {e}"))
    })?;
    if bytes.len() >= 8 && &bytes[..8] == b"!<arch>\n" {
        return Err(IpkParseError::LegacyArFormat);
    }

    // Parse outer gzip( tar { ... } ) envelope.
    let outer_reader = GzDecoder::new(std::io::Cursor::new(&bytes));
    let mut outer_tar = tar::Archive::new(outer_reader);

    let mut control_tar_gz: Option<Vec<u8>> = None;
    let mut data_file_list: Vec<String> = Vec::new();

    let entries = outer_tar.entries().map_err(|e| {
        IpkParseError::OuterMalformed(format!("outer tar.entries() failed: {e}"))
    })?;

    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| {
            IpkParseError::OuterMalformed(format!("outer tar entry read failed: {e}"))
        })?;
        let entry_path = entry.path().map_err(|e| {
            IpkParseError::OuterMalformed(format!("outer tar entry path failed: {e}"))
        })?;
        let name = entry_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        match name {
            "control.tar.gz" => {
                let size = entry.size();
                if size > config.max_control_size {
                    return Err(IpkParseError::ControlOversize {
                        actual: size,
                        cap: config.max_control_size,
                    });
                }
                let mut buf = Vec::with_capacity(size as usize);
                entry.read_to_end(&mut buf).map_err(|e| {
                    IpkParseError::OuterMalformed(format!(
                        "control.tar.gz read failed: {e}"
                    ))
                })?;
                control_tar_gz = Some(buf);
            }
            "data.tar.gz" => {
                // Enumerate paths for FR-011/FR-017 skip-set feed (US4).
                let inner = GzDecoder::new(&mut entry);
                let mut inner_tar = tar::Archive::new(inner);
                if let Ok(inner_entries) = inner_tar.entries() {
                    for inner_entry in inner_entries.flatten() {
                        if let Ok(p) = inner_entry.path() {
                            let s = p.to_string_lossy();
                            // Drop leading "./" convention.
                            let cleaned = s.trim_start_matches("./").to_string();
                            if !cleaned.is_empty() {
                                data_file_list.push(cleaned);
                            }
                        }
                    }
                }
            }
            "debian-binary" | "" => {
                // Format-version marker or the "./" tar-root entry —
                // ignore both.
            }
            _ => {
                // Unknown extra entries in the outer tarball. Modern
                // .ipk files don't have these; ignore silently.
            }
        }
    }

    let control_bytes = control_tar_gz.ok_or(IpkParseError::ControlMissing)?;
    let control_text = extract_control_file(&control_bytes)?;

    build_entry_from_control(path, &control_text, &data_file_list, distro_tag)
        .ok_or_else(|| {
            IpkParseError::OuterMalformed(
                "control file present but Package/Version/Architecture fields missing"
                    .to_string(),
            )
        })
}

/// Extract the `./control` file body from a `control.tar.gz` byte slice.
/// Returns the file contents as a UTF-8 string.
fn extract_control_file(control_tar_gz_bytes: &[u8]) -> Result<String, IpkParseError> {
    let gz = GzDecoder::new(control_tar_gz_bytes);
    let mut ar = tar::Archive::new(gz);
    let entries = ar.entries().map_err(|e| {
        IpkParseError::OuterMalformed(format!("control.tar.gz entries() failed: {e}"))
    })?;
    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| {
            IpkParseError::OuterMalformed(format!("control.tar.gz entry failed: {e}"))
        })?;
        let entry_path = entry.path().map_err(|e| {
            IpkParseError::OuterMalformed(format!("control.tar.gz path failed: {e}"))
        })?;
        let name = entry_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name == "control" {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).map_err(|e| {
                IpkParseError::OuterMalformed(format!(
                    "control file read failed: {e}"
                ))
            })?;
            return Ok(buf);
        }
    }
    Err(IpkParseError::ControlMissing)
}

/// Build a `PackageDbEntry` from a parsed control file. Wires Q2
/// alternative-list Depends handling + m152 SPDX license
/// canonicalization + `mikebom:evidence-kind = "ipk-file"` (FR-009) +
/// `sbom_tier = "analyzed"` (archive-file tier per m106 convention).
fn build_entry_from_control(
    ipk_path: &Path,
    control_text: &str,
    data_file_list: &[String],
    distro_tag: Option<&str>,
) -> Option<PackageDbEntry> {
    let stanzas = parse_stanzas(control_text);
    let stanza = stanzas.first()?;
    let name = stanza.name()?.to_string();
    let version = stanza.version()?.to_string();
    let arch = stanza.architecture().unwrap_or("all").to_string();

    // Q2 clarification wired via T005 shared parser.
    let depends_field = stanza.depends().unwrap_or("");
    let DepsWithAlternatives {
        resolved: depends,
        alternates_by_source,
    } = parse_depends_field_with_alternatives(depends_field);

    // FR-008: license field routes through m152/153/154 SPDX pipeline.
    let licenses = match stanza.get("license") {
        Some(raw) if !raw.trim().is_empty() => {
            match mikebom_common::types::license::SpdxExpression::try_canonical(raw) {
                Ok(e) => vec![e],
                Err(_) => {
                    // m152 LicenseRef escape hatch — preserve non-
                    // canonical expressions as `LicenseRef-<hex>`
                    // (best-effort via lenient constructor).
                    match mikebom_common::types::license::SpdxExpression::new(raw) {
                        Ok(e) => vec![e],
                        Err(_) => Vec::new(),
                    }
                }
            }
        }
        _ => Vec::new(),
    };

    let purl = build_opkg_purl(&name, &version, &arch, distro_tag)?;

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("ipk-file".to_string()),
    );

    // Q2: emit alternate list as annotation on the source component.
    if !alternates_by_source.is_empty() {
        // JSON-serialize map keyed by first-alt name → fallback list.
        // Wire shape: {"pkg-a": ["pkg-b", ...], ...}
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

    // Data-file list feeds FR-011 binary-walker skip-set (US4 T028).
    // For now, store on the entry so the walker helper can consume it;
    // exact wire-up lands with the collect_claimed_paths function.
    if !data_file_list.is_empty() {
        let arr: Vec<serde_json::Value> = data_file_list
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect();
        extra_annotations.insert(
            "mikebom:ipk-file-list".to_string(),
            serde_json::Value::Array(arr),
        );
    }

    let maintainer = stanza
        .maintainer()
        .map(str::to_string)
        .filter(|s| !s.is_empty());

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: Some(arch),
        source_path: ipk_path.to_string_lossy().into_owned(),
        depends,
        maintainer,
        licenses,
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        buildinfo_status: None,
        evidence_kind: Some("ipk-file".to_string()),
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
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

/// Filename fallback per US2 (FR-006). Called when the archive body
/// fails to parse but the file itself exists. Constructs a
/// `PackageDbEntry` with name/version/arch derived from the
/// `<name>_<version>_<arch>.ipk` convention. Returns `None` if the
/// filename doesn't match.
fn filename_fallback_entry(
    path: &Path,
    distro_tag: Option<&str>,
) -> Option<PackageDbEntry> {
    let filename = path.file_name().and_then(|s| s.to_str())?;
    let (name, version, arch) = parse_ipk_filename(filename)?;
    let purl = build_opkg_purl(&name, &version, &arch, distro_tag)?;

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("ipk-file-filename-fallback".to_string()),
    );

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name,
        version,
        arch: Some(arch),
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
        source_type: None,
        buildinfo_status: None,
        evidence_kind: Some("ipk-file".to_string()),
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
        sbom_tier: Some("analyzed".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    })
}

/// Parse an ipk filename into `(name, version, arch)` triple per the
/// canonical `<name>_<version>_<arch>.ipk` convention. Returns None
/// for non-conforming filenames. Note: version may contain `-<release>`
/// (e.g., `1.36.1-r0`); we split only on `_`, never `-`.
fn parse_ipk_filename(filename: &str) -> Option<(String, String, String)> {
    // Strip `.ipk` extension.
    let stem = filename.strip_suffix(".ipk")?;
    // Split on `_` into exactly 3 segments — canonical layout.
    let parts: Vec<&str> = stem.split('_').collect();
    if parts.len() != 3 {
        return None;
    }
    let name = parts[0].to_string();
    let version = parts[1].to_string();
    let arch = parts[2].to_string();
    if name.is_empty() || version.is_empty() || arch.is_empty() {
        return None;
    }
    Some((name, version, arch))
}

/// Build a `pkg:opkg/<name>@<version>?arch=<arch>[&distro=<tag>]` PURL
/// per FR-004 + FR-010 + purl-spec's opkg type. Mirrors
/// `opkg::build_opkg_purl`.
///
/// Milestone 169 T033 (US5): `distro_tag` — when `Some`, is appended as
/// a `&distro=<tag>` qualifier (encoded via `encode_purl_segment`). When
/// `None`, the qualifier is omitted entirely. Qualifier ordering matches
/// PURL-spec's alphabetical convention (`arch` before `distro`).
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("ipk-files")
    }

    #[test]
    fn config_default_matches_m069_size_cap() {
        let c = IpkReaderConfig::default();
        assert_eq!(c.max_control_size, 16 * 1024 * 1024);
    }

    #[test]
    fn parse_error_display_covers_all_variants() {
        let variants = vec![
            IpkParseError::OuterMalformed("truncated".to_string()),
            IpkParseError::ControlMissing,
            IpkParseError::ControlOversize {
                actual: 20 * 1024 * 1024,
                cap: 16 * 1024 * 1024,
            },
            IpkParseError::FilenameNonConforming,
            IpkParseError::LegacyArFormat,
        ];
        for v in variants {
            let msg = format!("{v}");
            assert!(!msg.is_empty(), "Display for {v:?} produced empty string");
        }
    }

    // ------------------------------------------------------------
    // Milestone 169 T014 (US1) — well-formed ipk emits correct PURL
    // + evidence-kind.
    // ------------------------------------------------------------
    #[test]
    fn t014_well_formed_ipk_emits_correct_purl_and_evidence_kind() {
        let fixture_dir = workspace_fixture_dir();
        let cfg = IpkReaderConfig::default();
        let entries = read(&fixture_dir, &cfg);

        // 5 vendored ipks + fixture-README = 5 emitted components (the
        // README is not an .ipk file).
        assert_eq!(entries.len(), 5, "expected 5 components; got {}", entries.len());

        // Every emission carries the canonical evidence-kind + tier.
        for e in &entries {
            assert_eq!(e.evidence_kind.as_deref(), Some("ipk-file"));
            assert_eq!(e.sbom_tier.as_deref(), Some("analyzed"));
            assert!(e.purl.as_str().starts_with("pkg:opkg/"));
        }

        // Headline sample: 6in4 package resolves correctly.
        let six_in_four = entries
            .iter()
            .find(|e| e.name == "6in4")
            .expect("6in4 component present");
        assert_eq!(six_in_four.version, "28");
        assert_eq!(six_in_four.arch.as_deref(), Some("all"));
    }

    // ------------------------------------------------------------
    // Milestone 169 T015 (US1) — License field routes through SPDX
    // canonicalization / m152 LicenseRef fallback.
    // ------------------------------------------------------------
    #[test]
    fn t015_license_field_routes_through_spdx_canonical() {
        let fixture_dir = workspace_fixture_dir();
        let cfg = IpkReaderConfig::default();
        let entries = read(&fixture_dir, &cfg);

        // 6in4 declares `License: GPL-2.0` — must canonicalize.
        let six_in_four = entries
            .iter()
            .find(|e| e.name == "6in4")
            .expect("6in4 component present");
        assert!(
            !six_in_four.licenses.is_empty(),
            "6in4 must carry parsed License field; got: {:?}",
            six_in_four.licenses
        );
    }

    // ------------------------------------------------------------
    // Milestone 169 T016 (US1) — Depends field emits dep edges.
    // ------------------------------------------------------------
    #[test]
    fn t016_depends_field_emits_dep_edges() {
        let fixture_dir = workspace_fixture_dir();
        let cfg = IpkReaderConfig::default();
        let entries = read(&fixture_dir, &cfg);

        // 6in4 declares `Depends: libc, kmod-sit, uclient-fetch`.
        let six_in_four = entries
            .iter()
            .find(|e| e.name == "6in4")
            .expect("6in4 component present");
        assert!(
            six_in_four.depends.contains(&"libc".to_string()),
            "6in4 Depends must include libc; got: {:?}",
            six_in_four.depends
        );
        assert!(six_in_four.depends.contains(&"kmod-sit".to_string()));
        assert!(six_in_four.depends.contains(&"uclient-fetch".to_string()));
    }

    // ------------------------------------------------------------
    // Milestone 169 T017 (US1) — filename-fallback covered via a
    // synthesized malformed .ipk in a tempdir.
    // ------------------------------------------------------------
    #[test]
    fn t017_filename_fallback_on_malformed_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("busybox_1.36.1-r0_core2-64.ipk");
        // Write 8 bytes that are NOT gzip magic (0x1f 0x8b) — malformed.
        std::fs::write(&path, b"garbage_").unwrap();

        let cfg = IpkReaderConfig::default();
        let entries = read(tmp.path(), &cfg);

        assert_eq!(entries.len(), 1, "filename fallback should emit 1 entry");
        assert_eq!(entries[0].name, "busybox");
        assert_eq!(entries[0].version, "1.36.1-r0");
        assert_eq!(entries[0].arch.as_deref(), Some("core2-64"));
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("ipk-file-filename-fallback")
        );
    }

    // ------------------------------------------------------------
    // Milestone 169 T018 (US1) — filename non-conforming → skip
    // (no invented component; single WARN).
    // ------------------------------------------------------------
    #[test]
    fn t018_filename_non_conforming_skips_without_emitting() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("not-conforming-filename.ipk");
        std::fs::write(&path, b"garbage_").unwrap();

        let cfg = IpkReaderConfig::default();
        let entries = read(tmp.path(), &cfg);

        assert!(entries.is_empty(), "expected 0 emissions; got {}", entries.len());
    }

    // ------------------------------------------------------------
    // Milestone 169 T027 (US3 P2) — mixed well-formed + malformed +
    // non-conforming triage in a single scan. Verifies each file
    // routes to the correct code path independently and that per-
    // file failures do not affect peer emissions.
    // ------------------------------------------------------------
    #[test]
    fn t027_mixed_wellformed_malformed_nonconforming_all_bucketed_correctly() {
        let tmp = tempfile::tempdir().unwrap();

        // (a) Well-formed — copy a vendored real fixture.
        let src = workspace_fixture_dir().join("6in4_28_all.ipk");
        let src_bytes = std::fs::read(&src).unwrap();
        std::fs::write(tmp.path().join("6in4_28_all.ipk"), &src_bytes).unwrap();

        // (b) Malformed archive body BUT canonical filename → US2
        // filename fallback should emit.
        std::fs::write(
            tmp.path().join("busybox_1.36.1-r0_core2-64.ipk"),
            b"garbage_",
        )
        .unwrap();

        // (c) Filename non-conforming (no `_` separators) → skip
        // with WARN, NO emission.
        std::fs::write(
            tmp.path().join("bad-filename-no-underscores.ipk"),
            b"garbage_",
        )
        .unwrap();

        let cfg = IpkReaderConfig::default();
        let entries = read(tmp.path(), &cfg);

        // 2 emissions expected: the well-formed 6in4 + the filename
        // fallback busybox. The non-conforming file is skipped.
        assert_eq!(
            entries.len(),
            2,
            "expected 2 emissions (1 well-formed + 1 filename-fallback); got {}",
            entries.len()
        );

        // Well-formed emission carries full metadata (License, Depends).
        let wellformed = entries
            .iter()
            .find(|e| e.name == "6in4")
            .expect("6in4 must be present from well-formed archive");
        assert!(!wellformed.licenses.is_empty(), "well-formed emission has licenses");
        assert!(!wellformed.depends.is_empty(), "well-formed emission has deps");
        assert_eq!(
            wellformed
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("ipk-file"),
            "well-formed emission uses primary source-mechanism"
        );

        // Filename-fallback emission carries only filename-derived
        // identity (name/version/arch), NO license, NO deps, marker
        // annotation.
        let fallback = entries
            .iter()
            .find(|e| e.name == "busybox")
            .expect("busybox must be present from filename-fallback path");
        assert_eq!(fallback.version, "1.36.1-r0");
        assert_eq!(fallback.arch.as_deref(), Some("core2-64"));
        assert!(fallback.licenses.is_empty(), "fallback has no licenses");
        assert!(fallback.depends.is_empty(), "fallback has no deps");
        assert_eq!(
            fallback
                .extra_annotations
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("ipk-file-filename-fallback"),
            "fallback emission carries distinct source-mechanism marker"
        );

        // Confirm the non-conforming file was NOT emitted (defensive).
        assert!(
            !entries.iter().any(|e| e.name.contains("bad-filename")),
            "non-conforming file MUST NOT emit any component"
        );
    }

    // ------------------------------------------------------------
    // Milestone 169 T031 (US4 FR-011): collect_claimed_paths reads
    // the data.tar.gz payload's file list from each `.ipk` and feeds
    // it into the binary-walker skip-set — preventing duplicate
    // `pkg:generic/*` emissions on the same files.
    // ------------------------------------------------------------
    #[test]
    fn t031_collect_claimed_paths_feeds_binary_walker_skip_set() {
        let tmp = tempfile::tempdir().unwrap();
        // Copy a vendored real fixture so we know its data.tar.gz has
        // real file entries.
        let src = workspace_fixture_dir().join("6in4_28_all.ipk");
        let src_bytes = std::fs::read(&src).unwrap();
        std::fs::write(tmp.path().join("6in4_28_all.ipk"), &src_bytes).unwrap();

        let mut claimed: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        #[cfg(unix)]
        let mut claimed_inodes: std::collections::HashSet<(u64, u64)> =
            std::collections::HashSet::new();

        collect_claimed_paths(
            tmp.path(),
            &mut claimed,
            #[cfg(unix)]
            &mut claimed_inodes,
        );

        // The 6in4 ipk should declare at least one file path in its
        // data.tar.gz payload (typically installs a shell script or
        // ipk registration file). Post-collect the claim-set must be
        // non-empty.
        assert!(
            !claimed.is_empty(),
            "collect_claimed_paths must populate claim-set from data.tar.gz; got empty"
        );
        // Every claimed path is rooted at the scan tempdir per
        // implementation semantics.
        for p in &claimed {
            assert!(
                p.starts_with(tmp.path()),
                "claimed path {p:?} MUST be rooted at scan tempdir"
            );
        }
    }

    // ------------------------------------------------------------
    // Milestone 169 T034 (US5, FR-010): when `/etc/os-release` in the
    // scanned rootfs declares `ID=poky` + `VERSION_ID=5.0`, every
    // emitted PURL carries a `distro=poky-5.0` qualifier. Covers both
    // (a) archive-body parse path AND (b) filename-fallback path — the
    // qualifier propagation lives at PURL-construction time so both
    // paths must attach it.
    // ------------------------------------------------------------
    #[test]
    fn t034_distro_qualifier_propagates_from_etc_os_release_poky() {
        let tmp = tempfile::tempdir().unwrap();
        // Write /etc/os-release declaring poky 5.0 (matches the
        // synthetic opkg-installed-db fixture convention).
        let etc = tmp.path().join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(
            etc.join("os-release"),
            "ID=poky\nVERSION_ID=\"5.0\"\n",
        )
        .unwrap();

        // Case (a): well-formed archive (vendored fixture copy).
        let src = workspace_fixture_dir().join("6in4_28_all.ipk");
        let src_bytes = std::fs::read(&src).unwrap();
        std::fs::write(tmp.path().join("6in4_28_all.ipk"), &src_bytes).unwrap();

        // Case (b): filename-fallback (malformed archive w/ conforming name).
        std::fs::write(
            tmp.path().join("busybox_1.36.1-r0_core2-64.ipk"),
            b"garbage_",
        )
        .unwrap();

        let cfg = IpkReaderConfig::default();
        let entries = read(tmp.path(), &cfg);

        assert!(!entries.is_empty(), "at least one emission expected");
        for e in &entries {
            let s = e.purl.as_str();
            assert!(
                s.contains("&distro=poky-5.0"),
                "PURL {s:?} MUST carry `&distro=poky-5.0` qualifier per FR-010"
            );
            // Qualifier ordering: arch precedes distro (alphabetical).
            let arch_pos = s.find("arch=").expect("arch= present");
            let distro_pos = s.find("distro=").expect("distro= present");
            assert!(
                arch_pos < distro_pos,
                "PURL qualifier ordering MUST be arch before distro; got {s:?}"
            );
        }
    }

    // ------------------------------------------------------------
    // Milestone 169 T035 (US5, FR-010): bare ipk-directory (no
    // `/etc/os-release` in the scan target) → PURLs omit `distro=`
    // entirely — no hardcoded default.
    // ------------------------------------------------------------
    #[test]
    fn t035_no_distro_qualifier_when_os_release_absent() {
        // Vendored fixture directory has no etc/ → no os-release.
        let fixture_dir = workspace_fixture_dir();
        let cfg = IpkReaderConfig::default();
        let entries = read(&fixture_dir, &cfg);

        assert!(!entries.is_empty(), "at least one emission expected");
        for e in &entries {
            let s = e.purl.as_str();
            assert!(
                !s.contains("distro="),
                "PURL {s:?} MUST NOT carry `distro=` when os-release is absent"
            );
        }
    }
}
