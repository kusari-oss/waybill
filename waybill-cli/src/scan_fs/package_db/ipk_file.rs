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
use waybill_common::types::purl::{encode_purl_segment, Purl};

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
    /// Milestone 187 (#543) — the archive's first 8 bytes match the
    /// ar magic (`!<arch>\n`) AND the [`parse_ar_archive`] helper
    /// failed to enumerate members. Reasons: truncated header,
    /// non-ASCII size field, or size overrun. Ships as the primary
    /// failure class for post-2015 opkg-build ipks (which mikebom
    /// pre-m187 misclassified as "legacy ar-format" and never
    /// attempted to parse).
    ArMalformed(String),
    /// Milestone 187 (#543) — renamed from `LegacyArFormat`. The
    /// pre-2015 `gzip(tar)` outer-envelope fallback path was tried
    /// (after ar-format probe failed at magic-check) AND ALSO
    /// failed. Ships as the SECONDARY failure class for ipks that
    /// are neither ar-format nor well-formed gzip-tar.
    LegacyGzipTarFallbackFailed(String),
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
            Self::ArMalformed(reason) => write!(
                f,
                "ar-format archive malformed: {reason}"
            ),
            Self::LegacyGzipTarFallbackFailed(reason) => write!(
                f,
                "pre-2015 gzip(tar) outer envelope parse failed: {reason}"
            ),
            Self::ControlOversize { actual, cap } => write!(
                f,
                "control.tar.gz uncompressed size {actual} bytes exceeds cap {cap} bytes"
            ),
        }
    }
}

// -----------------------------------------------------------------
// Milestone 187 (#543) — ar-format primary parser.
// -----------------------------------------------------------------

/// One entry from an ar-format archive (BSD ar spec) — a member name +
/// its raw data body. m187 US1 (#543) — the primary parse type
/// returned by [`parse_ar_archive`].
///
/// Members are returned in the order they appear in the archive.
/// Caller scans for named members (`control.tar.gz`, `data.tar.gz`,
/// `debian-binary`) without assuming any specific ordering per
/// contracts/ipk-parse-pipeline.md §Branch 1.
#[derive(Debug)]
struct ArMember {
    /// Member name, decoded from the 16-byte name field. Trailing
    /// slash (BSD ar convention for null-padding) is stripped. Never
    /// empty (the parser rejects empty names).
    name: String,
    /// Raw member data (unpadded). Length matches the header's
    /// decimal `size` field.
    data: Vec<u8>,
}

/// Milestone 187 (#543) — errors from [`parse_ar_archive`]. Distinct
/// from [`IpkParseError`] so the caller can distinguish "malformed
/// container" vs downstream failure classes.
#[derive(Debug)]
enum ArError {
    /// The 60-byte header couldn't be read (archive body ended
    /// mid-header).
    TruncatedHeader,
    /// The header's size field contained non-decimal bytes.
    NonAsciiSizeField,
    /// The size field claims more bytes than remain in the archive.
    SizeOverrunsBody,
}

impl std::fmt::Display for ArError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TruncatedHeader => {
                write!(f, "truncated ar header (archive ended mid-member)")
            }
            Self::NonAsciiSizeField => {
                write!(f, "non-ASCII decimal size field in ar header")
            }
            Self::SizeOverrunsBody => {
                write!(f, "ar member size overruns archive body")
            }
        }
    }
}

/// Milestone 187 (#543) — parse a BSD ar-format archive into its
/// member list.
///
/// Format:
///   * 8-byte magic: `!<arch>\n`
///   * per member: 60-byte header (16-byte name + 12 mtime + 6 uid +
///     6 gid + 8 mode + 10-byte decimal size + 2-byte end marker
///     `` `\n ``) followed by data padded to even byte boundary.
///
/// opkg-build produces short member names (`debian-binary`,
/// `control.tar.gz`, `data.tar.gz`) that fit the 16-byte name field
/// directly — no GNU ar long-name-table (`//`, `#1/N` inline-length)
/// handling needed. If a member name contains `/`, everything from
/// the first `/` onward is stripped (BSD convention: `/` is the
/// name-terminator character).
///
/// Caller MUST have already verified the 8-byte magic.
fn parse_ar_archive(bytes: &[u8]) -> Result<Vec<ArMember>, ArError> {
    const MAGIC_LEN: usize = 8;
    const HEADER_LEN: usize = 60;
    if bytes.len() < MAGIC_LEN {
        return Err(ArError::TruncatedHeader);
    }
    let mut cursor = MAGIC_LEN;
    let mut members = Vec::new();
    while cursor < bytes.len() {
        // Skip optional 1-byte padding aligning to even byte boundary.
        if !cursor.is_multiple_of(2) {
            cursor += 1;
        }
        // Handle the case where padding advanced us past end-of-body.
        if cursor >= bytes.len() {
            break;
        }
        // Require a full 60-byte header remaining.
        if bytes.len() - cursor < HEADER_LEN {
            return Err(ArError::TruncatedHeader);
        }
        let header = &bytes[cursor..cursor + HEADER_LEN];
        // Name: first 16 bytes, whitespace-padded per BSD ar. Strip
        // trailing spaces and slash (BSD name-terminator convention).
        let raw_name = std::str::from_utf8(&header[..16])
            .map_err(|_| ArError::NonAsciiSizeField)?;
        let name = raw_name
            .split('/')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_string();
        // Size: bytes 48..58, ASCII decimal, whitespace-padded.
        let size_str = std::str::from_utf8(&header[48..58])
            .map_err(|_| ArError::NonAsciiSizeField)?
            .trim();
        let size: usize = size_str
            .parse::<u64>()
            .map_err(|_| ArError::NonAsciiSizeField)?
            .try_into()
            .map_err(|_| ArError::SizeOverrunsBody)?;
        let data_start = cursor + HEADER_LEN;
        let data_end = data_start
            .checked_add(size)
            .ok_or(ArError::SizeOverrunsBody)?;
        if data_end > bytes.len() {
            return Err(ArError::SizeOverrunsBody);
        }
        let data = bytes[data_start..data_end].to_vec();
        if !name.is_empty() {
            members.push(ArMember { name, data });
        }
        cursor = data_end;
    }
    Ok(members)
}

