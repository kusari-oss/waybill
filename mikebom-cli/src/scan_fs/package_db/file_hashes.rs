//! Compute per-file content hashes for dpkg-installed packages.
//!
//! Two functions, two cost profiles:
//! - [`hash_package_files`] — the default. Walks every path dpkg's
//!   `<pkg>.list` manifest claims the package owns, opens each one,
//!   stream-hashes the contents with SHA-256, and stitches the per-file
//!   hashes into a deterministic Merkle root. Cost is proportional to
//!   installed package size (~3-5 s on `debian:bookworm-slim`).
//! - [`hash_md5sums_only`] — the `--no-deep-hash` fast path. Just
//!   SHA-256s the dpkg-provided `.md5sums` file as-is. Microseconds per
//!   package; preserves the per-package identity claim but doesn't
//!   detect on-disk tampering and emits no per-file occurrences.
//!
//! Both produce a [`ContentHash`] suitable for `ResolvedComponent.hashes`.
//! Only the deep variant fills `ResolvedComponent.occurrences`.

use std::collections::HashMap;
use std::path::Path;

use mikebom_common::resolution::FileOccurrence;
use mikebom_common::types::hash::ContentHash;
use sha2::{Digest, Sha256};

use crate::trace::hasher::{sha256_file_hex, sha256_hex};

/// Maximum bytes to hash per individual installed file. Mirrors the
/// scan-mode artefact-walker cap. Files larger than this are skipped
/// (rare for dpkg-installed packages — they're almost always smaller).
const MAX_PER_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Deep-hash every file `<rootfs>/var/lib/dpkg/info/<pkg>[:<arch>].list`
/// claims the package owns. Returns the per-file occurrences and a
/// component-level Merkle root over them.
///
/// Multi-arch dpkg installs suffix each package's info files with
/// `:<arch>` (e.g. `libc6:arm64.list`). The `arch` parameter lets the
/// caller supply the architecture from the parsed status stanza; we
/// try `<pkg>.list` first (Architecture: all packages) and fall back
/// to `<pkg>:<arch>.list` when the plain form is absent.
///
/// Files that disappear between install time and scan time (configs the
/// admin removed, tmpfile entries that were never created, etc.) are
/// silently skipped. Directories listed in `.list` are ignored —
/// they're not hashable content and dpkg lists them only for ownership.
pub fn hash_package_files(
    rootfs: &Path,
    pkg_name: &str,
    arch: Option<&str>,
) -> (Vec<FileOccurrence>, Option<ContentHash>) {
    // Path-list source priority (milestone 038):
    //   1. <pkg>.list (legacy dpkg layout — preferred when available)
    //   2. paths derived from <pkg>.md5sums second column
    //      (status.d/ layout for distroless / chainguard / Bazel-built
    //      minimal images; .md5sums lines are `<32-hex>  <relpath>`).
    // If neither yields a non-empty list, return early with no
    // occurrences — same posture as before milestone 038.
    let list_text: String = if let Some(text) = read_info_file(rootfs, pkg_name, arch, "list") {
        text
    } else if let Some(text) = read_info_file(rootfs, pkg_name, arch, "md5sums") {
        // Synthesize a list-shaped string by stripping the leading md5
        // hash and whitespace from each line. dpkg's `.md5sums` uses
        // relative paths (no leading `/`); legacy `.list` uses absolute
        // paths. Prepend `/` so the resulting `FileOccurrence.location`
        // matches the legacy convention regardless of which source
        // produced it — keeps SBOM output stable across layouts.
        text.lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, char::is_whitespace);
                parts.next()?; // discard the md5 hex
                let rest = parts.next()?.trim_start();
                if rest.is_empty() {
                    None
                } else if rest.starts_with('/') {
                    Some(rest.to_string())
                } else {
                    Some(format!("/{rest}"))
                }
            })
            .collect::<Vec<String>>()
            .join("\n")
    } else {
        return (Vec::new(), None);
    };

    let md5_lookup = read_md5sums(rootfs, pkg_name, arch);

    let mut occurrences: Vec<FileOccurrence> = Vec::new();
    for raw in list_text.lines() {
        let path_in_pkg = raw.trim();
        if path_in_pkg.is_empty() || path_in_pkg == "/." {
            continue;
        }
        // dpkg's .list paths are absolute (`/usr/bin/jq`); resolve
        // against the rootfs so the same code works for "scan / on a
        // live host" and "scan an extracted image rootfs."
        let abs = rootfs.join(path_in_pkg.trim_start_matches('/'));
        let Ok(meta) = abs.symlink_metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_PER_FILE_BYTES {
            tracing::debug!(
                path = %abs.display(),
                size = meta.len(),
                "skipping oversized file in deep hash"
            );
            continue;
        }
        let sha256 = match sha256_file_hex(&abs, MAX_PER_FILE_BYTES) {
            Ok(h) => h,
            Err(e) => {
                tracing::debug!(path = %abs.display(), error = %e, "could not hash file");
                continue;
            }
        };
        // dpkg's .md5sums uses paths relative to root with no leading
        // slash (`usr/bin/jq`); look up via the same form.
        let md5_key = path_in_pkg.trim_start_matches('/');
        let md5_legacy = md5_lookup.get(md5_key).cloned();
        // Store the dpkg-declared path (the canonical deployed-filesystem
        // path), not the absolute path on the scanner. That keeps the
        // Merkle root stable across scans of the same package regardless
        // of where the rootfs was extracted.
        occurrences.push(FileOccurrence {
            location: path_in_pkg.to_string(),
            sha256,
            md5_legacy,
            apk_sha1: None,
            rpm_file_digest: None,
        });
    }

    let root = compute_merkle_root(&occurrences);
    (occurrences, root)
}