// -----------------------------------------------------------------
// Milestone 187 (#542) — filename-fallback arch-source disambiguation.
// -----------------------------------------------------------------

/// Milestone 187 US2 (#542) — the origin of the `?arch=` PURL
/// qualifier when the filename-fallback path is taken. Emitted as
/// `mikebom:arch-source` property on the component per FR-013.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchSource {
    /// Parent-directory name matched the filename's `_<arch>` suffix
    /// per FR-010 suffix-match gate.
    ParentDirectory,
    /// Fallback rsplit-based heuristic (no parent-dir agreement).
    FilenameHeuristic,
}

impl ArchSource {
    /// Wire-format string for the `mikebom:arch-source` property.
    fn as_wire_str(self) -> &'static str {
        match self {
            Self::ParentDirectory => "parent-directory",
            Self::FilenameHeuristic => "filename-heuristic",
        }
    }
}

/// Milestone 187 US2 (#542) — `parse_ipk_filename` return type
/// carrying the arch-source signal for FR-013 diagnostic property
/// emission.
#[derive(Debug)]
struct ParsedFilename {
    name: String,
    version: String,
    arch: String,
    arch_source: ArchSource,
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
        // Milestone 187 (#543) — ar-format primary branch. Extracts
        // the data.tar[.gz] member's file-list and inserts into the
        // claim set (mirrors the gzip-tar branch below). The pre-m187
        // `continue` short-circuit that skipped ar-format ipks
        // entirely was removed here — post-m187 both formats
        // contribute to the claim set for m169 US4 dedup.
        if bytes.len() >= 8 && &bytes[..8] == b"!<arch>\n" {
            let Ok(members) = parse_ar_archive(&bytes) else {
                continue;
            };
            let Some(data_member) = members
                .iter()
                .find(|m| m.name == "data.tar.gz" || m.name == "data.tar")
            else {
                continue;
            };
            let file_list =
                list_data_tar_paths(&data_member.data, data_member.name.ends_with(".gz"));
            for cleaned in file_list {
                let target = rootfs.join(&cleaned);
                #[cfg(unix)]
                {
                    if let Ok(meta) = std::fs::metadata(&target) {
                        use std::os::unix::fs::MetadataExt;
                        claimed_inodes.insert((meta.dev(), meta.ino()));
                    }
                }
                claimed.insert(target);
            }
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
///
/// Milestone 187 (#543) — the ar-format is now the PRIMARY parse
/// path (per FR-001). Detection is at magic-byte level (`!<arch>\n`
/// at offset 0). On ar-format success, mikebom emits
/// `mikebom:source-mechanism = "ipk-file-archive-extraction"` +
/// `mikebom:arch-source = "control-file"`. Pre-2015
/// `gzip(tar)`-outer-envelope ipks are still handled by the
/// SECONDARY path (unchanged wire format;
/// `mikebom:source-mechanism = "ipk-file"`, no arch-source
/// property per FR-014 / SC-005 byte-identity guarantee).
fn parse_ipk_file(
    path: &Path,
    config: &IpkReaderConfig,
    distro_tag: Option<&str>,
) -> Result<PackageDbEntry, IpkParseError> {
    let bytes = std::fs::read(path).map_err(|e| {
        IpkParseError::OuterMalformed(format!("read failed: {e}"))
    })?;

    // Milestone 187 (#543) — Branch 1: ar-format primary path.
    if bytes.len() >= 8 && &bytes[..8] == b"!<arch>\n" {
        let members = parse_ar_archive(&bytes)
            .map_err(|e| IpkParseError::ArMalformed(e.to_string()))?;
        return parse_ipk_from_ar_members(path, &members, config, distro_tag);
    }

    // Branch 2: pre-2015 gzip(tar) outer envelope (legacy path).
    let outer_reader = GzDecoder::new(std::io::Cursor::new(&bytes));
    let mut outer_tar = tar::Archive::new(outer_reader);

    let mut control_tar_gz: Option<Vec<u8>> = None;
    let mut data_file_list: Vec<String> = Vec::new();

    let entries = outer_tar.entries().map_err(|e| {
        IpkParseError::LegacyGzipTarFallbackFailed(format!("outer tar.entries() failed: {e}"))
    })?;

    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| {
            IpkParseError::LegacyGzipTarFallbackFailed(format!("outer tar entry read failed: {e}"))
        })?;
        let entry_path = entry.path().map_err(|e| {
            IpkParseError::LegacyGzipTarFallbackFailed(format!("outer tar entry path failed: {e}"))
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
                    IpkParseError::LegacyGzipTarFallbackFailed(format!(
                        "control.tar.gz read failed: {e}"
                    ))
                })?;
                control_tar_gz = Some(buf);
            }
            "data.tar.gz" => {
                let inner = GzDecoder::new(&mut entry);
                let mut inner_tar = tar::Archive::new(inner);
                if let Ok(inner_entries) = inner_tar.entries() {
                    for inner_entry in inner_entries.flatten() {
                        if let Ok(p) = inner_entry.path() {
                            let s = p.to_string_lossy();
                            let cleaned = s.trim_start_matches("./").to_string();
                            if !cleaned.is_empty() {
                                data_file_list.push(cleaned);
                            }
                        }
                    }
                }
            }
            "debian-binary" | "" => {}
            _ => {}
        }
    }

    let control_bytes = control_tar_gz.ok_or(IpkParseError::ControlMissing)?;
    let control_text = extract_control_from_gzipped_tar(&control_bytes)?;

    build_entry_from_control(
        path,
        &control_text,
        &data_file_list,
        distro_tag,
        "ipk-file",
        false, // legacy path: no arch-source property (SC-005 byte-identity)
    )
    .ok_or_else(|| {
        IpkParseError::LegacyGzipTarFallbackFailed(
            "control file present but Package/Version/Architecture fields missing"
                .to_string(),
        )
    })
}

/// Milestone 187 (#543) — ar-format branch of [`parse_ipk_file`].
/// Scans the ar member list for `control.tar[.gz]` + `data.tar[.gz]`,
/// extracts the control file, and calls `build_entry_from_control`
/// with the archive-extraction source-mechanism value + arch-source
/// property flag set.
fn parse_ipk_from_ar_members(
    path: &Path,
    members: &[ArMember],
    config: &IpkReaderConfig,
    distro_tag: Option<&str>,
) -> Result<PackageDbEntry, IpkParseError> {
    // Scan for control member (gzipped or uncompressed).
    let control_member = members
        .iter()
        .find(|m| m.name == "control.tar.gz" || m.name == "control.tar")
        .ok_or(IpkParseError::ControlMissing)?;
    if (control_member.data.len() as u64) > config.max_control_size {
        return Err(IpkParseError::ControlOversize {
            actual: control_member.data.len() as u64,
            cap: config.max_control_size,
        });
    }
    let control_gzipped = control_member.name.ends_with(".gz");
    let control_text = extract_control_file_from_bytes(&control_member.data, control_gzipped)?;

    // Scan for data member (optional; only used for file-list walk).
    let data_file_list = members
        .iter()
        .find(|m| m.name == "data.tar.gz" || m.name == "data.tar")
        .map(|m| list_data_tar_paths(&m.data, m.name.ends_with(".gz")))
        .unwrap_or_default();

    // debian-binary is optional per spec.md Edge Cases; log if absent
    // or if content differs from `2.0\n`.
    match members.iter().find(|m| m.name == "debian-binary") {
        None => tracing::warn!(
            path = %path.display(),
            "ar-format ipk missing debian-binary member; proceeding with control extraction"
        ),
        Some(db) if db.data != b"2.0\n" => tracing::warn!(
            path = %path.display(),
            debian_binary = ?String::from_utf8_lossy(&db.data),
            "ar-format ipk debian-binary content is not the standard `2.0\\n`; proceeding"
        ),
        _ => {}
    }

    build_entry_from_control(
        path,
        &control_text,
        &data_file_list,
        distro_tag,
        "ipk-file-archive-extraction",
        true, // ar path: emit mikebom:arch-source = "control-file"
    )
    .ok_or_else(|| {
        IpkParseError::ArMalformed(
            "control file present but Package/Version/Architecture fields missing"
                .to_string(),
        )
    })
}

/// Milestone 187 (#543) — walk an inner tar (optionally gzipped) and
/// return its file-list. Shared between the ar-format branch (T010)
/// and the m169-era `collect_claimed_paths` gzip-tar branch (T014).
fn list_data_tar_paths(data_bytes: &[u8], gzipped: bool) -> Vec<String> {
    let mut out = Vec::new();
    if gzipped {
        let gz = GzDecoder::new(std::io::Cursor::new(data_bytes));
        let mut tar = tar::Archive::new(gz);
        if let Ok(entries) = tar.entries() {
            for entry in entries.flatten() {
                if let Ok(p) = entry.path() {
                    let s = p.to_string_lossy();
                    let cleaned = s.trim_start_matches("./").to_string();
                    if !cleaned.is_empty() {
                        out.push(cleaned);
                    }
                }
            }
        }
    } else {
        let mut tar = tar::Archive::new(std::io::Cursor::new(data_bytes));
        if let Ok(entries) = tar.entries() {
            for entry in entries.flatten() {
                if let Ok(p) = entry.path() {
                    let s = p.to_string_lossy();
                    let cleaned = s.trim_start_matches("./").to_string();
                    if !cleaned.is_empty() {
                        out.push(cleaned);
                    }
                }
            }
        }
    }
    out
}

/// Milestone 187 (#543) — extract the `./control` file from a
/// `control.tar[.gz]` byte slice. Both compressed (`.tar.gz`) and
/// uncompressed (`.tar`) inner containers are supported per spec.md
/// Edge Cases.
fn extract_control_file_from_bytes(
    control_bytes: &[u8],
    gzipped: bool,
) -> Result<String, IpkParseError> {
    if gzipped {
        extract_control_from_gzipped_tar(control_bytes)
    } else {
        extract_control_from_plain_tar(control_bytes)
    }
}