/// `--no-deep-hash` fast path. Reads `<rootfs>/var/lib/dpkg/info/<pkg>[:<arch>].md5sums`
/// and SHA-256s the raw bytes as the package's "fingerprint." Empty
/// `Option<ContentHash>` when the file is absent (some packages don't
/// ship one — e.g. essential virtual packages).
pub fn hash_md5sums_only(
    rootfs: &Path,
    pkg_name: &str,
    arch: Option<&str>,
) -> Option<ContentHash> {
    let bytes = read_info_file_bytes(rootfs, pkg_name, arch, "md5sums")?;
    let hex = sha256_hex(&bytes);
    ContentHash::sha256(&hex).ok()
}

/// Read a `<pkg>[:<arch>].<ext>` companion file as UTF-8.
///
/// Lookup chain (milestone 038):
///
/// 1. `var/lib/dpkg/info/<pkg>.<ext>` — legacy single-file dpkg layout.
/// 2. `var/lib/dpkg/info/<pkg>:<arch>.<ext>` — multi-arch dpkg variant.
/// 3. `var/lib/dpkg/status.d/<pkg>.<ext>` — per-package layout used by
///    distroless / chainguard / Bazel-built minimal images. The
///    `status.d/` directory ships `<pkg>.md5sums` companion files
///    alongside the per-package stanza files; this fallback lets the
///    same code path consume them.
///
/// Returns `None` if no candidate exists. Legacy paths take priority
/// over `status.d/` to keep mixed-layout images deterministic per
/// research.md R5.
fn read_info_file(
    rootfs: &Path,
    pkg_name: &str,
    arch: Option<&str>,
    ext: &str,
) -> Option<String> {
    let info = rootfs.join("var/lib/dpkg/info");
    let plain = info.join(format!("{pkg_name}.{ext}"));
    if let Ok(text) = std::fs::read_to_string(&plain) {
        return Some(text);
    }
    if let Some(a) = arch.filter(|a| !a.is_empty()) {
        let archy = info.join(format!("{pkg_name}:{a}.{ext}"));
        if let Ok(text) = std::fs::read_to_string(&archy) {
            return Some(text);
        }
    }
    let status_d = rootfs
        .join("var/lib/dpkg/status.d")
        .join(format!("{pkg_name}.{ext}"));
    if let Ok(text) = std::fs::read_to_string(&status_d) {
        return Some(text);
    }
    None
}

/// Raw-bytes variant of [`read_info_file`] for files we hash directly
/// (`.md5sums` on the fast path). Same lookup chain.
fn read_info_file_bytes(
    rootfs: &Path,
    pkg_name: &str,
    arch: Option<&str>,
    ext: &str,
) -> Option<Vec<u8>> {
    let info = rootfs.join("var/lib/dpkg/info");
    let plain = info.join(format!("{pkg_name}.{ext}"));
    if let Ok(bytes) = std::fs::read(&plain) {
        return Some(bytes);
    }
    if let Some(a) = arch.filter(|a| !a.is_empty()) {
        let archy = info.join(format!("{pkg_name}:{a}.{ext}"));
        if let Ok(bytes) = std::fs::read(&archy) {
            return Some(bytes);
        }
    }
    let status_d = rootfs
        .join("var/lib/dpkg/status.d")
        .join(format!("{pkg_name}.{ext}"));
    if let Ok(bytes) = std::fs::read(&status_d) {
        return Some(bytes);
    }
    None
}

// ---- Milestone 039: apk per-file deep-hashing -------------------------

/// Deep-hash every file the apk installed-db claims `pkg_name` owns.
/// Returns the per-file occurrences and a component-level Merkle
/// root over them — matching the shape produced by
/// [`hash_package_files`] for dpkg components, so downstream emitters
/// don't have to discriminate.
///
/// `files` is the rootfs-relative path list extracted by
/// [`super::apk::read_file_lists`] for this package. apk paths come
/// in unprefixed (`usr/bin/foo`); we resolve to absolute form
/// (`/usr/bin/foo`) for `FileOccurrence.location` to match the
/// legacy convention used by the dpkg path.
///
/// Files that disappear between install and scan time are silently
/// skipped (rare; would typically indicate config files removed
/// during image build). Oversized files (> [`MAX_PER_FILE_BYTES`])
/// are skipped with a debug log.
///
/// Unlike the dpkg path, no MD5 cross-reference is emitted —
/// apk's analogous `Z:` line carries SHA-1; that cross-reference
/// is plumbed through alongside the SHA-256 (milestone 040 US2).
/// Each [`ApkFileEntry`] passed in carries an optional SHA-1 from
/// the package's stanza; when present, it surfaces as
/// `apk_sha1` on the resulting [`FileOccurrence`] (and from there
/// into the per-occurrence `additionalContext` JSON-string at
/// emission time).
pub fn hash_apk_package_files(
    rootfs: &Path,
    files: &[super::apk::ApkFileEntry],
) -> (Vec<FileOccurrence>, Option<ContentHash>) {
    let mut occurrences: Vec<FileOccurrence> = Vec::new();
    for entry in files {
        let rel = entry.path.trim_start_matches('/');
        if rel.is_empty() {
            continue;
        }
        let abs = rootfs.join(rel);
        let Ok(meta) = abs.symlink_metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_PER_FILE_BYTES {
            tracing::debug!(
                path = %abs.display(),
                size = meta.len(),
                "skipping oversized file in apk deep hash"
            );
            continue;
        }
        let sha256 = match sha256_file_hex(&abs, MAX_PER_FILE_BYTES) {
            Ok(h) => h,
            Err(e) => {
                tracing::debug!(path = %abs.display(), error = %e, "could not hash apk file");
                continue;
            }
        };
        // Store the absolute deployed-filesystem path (matches the
        // legacy convention used on the dpkg side — keeps SBOM
        // consumers from having to detect ecosystem-specific
        // location prefixes).
        occurrences.push(FileOccurrence {
            location: format!("/{rel}"),
            sha256,
            md5_legacy: None,
            apk_sha1: entry.sha1.clone(),
            rpm_file_digest: None,
        });
    }

    let root = compute_merkle_root(&occurrences);
    (occurrences, root)
}