/// Legacy helper — retained as-is for the gzip(tar) code path.
/// Extracts the `./control` file body from a `control.tar.gz` byte
/// slice. Returns the file contents as a UTF-8 string.
fn extract_control_from_gzipped_tar(control_tar_gz_bytes: &[u8]) -> Result<String, IpkParseError> {
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

/// Milestone 187 (#543) — extract the `./control` file from an
/// uncompressed `control.tar` byte slice.
fn extract_control_from_plain_tar(control_tar_bytes: &[u8]) -> Result<String, IpkParseError> {
    let mut ar = tar::Archive::new(std::io::Cursor::new(control_tar_bytes));
    let entries = ar.entries().map_err(|e| {
        IpkParseError::OuterMalformed(format!("control.tar entries() failed: {e}"))
    })?;
    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| {
            IpkParseError::OuterMalformed(format!("control.tar entry failed: {e}"))
        })?;
        let entry_path = entry.path().map_err(|e| {
            IpkParseError::OuterMalformed(format!("control.tar path failed: {e}"))
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
///
/// Milestone 187 (#543) — `source_mechanism_value` selects the
/// `mikebom:source-mechanism` property value: `"ipk-file"` for the
/// legacy `gzip(tar)` path (unchanged wire format), or
/// `"ipk-file-archive-extraction"` for the new ar-format primary
/// path. `emit_arch_source` controls whether the
/// `mikebom:arch-source = "control-file"` property is emitted (true
/// for ar path; false for legacy path per FR-014 / SC-005
/// byte-identity guarantee).
fn build_entry_from_control(
    ipk_path: &Path,
    control_text: &str,
    data_file_list: &[String],
    distro_tag: Option<&str>,
    source_mechanism_value: &str,
    emit_arch_source: bool,
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

    // FR-008 (m152): license field routes through SPDX canonicalization
    // + LicenseRef fallback. Milestone 190 (#550): pre-normalize BitBake
    // operators (`&`, `&&`, `|`, `||`) to SPDX (`AND`, `OR`) BEFORE
    // try_canonical so real-world Yocto license expressions (e.g.,
    // `GPL-2.0-only & MIT`) canonicalize instead of falling to the
    // LicenseRef-<hex> hashed form.
    let licenses = match stanza.get("license") {
        Some(raw) if !raw.trim().is_empty() => {
            let normalized = normalize_bitbake_license_operators(raw);
            match waybill_common::types::license::SpdxExpression::try_canonical(&normalized) {
                Ok(e) => vec![e],
                Err(_) => {
                    // m152 LicenseRef escape hatch — preserve non-
                    // canonical expressions as `LicenseRef-<hex>`
                    // (best-effort via lenient constructor).
                    match waybill_common::types::license::SpdxExpression::new(&normalized) {
                        Ok(e) => vec![e],
                        Err(_) => Vec::new(),
                    }
                }
            }
        }
        _ => Vec::new(),
    };

    // Milestone 190 (#552): epoch extraction. Debian/opkg version
    // strings encode epoch as `<digits>:<upstream-version>-<release>`;
    // the purl-spec ` opkg` type carries epoch as a `?epoch=<N>`
    // qualifier, NOT inline in the version. Mirror the rpm reader
    // pattern at `rpm_file.rs:397-411`.
    let (epoch, version) = parse_opkg_version_with_epoch(&version);
    let purl = build_opkg_purl(&name, &version, &arch, distro_tag, epoch)?;

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String(source_mechanism_value.to_string()),
    );
    // Milestone 187 US1 (FR-013) — emit arch-source ONLY for the ar
    // primary path. The legacy gzip-tar path deliberately does NOT
    // emit this property to preserve FR-014 / SC-005 byte-identity.
    if emit_arch_source {
        extra_annotations.insert(
            "mikebom:arch-source".to_string(),
            serde_json::Value::String("control-file".to_string()),
        );
    }

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
        requirement_ranges: Vec::new(),
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
///
/// Milestone 187 US2 (#542) — consults the ipk's IMMEDIATE parent
/// directory name for the authoritative `?arch=` source per FR-010
/// suffix-match gate. On match, emits `mikebom:arch-source =
/// "parent-directory"`. On no-match, falls back to the pre-m187
/// rsplit-based filename heuristic + emits `mikebom:arch-source =
/// "filename-heuristic"` per FR-012 / FR-013.
fn filename_fallback_entry(
    path: &Path,
    distro_tag: Option<&str>,
) -> Option<PackageDbEntry> {
    let filename = path.file_name().and_then(|s| s.to_str())?;
    let parent_dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());
    let parsed = parse_ipk_filename(filename, parent_dir_name)?;
    // Milestone 190 (#552, FR-012): epoch on the filename-fallback path.
    // The filename may encode epoch as `pkg_<epoch>:<version>-<release>_<arch>.ipk`
    // (pre-2015 opkg-build style). Extract before PURL construction so
    // the emitted PURL carries `?epoch=<N>` instead of embedding the
    // epoch inline in the version segment.
    let (epoch, naked_version) = parse_opkg_version_with_epoch(&parsed.version);
    let purl = build_opkg_purl(&parsed.name, &naked_version, &parsed.arch, distro_tag, epoch)?;

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        Default::default();
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::Value::String("ipk-file-filename-fallback".to_string()),
    );
    // Milestone 187 US2 (FR-013) — arch-source diagnostic property.
    extra_annotations.insert(
        "mikebom:arch-source".to_string(),
        serde_json::Value::String(parsed.arch_source.as_wire_str().to_string()),
    );

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: parsed.name,
        version: naked_version,
        arch: Some(parsed.arch),
        source_path: path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
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

/// Milestone 187 US2 (#542) — parent-directory arch-source
/// disambiguation. Returns `Some(prefix)` IFF `filename_no_ext` ends
/// with `_<parent_dir_name>` byte-for-byte case-sensitive. The
/// returned prefix is everything BEFORE the matched suffix, feeding
/// the existing `<name>_<version>` LEFT-split logic in
/// [`parse_ipk_filename`].
///
/// Rationale per spec.md Clarifications Q1: the Yocto convention
/// emits filenames with the arch as the last `_`-delimited segment
/// AND places each ipk in an `<arch>/` directory. This suffix-match
/// gate identifies parent-dir-as-arch without misfiring on loose-file
/// layouts (`~/downloads/foo_1.0_all.ipk` — parent `downloads` does
/// NOT match filename suffix `_all` → returns `None` → caller falls
/// back to the rsplit heuristic).
fn parent_dir_arch_match<'a>(
    filename_no_ext: &'a str,
    parent_dir_name: &str,
) -> Option<&'a str> {
    let suffix = format!("_{parent_dir_name}");
    filename_no_ext.strip_suffix(&suffix)
}