// ---- Milestone 040 US3: rpm per-file deep-hashing -----------------------

/// Deep-hash every file the rpm package claims (via
/// HeaderBlob `BASENAMES` / `DIRNAMES` / `DIRINDEXES`).
/// Mirrors [`hash_apk_package_files`].
///
/// `files` is the path-and-digest list yielded by
/// [`super::rpm::read_file_lists`]. Each entry's `path` is
/// rpm-on-disk absolute (`/usr/bin/bash`); the optional `digest`
/// is the upstream-provided cross-ref in algorithm-prefixed
/// form (e.g. `"sha256:abc..."`) and threads through to the
/// resulting `FileOccurrence.rpm_file_digest` (milestone 041).
///
/// Files that disappear between install and scan time are
/// silently skipped (rare; would typically indicate config files
/// removed during image build). Oversized files (> [`MAX_PER_FILE_BYTES`])
/// are skipped with a debug log.
pub fn hash_rpm_package_files(
    rootfs: &Path,
    files: &[super::rpm::RpmFileListEntry],
) -> (Vec<FileOccurrence>, Option<ContentHash>) {
    let mut occurrences: Vec<FileOccurrence> = Vec::new();
    for entry in files {
        let trimmed = entry.path.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Normalize to an absolute deployed path for `location`
        // (rpm sources already provide them this way) and to a
        // rootfs-relative form for the actual filesystem read.
        let absolute_location = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            format!("/{trimmed}")
        };
        let rel = absolute_location.trim_start_matches('/');
        let abs = rootfs.join(rel);
        let Ok(meta) = abs.symlink_metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_PER_FILE_BYTES {
            tracing::debug!(
                path = %abs.display(),
                size = meta.len(),
                "skipping oversized file in rpm deep hash"
            );
            continue;
        }
        let sha256 = match sha256_file_hex(&abs, MAX_PER_FILE_BYTES) {
            Ok(h) => h,
            Err(e) => {
                tracing::debug!(path = %abs.display(), error = %e, "could not hash rpm file");
                continue;
            }
        };
        occurrences.push(FileOccurrence {
            location: absolute_location,
            sha256,
            md5_legacy: None,
            apk_sha1: None,
            rpm_file_digest: entry.digest.clone(),
        });
    }

    let root = compute_merkle_root(&occurrences);
    (occurrences, root)
}

/// `--no-deep-hash` fast path for rpm. SHA-256s a deterministic
/// per-package fingerprint derived from the rpmdb-resident metadata.
///
/// Implementation: compute SHA-256 of a sorted, newline-terminated
/// concatenation of the package's claimed file paths. This gives a
/// stable per-package identity claim that survives image-rebuild
/// jitter (the rpm HeaderBlob's bytes change with metadata-only
/// re-installs even when the file list is identical, so hashing the
/// path-set directly is more useful than hashing the blob).
///
/// Returns `None` when the named package isn't found in the rpmdb
/// or has no associated file paths.
pub fn hash_rpm_db_only(rootfs: &Path, pkg_name: &str) -> Option<ContentHash> {
    let map = super::rpm::read_file_lists(rootfs);
    let files = map.get(pkg_name)?;
    if files.is_empty() {
        return None;
    }
    let mut paths: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();
    paths.sort();
    let mut payload = String::new();
    for p in &paths {
        payload.push_str(p);
        payload.push('\n');
    }
    let hex = sha256_hex(payload.as_bytes());
    ContentHash::sha256(&hex).ok()
}

// -------------------------------------------------------------------------

/// `--no-deep-hash` fast path for apk. SHA-256s the bytes of the
/// package's stanza extracted from `<rootfs>/lib/apk/db/installed`.
/// Returns `None` if the installed-db is absent OR the package
/// isn't found within it. Mirrors the role
/// [`hash_md5sums_only`] plays for dpkg.
///
/// The stanza-extraction is a small embedded scan: walk the file,
/// detect the `P:<pkg_name>` line, accumulate bytes until the
/// next blank-line stanza boundary.
pub fn hash_apk_db_only(rootfs: &Path, pkg_name: &str) -> Option<ContentHash> {
    let path = rootfs.join("lib/apk/db/installed");
    let bytes = std::fs::read(&path).ok()?;
    let stanza = extract_apk_stanza(&bytes, pkg_name)?;
    let hex = sha256_hex(stanza);
    ContentHash::sha256(&hex).ok()
}