/// Parse an ipk filename into a [`ParsedFilename`] per the canonical
/// `<name>_<version>_<arch>.ipk` convention.
///
/// Milestone 187 US2 (#542) — when `parent_dir_name` is `Some` AND
/// [`parent_dir_arch_match`] succeeds, the parent-dir name is used
/// as the authoritative arch (per FR-010) and the version is
/// preserved verbatim (per FR-011 — no arch-vs-version competition
/// for the last `_` in the filename). This closes the m185
/// underscore-in-arch regression documented at #542.
///
/// Milestone 185 US1 (#538) — the version field itself may legally
/// contain `_` when produced by BitBake's `SRCPV` expansion for
/// git-sourced upstream recipes (e.g., Yocto kernel modules with
/// versions like `6.6.127+git0+45f69741c7_70af2998be-r0`). The
/// legacy rsplit path preserves the version-internal underscore by
/// splitting asymmetrically:
///   1. `split_once('_')` from the LEFT → peels off the name at the
///      first underscore (name never contains `_`).
///   2. `rsplit_once('_')` from the RIGHT on the remainder → peels
///      off the arch at the last underscore; version is everything
///      in between (may include underscores).
///
/// The rsplit path is entered when `parent_dir_name` is `None` OR
/// when the parent-dir suffix-match check returns `None`.
fn parse_ipk_filename(
    filename: &str,
    parent_dir_name: Option<&str>,
) -> Option<ParsedFilename> {
    // Strip `.ipk` extension.
    let stem = filename.strip_suffix(".ipk")?;

    // Milestone 187 US2 (#542) — try parent-dir suffix match first.
    if let Some(parent) = parent_dir_name {
        if let Some(prefix) = parent_dir_arch_match(stem, parent) {
            // Split prefix on FIRST `_` for <name>_<version>. If no
            // `_` present (e.g. `foo` with parent `qemux86_64` for
            // filename `foo_qemux86_64.ipk`), the ipk has no version
            // — this is a hand-crafted / non-conforming shape; fall
            // through to the rsplit heuristic per spec.md edge case.
            if let Some((name, version)) = prefix.split_once('_') {
                if !name.is_empty() && !version.is_empty() {
                    return Some(ParsedFilename {
                        name: name.to_string(),
                        version: version.to_string(),
                        arch: parent.to_string(),
                        arch_source: ArchSource::ParentDirectory,
                    });
                }
            }
        }
    }

    // Fall through: legacy rsplit heuristic (pre-m187 behavior).
    let (name, rest) = stem.split_once('_')?;
    let (version, arch) = rest.rsplit_once('_')?;
    if name.is_empty() || version.is_empty() || arch.is_empty() {
        return None;
    }
    Some(ParsedFilename {
        name: name.to_string(),
        version: version.to_string(),
        arch: arch.to_string(),
        arch_source: ArchSource::FilenameHeuristic,
    })
}

/// Build a `pkg:opkg/<name>@<version>?arch=<arch>[&distro=<tag>][&epoch=<N>]` PURL
/// per FR-004 + FR-010 + purl-spec's opkg type. Mirrors
/// `opkg::build_opkg_purl`.
///
/// Milestone 169 T033 (US5): `distro_tag` — when `Some`, is appended as
/// a `&distro=<tag>` qualifier (encoded via `encode_purl_segment`). When
/// `None`, the qualifier is omitted entirely.
///
/// Milestone 190 (#552): `epoch` — when `Some(v)` with `v != 0`, appended
/// as `&epoch=<v>` qualifier. When `None` or `Some(0)`, the qualifier is
/// omitted (matches purl-spec convention where `epoch=0` is implicit and
/// mirrors `rpm_file.rs:410-411`).
///
/// Qualifier ordering follows the PURL-spec alphabetical convention:
/// `arch` (a) < `distro` (d) < `epoch` (e). Empty-epoch-and-empty-distro
/// path is byte-identical to pre-m190 output — enforces FR-011 / SC-006.
fn build_opkg_purl(
    name: &str,
    version: &str,
    arch: &str,
    distro_tag: Option<&str>,
    epoch: Option<u32>,
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
    // Milestone 190: epoch qualifier — omitted when None or Some(0)
    // per purl-spec convention (see rpm_file.rs:410-411 for the
    // canonical reference). Emitted after distro to preserve alphabetical
    // key ordering.
    if let Some(v) = epoch {
        if v != 0 {
            purl_str.push_str(&format!("&epoch={v}"));
        }
    }
    Purl::new(&purl_str).ok()
}