/// Find the first stanza in `db_bytes` whose `P:` line names
/// `pkg_name`, and return that stanza's bytes (a contiguous slice).
/// Stanzas are separated by `\n\n`; the package-name line is the
/// authoritative identifier (the apk `P:` field). Linear scan.
fn extract_apk_stanza<'a>(
    db_bytes: &'a [u8],
    pkg_name: &str,
) -> Option<&'a [u8]> {
    let needle = format!("P:{pkg_name}\n");
    let needle_bytes = needle.as_bytes();
    let mut start = 0usize;
    while start < db_bytes.len() {
        let stanza_end = find_blank_line(db_bytes, start);
        let stanza_slice = &db_bytes[start..stanza_end];
        if window_starts_with_or_contains_line(stanza_slice, needle_bytes) {
            return Some(stanza_slice);
        }
        // Advance past the blank-line boundary (or to EOF).
        if stanza_end >= db_bytes.len() {
            return None;
        }
        start = stanza_end + 2; // skip "\n\n"
    }
    None
}

/// Return the byte offset of the next blank line ("\n\n") at or
/// after `from`, or `db_bytes.len()` if there's no blank line
/// (last stanza in the file).
fn find_blank_line(db_bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i + 1 < db_bytes.len() {
        if db_bytes[i] == b'\n' && db_bytes[i + 1] == b'\n' {
            return i;
        }
        i += 1;
    }
    db_bytes.len()
}

/// Whether `stanza` contains a line exactly equal to `needle_line`.
/// `needle_line` ends with `\n`; the match must be at the start of
/// the stanza or immediately after a `\n` boundary, and consume
/// the trailing newline.
fn window_starts_with_or_contains_line(stanza: &[u8], needle_line: &[u8]) -> bool {
    if stanza.starts_with(needle_line) {
        return true;
    }
    let mut i = 0usize;
    while i + needle_line.len() <= stanza.len() {
        if stanza[i] == b'\n' && stanza[i + 1..].starts_with(needle_line) {
            return true;
        }
        i += 1;
    }
    false
}

// ----------------------------------------------------------------------

/// Read `<pkg>[:<arch>].md5sums` into a `path -> md5` map. Lines are
/// `<32-hex-md5>  <relative-path>` (two spaces between the two fields,
/// per dpkg convention). Missing file → empty map.
fn read_md5sums(
    rootfs: &Path,
    pkg_name: &str,
    arch: Option<&str>,
) -> HashMap<String, String> {
    let Some(text) = read_info_file(rootfs, pkg_name, arch, "md5sums") else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for line in text.lines() {
        // Split on the first whitespace run; dpkg uses two spaces but
        // be permissive with any whitespace count.
        let mut parts = line.splitn(2, char::is_whitespace);
        let Some(md5) = parts.next() else { continue };
        let Some(rest) = parts.next() else { continue };
        let path = rest.trim_start();
        if md5.len() == 32 && md5.chars().all(|c| c.is_ascii_hexdigit()) && !path.is_empty() {
            out.insert(path.to_string(), md5.to_string());
        }
    }
    out
}

/// Component-level fingerprint: SHA-256 of a deterministic concatenation
/// of per-file `<sha256>  <location>\n` lines, sorted by location. Stable
/// across scans of the same install regardless of walk order.
fn compute_merkle_root(occurrences: &[FileOccurrence]) -> Option<ContentHash> {
    if occurrences.is_empty() {
        return None;
    }
    let mut sorted: Vec<&FileOccurrence> = occurrences.iter().collect();
    sorted.sort_by(|a, b| a.location.cmp(&b.location));
    let mut hasher = Sha256::new();
    for occ in sorted {
        hasher.update(occ.sha256.as_bytes());
        hasher.update(b"  ");
        hasher.update(occ.location.as_bytes());
        hasher.update(b"\n");
    }
    let bytes = hasher.finalize();
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    ContentHash::sha256(&hex).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::fs;

    /// Build a fake rootfs at `<tmp>/` with one dpkg-installed package
    /// `pkg_name` that owns `files` (path-relative-to-rootfs ↔ contents).
    /// Optionally writes a `.md5sums` referencing those files.
    fn make_rootfs(
        pkg_name: &str,
        files: &[(&str, &[u8])],
        write_md5sums: bool,
    ) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let info = dir.path().join("var/lib/dpkg/info");
        fs::create_dir_all(&info).unwrap();

        // .list with absolute paths.
        let list: String = files
            .iter()
            .map(|(p, _)| format!("/{}", p.trim_start_matches('/')))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(info.join(format!("{pkg_name}.list")), list).unwrap();

        if write_md5sums {
            let mut md5_text = String::new();
            for (p, content) in files {
                let mut md5 = md5_like_string(content);
                md5.truncate(32);
                let rel = p.trim_start_matches('/');
                md5_text.push_str(&format!("{md5}  {rel}\n"));
            }
            fs::write(info.join(format!("{pkg_name}.md5sums")), md5_text).unwrap();
        }

        for (p, content) in files {
            let abs = dir.path().join(p.trim_start_matches('/'));
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, content).unwrap();
        }
        dir
    }

    /// Build a 32-hex-char string from arbitrary bytes for MD5-shaped
    /// fixture data — we don't need real MD5 in tests, just a value
    /// that satisfies the parser's hex-digit check.
    fn md5_like_string(bytes: &[u8]) -> String {
        let mut h = String::new();
        for b in bytes.iter().take(16) {
            h.push_str(&format!("{b:02x}"));
        }
        while h.len() < 32 {
            h.push('0');
        }
        h
    }

    #[test]
    fn deep_hash_produces_per_file_occurrences_and_merkle_root() {
        let dir = make_rootfs(
            "jq",
            &[("usr/bin/jq", b"binary-bytes"), ("usr/share/man/man1/jq.1.gz", b"manpage")],
            true,
        );
        let (occs, root) = hash_package_files(dir.path(), "jq", None);
        assert_eq!(occs.len(), 2);
        assert!(root.is_some(), "must produce a per-component root");
        // Each occurrence carries both hashes.
        for o in &occs {
            assert_eq!(o.sha256.len(), 64, "sha256 hex length");
            assert!(o.md5_legacy.is_some(), "md5sums entry should be present");
        }
    }

    #[test]
    fn merkle_root_stable_across_walk_order() {
        // Build the same fileset twice; the resulting Merkle should
        // be byte-identical regardless of insertion order.
        let dir1 = make_rootfs(
            "p",
            &[("usr/a", b"first"), ("usr/b", b"second"), ("usr/c", b"third")],
            false,
        );
        let dir2 = make_rootfs(
            "p",
            &[("usr/c", b"third"), ("usr/a", b"first"), ("usr/b", b"second")],
            false,
        );
        let (_, root1) = hash_package_files(dir1.path(), "p", None);
        let (_, root2) = hash_package_files(dir2.path(), "p", None);
        // Both should be Some and equal (same path/content sets, just
        // different .list line order — sort makes the root deterministic).
        assert_eq!(root1.is_some(), root2.is_some());
        assert_eq!(
            root1.as_ref().map(|h| h.value.as_str().to_string()),
            root2.as_ref().map(|h| h.value.as_str().to_string())
        );
    }

    #[test]
    fn deep_hash_skips_files_listed_but_missing_on_disk() {
        // .list claims usr/bin/jq + /etc/jqrc; on disk, only jq exists.
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join("var/lib/dpkg/info");
        fs::create_dir_all(&info).unwrap();
        fs::write(
            info.join("jq.list"),
            "/usr/bin/jq\n/etc/jqrc\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("usr/bin")).unwrap();
        fs::write(dir.path().join("usr/bin/jq"), b"binary").unwrap();

        let (occs, _root) = hash_package_files(dir.path(), "jq", None);
        assert_eq!(occs.len(), 1, "missing file must be skipped");
        assert!(occs[0].location.ends_with("usr/bin/jq"));
    }

    #[test]
    fn deep_hash_returns_empty_when_list_absent() {
        let dir = tempfile::tempdir().unwrap();
        let (occs, root) = hash_package_files(dir.path(), "ghost", None);
        assert!(occs.is_empty());
        assert!(root.is_none());
    }

    #[test]
    fn fast_path_md5sums_only() {
        let dir = make_rootfs("jq", &[("usr/bin/jq", b"x")], true);
        let h = hash_md5sums_only(dir.path(), "jq", None).expect("hash");
        // Sanity: same input → same output across calls.
        let h2 = hash_md5sums_only(dir.path(), "jq", None).expect("hash again");
        assert_eq!(h.value.as_str(), h2.value.as_str());
    }

    #[test]
    fn fast_path_returns_none_when_md5sums_absent() {
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join("var/lib/dpkg/info");
        fs::create_dir_all(&info).unwrap();
        // Only .list, no .md5sums.
        fs::write(info.join("p.list"), "/x\n").unwrap();
        assert!(hash_md5sums_only(dir.path(), "p", None).is_none());
    }

    #[test]
    fn occurrence_md5_legacy_is_none_when_not_in_md5sums() {
        // .list has a file; .md5sums omits it (config files that dpkg
        // intentionally doesn't checksum).
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join("var/lib/dpkg/info");
        fs::create_dir_all(&info).unwrap();
        fs::write(info.join("p.list"), "/etc/p.conf\n").unwrap();
        fs::write(info.join("p.md5sums"), "").unwrap(); // empty
        fs::create_dir_all(dir.path().join("etc")).unwrap();
        fs::write(dir.path().join("etc/p.conf"), b"config").unwrap();

        let (occs, _) = hash_package_files(dir.path(), "p", None);
        assert_eq!(occs.len(), 1);
        assert!(occs[0].md5_legacy.is_none());
    }

    #[test]
    fn multi_arch_info_files_resolve_via_colon_arch_fallback() {
        // Multi-arch dpkg installs name their info files with a
        // `:<arch>` suffix (e.g. libc6:arm64.list). The function must
        // fall back from `<pkg>.list` to `<pkg>:<arch>.list`.
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join("var/lib/dpkg/info");
        fs::create_dir_all(&info).unwrap();
        // Only write the arch-suffixed variant — the plain name is absent.
        fs::write(info.join("libc6:arm64.list"), "/usr/lib/libc.so.6\n").unwrap();
        fs::write(
            info.join("libc6:arm64.md5sums"),
            "d41d8cd98f00b204e9800998ecf8427e  usr/lib/libc.so.6\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("usr/lib")).unwrap();
        fs::write(dir.path().join("usr/lib/libc.so.6"), b"libc-body").unwrap();

        // Without arch hint → plain lookup fails, no occurrences.
        let (occs_plain, _) = hash_package_files(dir.path(), "libc6", None);
        assert!(occs_plain.is_empty(), "must not match arch-suffixed file without arch hint");

        // With arch hint → fallback finds it.
        let (occs, root) = hash_package_files(dir.path(), "libc6", Some("arm64"));
        assert_eq!(occs.len(), 1);
        assert!(occs[0].md5_legacy.is_some(), "md5sums cross-ref must resolve too");
        assert!(root.is_some());

        // Fast path also resolves the arch-suffixed md5sums.
        assert!(hash_md5sums_only(dir.path(), "libc6", Some("arm64")).is_some());
        assert!(
            hash_md5sums_only(dir.path(), "libc6", None).is_none(),
            "plain lookup on fast path must not match the arch-suffixed file"
        );
    }

    // ---- Milestone 038: status.d/ deep-hash for minimal images -------------

    /// Build a minimal-image-shaped rootfs at `<tmp>/` with one
    /// `status.d/<pkg>` stanza file, optionally a `<pkg>.md5sums`
    /// companion, and the listed files actually present on disk.
    /// Returns the tempdir handle.
    fn make_status_d_rootfs(
        pkg_name: &str,
        files: &[(&str, &[u8])],
        write_md5sums: bool,
    ) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status_d = dir.path().join("var/lib/dpkg/status.d");
        fs::create_dir_all(&status_d).unwrap();

        // Stanza file (mirrors what dpkg.rs::read_status_d_dir consumes).
        let stanza = format!(
            "Package: {pkg_name}\n\
             Status: install ok installed\n\
             Version: 1.0\n\
             Architecture: amd64\n",
        );
        fs::write(status_d.join(pkg_name), stanza).unwrap();

        if write_md5sums {
            let mut md5_text = String::new();
            for (p, content) in files {
                let mut md5 = md5_like_string(content);
                md5.truncate(32);
                let rel = p.trim_start_matches('/');
                md5_text.push_str(&format!("{md5}  {rel}\n"));
            }
            fs::write(status_d.join(format!("{pkg_name}.md5sums")), md5_text).unwrap();
        }

        for (p, content) in files {
            let abs = dir.path().join(p.trim_start_matches('/'));
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, content).unwrap();
        }
        dir
    }

    /// T006 — `read_info_file` falls back to
    /// `var/lib/dpkg/status.d/<pkg>.<ext>` when neither
    /// `info/<pkg>.<ext>` nor `info/<pkg>:<arch>.<ext>` exists.
    #[test]
    fn read_info_file_falls_back_to_status_d() {
        let dir = tempfile::tempdir().unwrap();
        let status_d = dir.path().join("var/lib/dpkg/status.d");
        fs::create_dir_all(&status_d).unwrap();
        // Synthetic .md5sums content; the value is opaque to the
        // function under test, which just reads the file as UTF-8.
        let body =
            "abcdef0123456789abcdef0123456789  usr/bin/foo\n\
             0123456789abcdef0123456789abcdef  etc/foo.conf\n";
        fs::write(status_d.join("foo.md5sums"), body).unwrap();

        let got = read_info_file(dir.path(), "foo", None, "md5sums");
        assert_eq!(got.as_deref(), Some(body));
    }

    /// T007 — when both legacy `info/<pkg>.md5sums` AND
    /// `status.d/<pkg>.md5sums` exist, the legacy file wins (R5
    /// precedence: more complete data takes priority on collision).
    #[test]
    fn read_info_file_legacy_wins_over_status_d() {
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join("var/lib/dpkg/info");
        let status_d = dir.path().join("var/lib/dpkg/status.d");
        fs::create_dir_all(&info).unwrap();
        fs::create_dir_all(&status_d).unwrap();
        fs::write(info.join("foo.md5sums"), "from-info\n").unwrap();
        fs::write(status_d.join("foo.md5sums"), "from-status-d\n").unwrap();

        let got = read_info_file(dir.path(), "foo", None, "md5sums");
        assert_eq!(
            got.as_deref(),
            Some("from-info\n"),
            "legacy info/ source must take priority over status.d/"
        );
    }

    /// T008 — `hash_package_files` synthesizes the path list from
    /// `<pkg>.md5sums` when no `<pkg>.list` exists. The 2 files
    /// listed in the synthesized .list resolve to actual rootfs
    /// files and produce 2 occurrences with computed SHA-256 values.
    #[test]
    fn hash_package_files_synthesizes_list_from_md5sums() {
        let dir = make_status_d_rootfs(
            "base-files",
            &[
                ("usr/share/base-files/info", b"image release info"),
                ("etc/debian_version", b"12.0\n"),
            ],
            true,
        );
        let (occs, root) = hash_package_files(dir.path(), "base-files", None);
        assert_eq!(occs.len(), 2, "both md5sums-listed files should occur");
        assert!(root.is_some(), "must produce a per-component Merkle root");
        for o in &occs {
            assert_eq!(o.sha256.len(), 64, "each occurrence carries a sha256");
            assert!(
                o.location.starts_with('/'),
                "synthesized location must be absolute (matches legacy convention); \
                 got `{}`",
                o.location
            );
            // md5_legacy lookup uses the same .md5sums file → cross-
            // referenced and present.
            assert!(
                o.md5_legacy.is_some(),
                "md5sums cross-reference must resolve in status.d/ layout too"
            );
        }
    }

    /// T009 — when only the stanza file exists (no `.md5sums`
    /// companion), `hash_package_files` returns empty occurrences
    /// rather than failing. Matches FR-004's graceful incomplete-
    /// metadata handling.
    #[test]
    fn hash_package_files_returns_empty_when_no_list_or_md5sums() {
        let dir = tempfile::tempdir().unwrap();
        let status_d = dir.path().join("var/lib/dpkg/status.d");
        fs::create_dir_all(&status_d).unwrap();
        // Stanza only — no .list, no .md5sums.
        fs::write(
            status_d.join("orphan"),
            "Package: orphan\n\
             Status: install ok installed\n\
             Version: 0.1\n\
             Architecture: amd64\n",
        )
        .unwrap();

        let (occs, root) = hash_package_files(dir.path(), "orphan", None);
        assert!(occs.is_empty(), "no metadata source → no occurrences");
        assert!(root.is_none(), "no occurrences → no Merkle root");
    }

    /// T010 — the `--no-deep-hash` fast path (`hash_md5sums_only`)
    /// finds `status.d/<pkg>.md5sums` via the same lookup-chain
    /// extension. Per FR-003, fast-hash never produces per-file
    /// evidence, but the package-level identity hash MUST still
    /// resolve for status.d/ images.
    #[test]
    fn hash_md5sums_only_finds_status_d_md5sums() {
        let dir = make_status_d_rootfs(
            "tzdata",
            &[("usr/share/zoneinfo/UTC", b"utc")],
            true,
        );
        let h = hash_md5sums_only(dir.path(), "tzdata", None)
            .expect("md5sums-only must resolve under status.d/ layout");
        // Stable across calls (deterministic over the same input bytes).
        let h2 = hash_md5sums_only(dir.path(), "tzdata", None).expect("again");
        assert_eq!(h.value.as_str(), h2.value.as_str());
    }

    // ---- Milestone 039: apk per-file deep-hashing -----------------------

    fn write_apk_db(rootfs: &std::path::Path, body: &str) {
        let p = rootfs.join("lib/apk/db/installed");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, body).unwrap();
    }

    /// Helper: build a tempdir-rooted fake apk rootfs with the given
    /// files actually present on disk.
    fn place_files(rootfs: &std::path::Path, files: &[(&str, &[u8])]) {
        for (rel, content) in files {
            let abs = rootfs.join(rel.trim_start_matches('/'));
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, content).unwrap();
        }
    }

    #[test]
    fn hash_apk_package_files_round_trips_files() {
        let dir = tempfile::tempdir().unwrap();
        place_files(
            dir.path(),
            &[("usr/bin/foo", b"foo-bytes"), ("etc/foo.conf", b"conf=1\n")],
        );
        let files = vec![
            super::super::apk::ApkFileEntry {
                path: "usr/bin/foo".to_string(),
                sha1: None,
            },
            super::super::apk::ApkFileEntry {
                path: "etc/foo.conf".to_string(),
                sha1: None,
            },
        ];
        let (occs, root) = hash_apk_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 2);
        assert!(root.is_some(), "must produce a per-component Merkle root");
        for o in &occs {
            assert_eq!(o.sha256.len(), 64);
            assert!(
                o.location.starts_with('/'),
                "apk occurrence locations must be absolute, got `{}`",
                o.location
            );
            assert!(
                o.md5_legacy.is_none(),
                "apk path never carries MD5 cross-ref"
            );
            assert!(
                o.apk_sha1.is_none(),
                "no Z: data was passed in this test, so apk_sha1 must be None"
            );
        }
    }

    /// Milestone 040 US2: Z:-line SHA-1 surfaces on the resulting
    /// occurrence's `apk_sha1` field when the input ApkFileEntry
    /// carries one.
    #[test]
    fn hash_apk_package_files_threads_sha1_when_provided() {
        let dir = tempfile::tempdir().unwrap();
        place_files(dir.path(), &[("usr/bin/foo", b"foo-bytes")]);
        let files = vec![super::super::apk::ApkFileEntry {
            path: "usr/bin/foo".to_string(),
            sha1: Some("aabbccddeeff00112233445566778899aabbccdd".to_string()),
        }];
        let (occs, _root) = hash_apk_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 1);
        assert_eq!(
            occs[0].apk_sha1.as_deref(),
            Some("aabbccddeeff00112233445566778899aabbccdd"),
            "apk_sha1 must thread from input entry to output occurrence"
        );
    }

    #[test]
    fn hash_apk_package_files_skips_absent_files() {
        let dir = tempfile::tempdir().unwrap();
        place_files(dir.path(), &[("usr/bin/exists", b"on-disk")]);
        // Only one of the two listed files actually exists on disk.
        let files = vec![
            super::super::apk::ApkFileEntry {
                path: "usr/bin/exists".to_string(),
                sha1: None,
            },
            super::super::apk::ApkFileEntry {
                path: "usr/bin/missing".to_string(),
                sha1: None,
            },
        ];
        let (occs, root) = hash_apk_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 1, "absent file must be skipped");
        assert_eq!(occs[0].location, "/usr/bin/exists");
        assert!(root.is_some());
    }

    #[test]
    fn hash_apk_package_files_returns_empty_for_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        let (occs, root) = hash_apk_package_files(dir.path(), &[]);
        assert!(occs.is_empty());
        assert!(root.is_none());
    }

    #[test]
    fn hash_apk_db_only_finds_named_stanza() {
        let dir = tempfile::tempdir().unwrap();
        write_apk_db(
            dir.path(),
            "P:foo\n\
             V:1.0\n\
             A:x86_64\n\
             \n\
             P:bar\n\
             V:2.0\n\
             A:x86_64\n",
        );
        let h_foo = hash_apk_db_only(dir.path(), "foo")
            .expect("foo stanza should resolve");
        let h_bar = hash_apk_db_only(dir.path(), "bar")
            .expect("bar stanza should resolve");
        assert_ne!(
            h_foo.value.as_str(),
            h_bar.value.as_str(),
            "different stanzas must hash differently"
        );
        // Stable across calls.
        let h_foo2 = hash_apk_db_only(dir.path(), "foo").expect("again");
        assert_eq!(h_foo.value.as_str(), h_foo2.value.as_str());
    }

    #[test]
    fn hash_apk_db_only_returns_none_for_missing_package() {
        let dir = tempfile::tempdir().unwrap();
        write_apk_db(dir.path(), "P:foo\nV:1.0\n");
        assert!(hash_apk_db_only(dir.path(), "ghost").is_none());
    }

    #[test]
    fn hash_apk_db_only_returns_none_when_db_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(hash_apk_db_only(dir.path(), "foo").is_none());
    }

    // ---- Milestone 040 US3: rpm per-file deep-hashing ---------------------

    /// Helper: build an RpmFileListEntry with no cross-ref digest
    /// (matches packages whose FILEDIGESTS isn't being exercised
    /// by the test).
    fn rpm_path(p: &str) -> super::super::rpm::RpmFileListEntry {
        super::super::rpm::RpmFileListEntry {
            path: p.to_string(),
            digest: None,
        }
    }

    #[test]
    fn hash_rpm_package_files_round_trips_files() {
        let dir = tempfile::tempdir().unwrap();
        place_files(
            dir.path(),
            &[("usr/bin/bash", b"bash-bytes"), ("etc/bash.bashrc", b"# rc\n")],
        );
        // rpm path conventions: paths come in as absolute (with
        // leading /).
        let files = vec![rpm_path("/usr/bin/bash"), rpm_path("/etc/bash.bashrc")];
        let (occs, root) = hash_rpm_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 2);
        assert!(root.is_some(), "must produce a per-component Merkle root");
        for o in &occs {
            assert_eq!(o.sha256.len(), 64);
            assert!(
                o.location.starts_with('/'),
                "rpm occurrence locations must be absolute, got `{}`",
                o.location
            );
            assert!(
                o.md5_legacy.is_none(),
                "rpm path never carries MD5 cross-ref"
            );
            assert!(
                o.apk_sha1.is_none(),
                "rpm path never carries the apk SHA-1 cross-ref"
            );
            assert!(
                o.rpm_file_digest.is_none(),
                "no FILEDIGESTS data was passed in this test, so rpm_file_digest must be None"
            );
        }
    }

    /// Milestone 041: rpm_file_digest threads from input
    /// `RpmFileListEntry.digest` to output
    /// `FileOccurrence.rpm_file_digest` unchanged.
    #[test]
    fn hash_rpm_package_files_threads_digest_when_provided() {
        let dir = tempfile::tempdir().unwrap();
        place_files(dir.path(), &[("usr/bin/bash", b"bash-bytes")]);
        let files = vec![super::super::rpm::RpmFileListEntry {
            path: "/usr/bin/bash".to_string(),
            digest: Some("sha256:aabbccdd...".to_string()),
        }];
        let (occs, _root) = hash_rpm_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 1);
        assert_eq!(
            occs[0].rpm_file_digest.as_deref(),
            Some("sha256:aabbccdd..."),
            "rpm_file_digest must thread from input entry to output"
        );
    }

    #[test]
    fn hash_rpm_package_files_skips_absent_files() {
        let dir = tempfile::tempdir().unwrap();
        place_files(dir.path(), &[("usr/bin/exists", b"on-disk")]);
        let files = vec![rpm_path("/usr/bin/exists"), rpm_path("/usr/bin/missing")];
        let (occs, root) = hash_rpm_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 1, "absent file must be skipped");
        assert_eq!(occs[0].location, "/usr/bin/exists");
        assert!(root.is_some());
    }

    #[test]
    fn hash_rpm_package_files_returns_empty_for_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        let (occs, root) = hash_rpm_package_files(dir.path(), &[]);
        assert!(occs.is_empty());
        assert!(root.is_none());
    }

    #[test]
    fn hash_rpm_package_files_handles_relative_path_entries() {
        // Defensive: even if a hypothetical caller passes paths
        // without the leading `/`, the helper should normalize.
        let dir = tempfile::tempdir().unwrap();
        place_files(dir.path(), &[("usr/bin/foo", b"bytes")]);
        let files = vec![rpm_path("usr/bin/foo")]; // no leading /
        let (occs, _root) = hash_rpm_package_files(dir.path(), &files);
        assert_eq!(occs.len(), 1);
        assert_eq!(
            occs[0].location, "/usr/bin/foo",
            "relative input should normalize to absolute location"
        );
    }
}