/// Milestone 190 (#550): normalize BitBake license operators to their
/// SPDX equivalents so the raw ipk `License:` field can be passed to
/// `SpdxExpression::try_canonical` without falling to the LicenseRef
/// hashed fallback.
///
/// Real-world Yocto recipes use BitBake's operator dialect (`&`, `|`)
/// which the `spdx` crate's expression parser does NOT recognize as
/// valid SPDX operators. Substitute BEFORE canonicalization so all
/// three format emitters (CDX 1.6, SPDX 2.3, SPDX 3) transitively
/// receive an SPDX-canonical value through the existing shared
/// `component.licenses` field.
///
/// Ordering invariant: long-form (`&&`, `||`) MUST be substituted
/// before single-form (`&`, `|`) to avoid partial-token overlap.
/// The four `str::replace` calls below encode this order; do NOT
/// reorder without re-reading spec §Q1 + research §R1.
///
/// Idempotent: applying twice equals applying once (since the SPDX
/// operators `AND`/`OR` contain no `&`/`|` characters and the
/// whitespace-collapsing step is stable).
fn normalize_bitbake_license_operators(raw: &str) -> String {
    let substituted = raw
        .replace("&&", " AND ")
        .replace("||", " OR ")
        .replace('&', " AND ")
        .replace('|', " OR ");
    // Collapse runs of whitespace to single spaces + trim ends so the
    // emitted expression is clean regardless of input spacing. Safe
    // for SPDX expressions (no significant multi-whitespace tokens).
    substituted.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Milestone 190 (#552): parse the raw ipk `Version:` field into
/// (optional epoch, naked-version) per the Debian/opkg convention
/// where `<digits>:<version>-<release>` embeds the epoch inline.
///
/// Returns `(None, raw.to_string())` when no `<digits>:` prefix is
/// present, preserving byte-identity for non-epoch inputs (SC-006).
/// Non-digit prefixes (e.g., `abc:1.0-r0`) are treated as literal
/// version text; only ASCII-digit prefixes match the epoch pattern.
///
/// Multi-colon input (`1:2.0-r0:beta`): only the FIRST `<digits>:`
/// prefix is treated as epoch; the rest of the string is preserved
/// verbatim in the returned naked-version.
///
/// Guards against `u32` overflow: on `parse::<u32>` failure the input
/// is treated as if it had no epoch prefix.
fn parse_opkg_version_with_epoch(raw: &str) -> (Option<u32>, String) {
    // Locate the first `:` and verify the prefix is all ASCII digits.
    if let Some(colon_pos) = raw.find(':') {
        let (prefix, rest) = raw.split_at(colon_pos);
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(v) = prefix.parse::<u32>() {
                // rest starts with `:`; skip it.
                return (Some(v), rest[1..].to_string());
            }
        }
    }
    (None, raw.to_string())
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
            IpkParseError::ArMalformed("truncated header".to_string()),
            IpkParseError::LegacyGzipTarFallbackFailed("outer envelope corrupt".to_string()),
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

    // ── Milestone 185 US1 — parse_ipk_filename multi-underscore fix (#538) ──

    #[test]
    fn parse_ipk_filename_canonical_2underscore_still_parses() {
        // Row 1 of contracts/parser-decision-matrix.md — canonical
        // 2-underscore case MUST be byte-identical to pre-m185.
        let out = parse_ipk_filename("test-pkg_1.0-r0_all.ipk", None).unwrap();
        assert_eq!(out.name, "test-pkg");
        assert_eq!(out.version, "1.0-r0");
        assert_eq!(out.arch, "all");
        assert_eq!(out.arch_source, ArchSource::FilenameHeuristic);
    }

    #[test]
    fn parse_ipk_filename_multi_underscore_version_now_parses() {
        // Row 3 — the m185 fix. Pre-m185 returned None because
        // split('_').len() == 4.
        let out = parse_ipk_filename("test-pkg_1.0+git0+abc_def-r0_all.ipk", None).unwrap();
        assert_eq!(out.name, "test-pkg");
        assert_eq!(out.version, "1.0+git0+abc_def-r0");
        assert_eq!(out.arch, "all");
        assert_eq!(out.arch_source, ArchSource::FilenameHeuristic);
    }

    #[test]
    fn parse_ipk_filename_yocto_kernel_module_shape() {
        // Row 4 — real BitBake SRCPV shape from issue #538 reproducer.
        // With parent=None, the legacy rsplit heuristic runs and
        // produces the m185 KNOWN LIMITATION (arch=`64`, version tail
        // absorbs `_qemux86`). Post-m187 US2 fixes this via parent-dir
        // suffix match — verified separately below.
        let filename = "kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk";
        let out = parse_ipk_filename(filename, None).unwrap();
        assert_eq!(out.name, "kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard");
        assert_eq!(out.version, "6.6.127+git0+45f69741c7_70af2998be-r0_qemux86");
        assert_eq!(out.arch, "64");
        assert_eq!(out.arch_source, ArchSource::FilenameHeuristic);
    }

    #[test]
    fn parse_ipk_filename_no_ipk_suffix_still_none() {
        // Row 7 — extension guard preserved.
        assert!(parse_ipk_filename("test-pkg_1.0-r0_all", None).is_none());
        assert!(parse_ipk_filename("test-pkg_1.0-r0_all.deb", None).is_none());
        assert!(parse_ipk_filename("just-a-file.txt", None).is_none());
    }

    #[test]
    fn parse_ipk_filename_no_underscores_still_none() {
        // Rows 5-6 — need at least 2 underscores to extract 3 fields.
        assert!(parse_ipk_filename("no-underscores.ipk", None).is_none());
        assert!(parse_ipk_filename("single_underscore.ipk", None).is_none());
    }

    #[test]
    fn parse_ipk_filename_empty_field_still_none() {
        // Rows 8-10 — empty-field guard preserved.
        assert!(parse_ipk_filename("_1.0-r0_all.ipk", None).is_none()); // empty name
        assert!(parse_ipk_filename("test-pkg__all.ipk", None).is_none()); // empty version
        assert!(parse_ipk_filename("test-pkg_1.0-r0_.ipk", None).is_none()); // empty arch
    }

    // ── Milestone 187 T008 (#543 US1) — ar-format parser unit tests ──

    /// Helper: build a BSD ar member header (60 bytes) for a given
    /// name + size. Fills mtime/uid/gid/mode with `0` (space-padded).
    fn ar_header(name: &str, size: u64) -> Vec<u8> {
        let mut h = vec![b' '; 60];
        let name_bytes = name.as_bytes();
        let name_slot = &mut h[..16];
        name_slot[..name_bytes.len()].copy_from_slice(name_bytes);
        // mtime (12), uid (6), gid (6), mode (8) — filled with `0` +
        // spaces so the field is well-formed but content is trivial.
        let mtime = "0           ".as_bytes(); // 12 chars
        h[16..28].copy_from_slice(mtime);
        let uid_gid_mode = "0     0     0       "; // 6+6+8
        h[28..48].copy_from_slice(uid_gid_mode.as_bytes());
        let size_str = format!("{size:<10}");
        h[48..58].copy_from_slice(size_str.as_bytes());
        h[58..60].copy_from_slice(b"`\n");
        h
    }

    /// Helper: assemble a full ar archive (magic + n members).
    fn ar_archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = b"!<arch>\n".to_vec();
        for (name, data) in members {
            out.extend_from_slice(&ar_header(name, data.len() as u64));
            out.extend_from_slice(data);
            if !data.len().is_multiple_of(2) {
                out.push(b'\n');
            }
        }
        out
    }

    #[test]
    fn parse_ar_archive_extracts_three_members() {
        let bytes = ar_archive(&[
            ("debian-binary", b"2.0\n"),
            ("control.tar.gz", b"MOCK-CONTROL-BYTES"),
            ("data.tar.gz", b"MOCK-DATA-BYTES"),
        ]);
        let members = parse_ar_archive(&bytes).unwrap();
        assert_eq!(members.len(), 3);
        assert_eq!(members[0].name, "debian-binary");
        assert_eq!(members[0].data, b"2.0\n");
        assert_eq!(members[1].name, "control.tar.gz");
        assert_eq!(members[1].data, b"MOCK-CONTROL-BYTES");
        assert_eq!(members[2].name, "data.tar.gz");
        assert_eq!(members[2].data, b"MOCK-DATA-BYTES");
    }

    #[test]
    fn parse_ar_archive_tolerates_missing_debian_binary() {
        // Only control + data — some vendor builds omit debian-binary.
        let bytes = ar_archive(&[
            ("control.tar.gz", b"CONTROL"),
            ("data.tar.gz", b"DATA"),
        ]);
        let members = parse_ar_archive(&bytes).unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m.name == "control.tar.gz"));
        assert!(members.iter().any(|m| m.name == "data.tar.gz"));
    }

    #[test]
    fn parse_ar_archive_handles_uncompressed_inner_tar() {
        // control.tar (no .gz) + data.tar (no .gz) — the m187 edge
        // case for uncompressed inner tar variants.
        let bytes = ar_archive(&[
            ("control.tar", b"CTRL"),
            ("data.tar", b"DATA"),
        ]);
        let members = parse_ar_archive(&bytes).unwrap();
        assert!(members.iter().any(|m| m.name == "control.tar"));
        assert!(members.iter().any(|m| m.name == "data.tar"));
    }

    #[test]
    fn parse_ar_archive_rejects_truncated_header() {
        // 8-byte magic + 20 bytes = truncated first header.
        let mut bytes = b"!<arch>\n".to_vec();
        bytes.extend_from_slice(&[b' '; 20]);
        assert!(matches!(
            parse_ar_archive(&bytes),
            Err(ArError::TruncatedHeader)
        ));
    }

    #[test]
    fn parse_ar_archive_rejects_non_ascii_size() {
        let mut header = ar_header("control.tar.gz", 0);
        // Overwrite the size field (bytes 48..58) with non-ASCII bytes.
        header[48] = 0xFF;
        header[49] = 0xFE;
        let mut bytes = b"!<arch>\n".to_vec();
        bytes.extend_from_slice(&header);
        assert!(matches!(
            parse_ar_archive(&bytes),
            Err(ArError::NonAsciiSizeField)
        ));
    }

    // ── Milestone 187 T016 (#542 US2) — parent-dir suffix match unit tests ──

    #[test]
    fn parent_dir_arch_match_matches_yocto_convention() {
        assert_eq!(
            parent_dir_arch_match("foo_1.0-r0_qemux86_64", "qemux86_64"),
            Some("foo_1.0-r0")
        );
    }

    #[test]
    fn parent_dir_arch_match_rejects_loose_layout() {
        // Filename `foo_1.0_all` in parent `downloads` — parent name
        // doesn't match the filename's arch suffix → no match →
        // caller falls through to rsplit heuristic.
        assert_eq!(parent_dir_arch_match("foo_1.0_all", "downloads"), None);
    }

    #[test]
    fn parent_dir_arch_match_handles_multi_underscore_arch() {
        assert_eq!(
            parent_dir_arch_match("foo_1.0_powerpc_e500v2", "powerpc_e500v2"),
            Some("foo_1.0")
        );
        assert_eq!(
            parent_dir_arch_match("foo_1.0_mips_i6400", "mips_i6400"),
            Some("foo_1.0")
        );
    }

    #[test]
    fn parse_ipk_filename_uses_parent_dir_when_suffix_matches() {
        // Full round-trip: Yocto kernel-module shape with parent dir
        // consulting — the #542 fix. The version's internal `_` is
        // preserved because arch is stripped as a suffix, not
        // rsplit-competed.
        let filename = "kernel-6.6.127_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk";
        let out = parse_ipk_filename(filename, Some("qemux86_64")).unwrap();
        assert_eq!(out.name, "kernel-6.6.127");
        assert_eq!(out.version, "6.6.127+git0+45f69741c7_70af2998be-r0");
        assert_eq!(out.arch, "qemux86_64");
        assert_eq!(out.arch_source, ArchSource::ParentDirectory);
    }

    #[test]
    fn parse_ipk_filename_falls_back_to_rsplit_when_no_parent_match() {
        // Loose-file layout: parent doesn't match filename suffix →
        // rsplit heuristic runs → arch is filename's last _-segment.
        let out = parse_ipk_filename("foo_1.0_arm.ipk", Some("downloads")).unwrap();
        assert_eq!(out.name, "foo");
        assert_eq!(out.version, "1.0");
        assert_eq!(out.arch, "arm");
        assert_eq!(out.arch_source, ArchSource::FilenameHeuristic);
    }

    // ── Milestone 190 (#550) — normalize_bitbake_license_operators ──

    #[test]
    fn normalize_bitbake_single_and_becomes_spdx_and() {
        let out = normalize_bitbake_license_operators("GPL-2.0-only & MIT");
        assert!(
            out.contains(" AND "),
            "expected ` AND ` in normalized output; got: {out:?}"
        );
        assert!(!out.contains('&'), "single & must be substituted: {out:?}");
        // Round-trip through try_canonical to prove SPDX validity.
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out)
            .expect("normalized expression must canonicalize");
        assert_eq!(canon.as_str(), "GPL-2.0-only AND MIT");
    }

    #[test]
    fn normalize_bitbake_single_or_becomes_spdx_or() {
        let out = normalize_bitbake_license_operators("MIT | Apache-2.0");
        assert!(out.contains(" OR "), "expected ` OR `; got: {out:?}");
        assert!(!out.contains('|'), "single | must be substituted: {out:?}");
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        assert_eq!(canon.as_str(), "MIT OR Apache-2.0");
    }

    #[test]
    fn normalize_bitbake_double_and_becomes_spdx_and() {
        let out = normalize_bitbake_license_operators("MIT && Apache-2.0");
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        assert_eq!(canon.as_str(), "MIT AND Apache-2.0");
        assert!(!out.contains("&&"));
        assert!(!out.contains('&'));
    }

    #[test]
    fn normalize_bitbake_double_or_becomes_spdx_or() {
        let out = normalize_bitbake_license_operators("MIT || Apache-2.0");
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        assert_eq!(canon.as_str(), "MIT OR Apache-2.0");
        assert!(!out.contains("||"));
        assert!(!out.contains('|'));
    }

    #[test]
    fn normalize_bitbake_no_operator_is_noop_on_operators() {
        // No BitBake operators → normalization changes nothing observable
        // for a single-operand license.
        let out = normalize_bitbake_license_operators("MIT");
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        assert_eq!(canon.as_str(), "MIT");
    }

    #[test]
    fn normalize_bitbake_no_whitespace_still_normalizes() {
        // Real-world Yocto recipe shape: no whitespace around the
        // operator. The helper inserts whitespace around SPDX operators
        // so try_canonical can tokenize.
        let out = normalize_bitbake_license_operators("MIT&&Apache-2.0");
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        assert_eq!(canon.as_str(), "MIT AND Apache-2.0");
    }

    #[test]
    fn normalize_bitbake_grouped_expression_preserves_parens() {
        let out = normalize_bitbake_license_operators("(GPL-2.0-only & MIT) | Apache-2.0");
        // Grouping preserved verbatim — only operator tokens change.
        assert!(out.contains('('));
        assert!(out.contains(')'));
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        // spdx crate normalizes the canonical form; either form of
        // grouping is acceptable as long as it parses cleanly.
        assert!(canon.as_str().contains("GPL-2.0-only"));
        assert!(canon.as_str().contains("MIT"));
        assert!(canon.as_str().contains("Apache-2.0"));
    }

    #[test]
    fn normalize_bitbake_with_clause_preserved() {
        // `WITH` is SPDX-native and MUST survive normalization.
        let out = normalize_bitbake_license_operators(
            "GPL-2.0-only WITH Classpath-exception-2.0 & MIT",
        );
        let canon = waybill_common::types::license::SpdxExpression::try_canonical(&out).unwrap();
        assert!(canon.as_str().contains("WITH"));
        assert!(canon.as_str().contains("Classpath-exception-2.0"));
        assert!(canon.as_str().contains("MIT"));
    }

    #[test]
    fn normalize_bitbake_is_idempotent() {
        let once = normalize_bitbake_license_operators("GPL-2.0-only & MIT");
        let twice = normalize_bitbake_license_operators(&once);
        assert_eq!(once, twice);
    }

    // ── Milestone 190 (#552) — parse_opkg_version_with_epoch ──

    #[test]
    fn parse_opkg_version_no_epoch_prefix() {
        // Baseline: unchanged input → no epoch, verbatim naked version.
        assert_eq!(
            parse_opkg_version_with_epoch("2.0-r0"),
            (None, "2.0-r0".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_nonzero_epoch() {
        assert_eq!(
            parse_opkg_version_with_epoch("1:2.0-r0"),
            (Some(1), "2.0-r0".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_zero_epoch_preserves_zero() {
        // Zero-epoch is preserved by the parser; the PURL builder is
        // responsible for treating Some(0) as "no qualifier" per
        // purl-spec convention. Test at build_opkg_purl level.
        assert_eq!(
            parse_opkg_version_with_epoch("0:1.0-r0"),
            (Some(0), "1.0-r0".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_multi_colon_only_first_treated_as_epoch() {
        assert_eq!(
            parse_opkg_version_with_epoch("1:2.0-r0:beta"),
            (Some(1), "2.0-r0:beta".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_non_digit_prefix_is_not_epoch() {
        // Non-digit prefix → parser must NOT treat it as epoch.
        assert_eq!(
            parse_opkg_version_with_epoch("abc:1.0-r0"),
            (None, "abc:1.0-r0".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_leading_colon_is_not_epoch() {
        // Colon with no prefix → not an epoch.
        assert_eq!(
            parse_opkg_version_with_epoch(":1.0-r0"),
            (None, ":1.0-r0".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_overflow_falls_back_to_no_epoch() {
        // Value that overflows u32 — must not panic; return input verbatim.
        let raw = "99999999999999999999:1.0-r0";
        assert_eq!(
            parse_opkg_version_with_epoch(raw),
            (None, raw.to_string())
        );
    }

    #[test]
    fn parse_opkg_version_empty_input() {
        assert_eq!(
            parse_opkg_version_with_epoch(""),
            (None, "".to_string())
        );
    }

    #[test]
    fn parse_opkg_version_large_valid_epoch() {
        // u32::MAX is a valid epoch value per purl-spec (no upper bound
        // in practice, but our type limits us; guard boundary condition).
        let raw = format!("{}:1.0", u32::MAX);
        assert_eq!(
            parse_opkg_version_with_epoch(&raw),
            (Some(u32::MAX), "1.0".to_string())
        );
    }

    // ── Milestone 190 — build_opkg_purl epoch qualifier ──

    #[test]
    fn build_opkg_purl_omits_epoch_when_none() {
        let p = build_opkg_purl("netbase", "6.4", "all", None, None).unwrap();
        assert_eq!(p.as_str(), "pkg:opkg/netbase@6.4?arch=all");
    }

    #[test]
    fn build_opkg_purl_omits_epoch_when_zero() {
        // FR-010 — Some(0) MUST NOT emit &epoch=0 per purl-spec.
        let p = build_opkg_purl("netbase", "6.4", "all", None, Some(0)).unwrap();
        assert_eq!(p.as_str(), "pkg:opkg/netbase@6.4?arch=all");
    }

    #[test]
    fn build_opkg_purl_emits_epoch_qualifier_when_nonzero() {
        // FR-009 — Some(1) → &epoch=1 qualifier appended alphabetically
        // after arch=.
        let p = build_opkg_purl("netbase", "6.4", "all", None, Some(1)).unwrap();
        assert_eq!(p.as_str(), "pkg:opkg/netbase@6.4?arch=all&epoch=1");
    }

    #[test]
    fn build_opkg_purl_alphabetical_qualifier_ordering() {
        // arch < distro < epoch alphabetically. Verifies research §R4
        // ordering claim.
        let p = build_opkg_purl("netbase", "6.4", "all", Some("nodistro-1"), Some(3)).unwrap();
        assert_eq!(
            p.as_str(),
            "pkg:opkg/netbase@6.4?arch=all&distro=nodistro-1&epoch=3"
        );
    }

    #[test]
    fn build_opkg_purl_no_epoch_no_distro_byte_identical_to_pre_m190() {
        // FR-011 / SC-006 — the (name, version, arch, None, None) path
        // MUST produce the exact same output as the pre-m190
        // signature. Regression guard for byte-identity of existing
        // goldens.
        let p = build_opkg_purl("6in4", "28", "all", None, None).unwrap();
        assert_eq!(p.as_str(), "pkg:opkg/6in4@28?arch=all");
    }
}
