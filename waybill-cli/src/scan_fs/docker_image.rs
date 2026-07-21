//! Extract a `docker save` tarball's layers onto disk so the filesystem
//! scanner can walk the resulting rootfs.
//!
//! Docker's `image save` format (also called the v1.2 tarball format) is a
//! plain tar archive containing:
//!
//! - `manifest.json` at the root: an array of
//!   `{Config, Layers: [...], RepoTags: [...]}` entries. We take the first
//!   entry.
//! - One file per layer at the paths named in `Layers[]` (typically
//!   `<sha256>/layer.tar` for older dockers, `blobs/sha256/<digest>` for
//!   newer OCI-formatted output). Each is itself a tar archive.
//! - Optional metadata files (config JSON, repositories) that we ignore.
//!
//! We stage the outer tarball into a temp directory, extract each layer
//! in order into a shared rootfs directory while applying OCI whiteouts
//! (`.wh.foo` removes `foo`; `.wh..wh..opq` empties the parent). The
//! returned [`ExtractedImage`] carries the rootfs path and some identity
//! metadata for the caller to attribute the SBOM to.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tempfile::TempDir;

/// Outcome of extracting a docker-save tarball.
#[derive(Debug)]
pub struct ExtractedImage {
    /// Owned tempdir that holds the extracted rootfs. Dropped when the
    /// caller is done scanning, which removes all extracted files.
    /// Held for lifetime, not read directly.
    #[allow(dead_code)]
    pub tempdir: TempDir,
    /// Root of the union filesystem (a subdirectory of `tempdir.path()`).
    pub rootfs: PathBuf,
    /// First `RepoTags` entry from the manifest, if any. Populated into
    /// the SBOM's subject `name` so downstream tooling can identify the
    /// image without having to re-read the tarball.
    pub repo_tag: Option<String>,
    /// SHA-256 of the outer `manifest.json` bytes. A stable identifier
    /// for the scanned tarball; useful for the SBOM's `serialNumber` or
    /// an accompanying attestation subject digest.
    pub manifest_digest: String,
    /// `VERSION_CODENAME` read from `<rootfs>/etc/os-release` after
    /// layer extraction, when available. Used as the default value for
    /// `--deb-codename` so deb PURLs pulled out of the rootfs carry
    /// the right `distro=` qualifier without the user having to pass
    /// it manually.
    pub distro_codename: Option<String>,
    /// Milestone 133 US2.2 (FR-013): map from rootfs-relative path
    /// (no leading `/`, forward-slash separators) to the SHA-256 digest
    /// (`sha256:<hex>`) of the layer-blob that wrote that path. When the
    /// same path is written by multiple layers, the LAST layer in the
    /// `Layers[]` array wins (OCI overlay semantics — later layers
    /// shadow earlier ones; consumers asking "which layer introduced
    /// this content?" want the latest writer, since earlier writes are
    /// no longer the file at rest).
    ///
    /// Drives the `waybill:layer-digest` per-component property at SBOM
    /// emission time. CDX, SPDX 2.3, SPDX 3 all read the same map so
    /// `holistic_parity` C-row directionality (`SymmetricEqual`) holds
    /// across formats.
    ///
    /// Stale entries are harmless: a path written by layer N then
    /// whiteout-deleted by layer N+1 still has a map entry, but the
    /// downstream resolver doesn't see the file (it isn't in the
    /// rootfs) so the entry is never looked up.
    pub layer_path_map: std::collections::HashMap<String, String>,
}

/// Parsed form of the top-level `manifest.json` in a docker save tarball.
/// Only the fields we consume are decoded; extras are ignored.
#[derive(Debug, Deserialize)]
struct DockerManifestEntry {
    #[serde(rename = "Config", default)]
    _config: String,
    // `Option<Vec<String>>` (not `Vec<String>`) because `docker save` on an
    // image that was pulled by digest (no tag) writes `"RepoTags": null` —
    // a present field with a null value, which `#[serde(default)]` does NOT
    // cover (default only catches *missing* fields). `Option` accepts both
    // `null` and absence, and `unwrap_or_default()` at the use-site treats
    // both as empty. Observed during milestone-132 MVP SC verification when
    // `docker pull <repo>@sha256:<digest>` produced a tag-less image.
    #[serde(rename = "RepoTags", default)]
    repo_tags: Option<Vec<String>>,
    #[serde(rename = "Layers", default)]
    layers: Vec<String>,
}

/// Extract a docker-save tarball at `archive_path` into a fresh tempdir
/// and return the resulting rootfs.
pub fn extract(archive_path: &Path) -> Result<ExtractedImage> {
    // We make two passes over the outer tarball: one to read the
    // manifest, one to extract each named layer. `tar::Archive` can't
    // rewind, so we reopen the file each time.
    let manifest_bytes = read_entry(archive_path, "manifest.json")
        .with_context(|| format!("reading manifest.json from {}", archive_path.display()))?;
    let manifest_digest = sha256_hex(&manifest_bytes);

    let entries: Vec<DockerManifestEntry> = serde_json::from_slice(&manifest_bytes)
        .context("parsing manifest.json")?;
    let Some(image) = entries.into_iter().next() else {
        bail!("manifest.json contains zero image entries");
    };

    let repo_tag = image.repo_tags.unwrap_or_default().into_iter().next();
    if image.layers.is_empty() {
        bail!("image manifest has zero layers — not a valid docker save tarball?");
    }

    let tempdir = tempfile::Builder::new()
        .prefix("waybill-image-")
        .tempdir()
        .context("creating tempdir for image extraction")?;
    let rootfs = tempdir.path().join("rootfs");
    fs::create_dir_all(&rootfs).context("creating rootfs dir")?;

    // Milestone 133 US2.2 (FR-013): build the path → layer-digest map
    // as we extract. Layer digest is the SHA-256 of the LAYER BLOB bytes
    // (the compressed-or-plain tar as stored in the docker save tarball),
    // matching trivy's `LayerDigest` (which is the OCI layer blob digest,
    // not the uncompressed `DiffID`). For the modern OCI-format docker
    // save the blob path itself encodes the digest (`blobs/sha256/<hex>`);
    // for legacy docker save it doesn't. Computing the hash ourselves
    // handles both formats uniformly.
    let mut layer_path_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (idx, layer_name) in image.layers.iter().enumerate() {
        tracing::debug!(layer = idx, name = %layer_name, "extracting layer");
        let layer_bytes = read_entry(archive_path, layer_name).with_context(|| {
            format!("reading layer {layer_name} from {}", archive_path.display())
        })?;
        let layer_digest = format!("sha256:{}", sha256_hex(&layer_bytes));
        let paths_written = extract_layer_over_rootfs(&layer_bytes, &rootfs)
            .with_context(|| format!("extracting layer {layer_name}"))?;
        // Later-layer-wins overlay semantics: unconditional insert overwrites
        // any earlier-layer entry for the same path.
        for path in paths_written {
            layer_path_map.insert(path, layer_digest.clone());
        }
    }

    // After the rootfs is fully assembled, read the distro tag (see
    // `os_release::read_distro_tag_from_rootfs`) so `waybill sbom scan
    // --image` can stamp `distro=<ID>-<VERSION_ID>` (e.g. `debian-12`)
    // on deb PURLs without the user having to pass --deb-codename.
    // Rootfs-aware because /etc/os-release is commonly a symlink into
    // /usr/lib/os-release that can dangle after layer extraction.
    // Absent or unreadable is not an error — not every image carries
    // os-release (minimal FROM scratch, busybox).
    let distro_codename = super::os_release::read_distro_tag_from_rootfs(&rootfs);

    Ok(ExtractedImage {
        tempdir,
        rootfs,
        repo_tag,
        manifest_digest,
        distro_codename,
        layer_path_map,
    })
}

/// Issue #401 — when a tar Symlink entry's target is absolute
/// (`/run`, `/lib64`, `/usr/bin`, ...), rewrite it to be a relative
/// path that climbs back to the rootfs root and then descends into
/// the target. Returns `Some(rewritten_target)` when the rewrite
/// applies; `None` when the target is already relative.
///
/// Algorithm: for a symlink at rootfs-relative path
/// `<parent_dir>/<link_name>` whose target is the absolute path
/// `/x/y/z`, the rewritten target is `(../ × N)x/y/z` where N is
/// the component count of `parent_dir`. This canonicalizes to
/// `<rootfs>/x/y/z` instead of host `/x/y/z`.
///
/// Examples:
/// - `var/run -> /run` → `var/run -> ../run` (1 `..`)
/// - `usr/lib64 -> /lib64` → `usr/lib64 -> ../lib64`
/// - `var/lib/dpkg/status -> /etc/passwd` → `../../../etc/passwd`
///
/// `#[cfg(unix)]`-gated to match the only call site in
/// `extract_layer_over_rootfs`. Windows image scans skip this path
/// (Windows symlink semantics differ enough that the rewrite would
/// need to be re-validated there).
#[cfg(unix)]
fn rewrite_symlink_target_if_absolute<R: std::io::Read>(
    link_rel_path: &Path,
    entry: &tar::Entry<'_, R>,
) -> Option<PathBuf> {
    let target = entry.link_name().ok()??.into_owned();
    if !target.is_absolute() {
        return None;
    }
    let bare_target = target.strip_prefix("/").unwrap_or(&target).to_path_buf();
    let parent_depth = link_rel_path
        .parent()
        .map(|p| p.components().count())
        .unwrap_or(0);
    let mut rewritten = PathBuf::new();
    for _ in 0..parent_depth {
        rewritten.push("..");
    }
    rewritten.push(bare_target);
    Some(rewritten)
}

/// Read a single named entry out of a tar archive into a `Vec<u8>`. The
/// outer tarball is opened from scratch, scanned for the entry, and
/// closed. `tar::Archive` doesn't let us hold a mutable borrow on the
/// reader across entries, so each call pays a fresh file-open cost.
fn read_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let file = fs::File::open(archive_path)?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries()? {
        let mut e = entry?;
        let path = e.path()?;
        if path.as_os_str() == entry_name {
            let mut buf = Vec::new();
            e.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    bail!("entry {entry_name} not found in tarball")
}

/// Extract an inner `layer.tar` byte stream on top of `rootfs`, applying
/// OCI whiteout semantics.
///
/// A regular file at `path/to/.wh.NAME` means "remove `path/to/NAME` from
/// the rootfs." The special name `.wh..wh..opq` under a directory means
/// "remove all existing contents of that directory." We implement the
/// common subset: remove-on-whiteout for both cases; the whiteout marker
/// files themselves are not extracted into the rootfs.
/// Returns the list of rootfs-relative paths the layer wrote (regular
/// files + symlinks + hardlinks; directories excluded — they're not
/// useful for the milestone-133 US2.2 layer-digest lookup since
/// components' source paths are always files). Paths use forward-slash
/// separators and have no leading `/` to match the rest of the milestone-
/// 133 path-emission convention (FR-007 / FR-012 / FR-014).
fn extract_layer_over_rootfs(layer_bytes: &[u8], rootfs: &Path) -> Result<Vec<String>> {
    // Layers may be plain tar (legacy docker save) or gzipped tar (OCI
    // format emitted by modern docker save + most registries). Detect
    // by magic bytes so callers don't need to know which they have.
    let decompressed: Vec<u8> = if layer_bytes.len() >= 2
        && layer_bytes[0] == 0x1f
        && layer_bytes[1] == 0x8b
    {
        let mut out = Vec::with_capacity(layer_bytes.len() * 4);
        let mut decoder = flate2::read::GzDecoder::new(layer_bytes);
        decoder
            .read_to_end(&mut out)
            .context("gunzipping OCI layer")?;
        out
    } else {
        layer_bytes.to_vec()
    };
    let layer_bytes: &[u8] = &decompressed;

    // First pass: collect whiteout directives so we apply them up front.
    // Two-pass keeps the logic simple — one pass to find `.wh.*` names
    // and delete their targets, then another to unpack everything else.
    let mut archive = tar::Archive::new(std::io::Cursor::new(layer_bytes));
    let mut whiteouts: HashSet<PathBuf> = HashSet::new();
    let mut opaque_dirs: HashSet<PathBuf> = HashSet::new();
    for entry in archive.entries()? {
        let e = entry?;
        let raw_path = e.path()?.into_owned();
        // Issue #399 — apply the same path-traversal defenses to
        // whiteout entries as the regular-file unpack loop below.
        // Without this, a malicious image with a `.wh.` entry like
        // `../../tmp/.wh.evicted` would have the cleanup loop call
        // `fs::remove_dir_all` / `fs::remove_file` on
        // `<rootfs>/../../tmp/evicted` — a delete primitive on
        // paths outside the extraction tempdir. Treatment is
        // identical to L312-L320: strip leading `/`, reject any
        // `..` component.
        let path = if raw_path.is_absolute() {
            raw_path.strip_prefix("/").unwrap_or(&raw_path).to_path_buf()
        } else {
            raw_path
        };
        if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            tracing::debug!(
                path = %path.display(),
                "skipping unsafe whiteout entry (parent-dir escape)"
            );
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == ".wh..wh..opq" {
            if let Some(parent) = path.parent() {
                opaque_dirs.insert(parent.to_path_buf());
            }
        } else if let Some(target) = name.strip_prefix(".wh.") {
            if let Some(parent) = path.parent() {
                whiteouts.insert(parent.join(target));
            } else {
                whiteouts.insert(PathBuf::from(target));
            }
        }
    }

    for opq in &opaque_dirs {
        let full = rootfs.join(opq);
        if full.is_dir() {
            // Clear contents but keep the directory itself so subsequent
            // entries for this path can repopulate it.
            if let Ok(entries) = fs::read_dir(&full) {
                for entry in entries.flatten() {
                    let _ = if entry
                        .file_type()
                        .map(|t| t.is_dir())
                        .unwrap_or(false)
                    {
                        fs::remove_dir_all(entry.path())
                    } else {
                        fs::remove_file(entry.path())
                    };
                }
            }
        }
    }
    for wh in &whiteouts {
        let full = rootfs.join(wh);
        if full.is_dir() {
            let _ = fs::remove_dir_all(&full);
        } else if full.exists() {
            let _ = fs::remove_file(&full);
        }
    }

    // Second pass: unpack everything except whiteout marker files.
    //
    // v6 Phase F: tar entries iterate in storage order, which is NOT
    // topologically sorted against hardlinks. A hardlink entry can
    // appear before the file it's linking to (common on Fedora
    // images where /usr/bin/rpm / rpm2archive / rpm2cpio share
    // inodes). When that happens, `unpack_in` fails because the
    // link target doesn't exist yet, and the hardlink silently
    // vanishes from the extracted tree. We defer hardlinks to a
    // second pass so targets are guaranteed present.
    let mut archive = tar::Archive::new(std::io::Cursor::new(layer_bytes));
    let _ = &mut archive; // configuration setters below need the &mut.
    archive.set_preserve_permissions(false);
    archive.set_preserve_mtime(true);
    archive.set_overwrite(true);

    // (link_path, target_path) pairs — applied after the main unpack.
    let mut deferred_links: Vec<(PathBuf, PathBuf)> = Vec::new();
    // Milestone 133 US2.2 (FR-013): track paths written so `extract()`
    // can build the global path → layer-digest map (later-layer-wins
    // overlay semantics).
    let mut paths_written: Vec<String> = Vec::new();

    for entry in archive.entries()? {
        let mut e = entry?;
        let path = e.path()?.into_owned();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == ".wh..wh..opq" || name.starts_with(".wh.") {
            continue;
        }
        // `tar` unpacks relative to a target directory. Reject entries
        // with `..` components — those CAN escape the rootfs and write
        // to e.g. `../../etc/passwd`. Leading `/` is NOT an escape risk:
        // it's the OCI/Docker tar convention for "relative to the rootfs
        // root" (and the underlying `tar::Entry::unpack_in` strips it
        // anyway). ko-built images write entries with absolute paths
        // (`/ko-app/<binary>`, `/var/run/ko`); the previous reject-
        // everything-absolute behavior silently dropped the entire ko
        // app layer. See #281.
        let path = if path.is_absolute() {
            path.strip_prefix("/").unwrap_or(&path).to_path_buf()
        } else {
            path
        };
        if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            tracing::debug!(path = %path.display(), "skipping unsafe tar entry (parent-dir escape)");
            continue;
        }

        // Defer hardlink entries. Symlinks are fine to unpack in-order
        // because `fs::symlink` doesn't require the target to exist.
        if e.header().entry_type() == tar::EntryType::Link {
            if let Ok(Some(target)) = e.link_name() {
                let target = target.into_owned();
                // Same treatment as entry paths: strip leading `/`,
                // reject only `..` components.
                let target = if target.is_absolute() {
                    target
                        .strip_prefix("/")
                        .unwrap_or(&target)
                        .to_path_buf()
                } else {
                    target
                };
                if target
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    tracing::debug!(
                        link = %path.display(),
                        target = %target.display(),
                        "skipping hardlink with parent-dir escape in target",
                    );
                    continue;
                }
                deferred_links.push((path, target));
                continue;
            }
            tracing::debug!(path = %path.display(), "hardlink entry has no link_name; skipping");
            continue;
        }

        // v7 Phase I: the tar crate doesn't reliably create parent
        // directories when the parent's own directory entry hasn't been
        // processed yet (Fedora image layers reference deep paths like
        // `usr/lib/sysimage/rpm/rpmdb.sqlite` whose parent-directory
        // entries come later in the stream). Pre-create parents so
        // `unpack_in` never fails on missing-directory. For directory
        // entries themselves, `unpack_in` creates the leaf directory;
        // we pre-create the parent chain as belt-and-suspenders.
        let abs = rootfs.join(&path);
        if let Some(parent) = abs.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // v9 Phase N: tar 0.4.45's Entry::unpack applies tar-header
        // permissions to extracted directories even with
        // `set_preserve_permissions(false)` (only SUID/SGID/sticky are
        // stripped; the rwx bits come from the header). Fedora images
        // ship directories like
        // `/etc/pki/ca-trust/extracted/pem/directory-hash/` with mode
        // 0555 (no write), which blocks every subsequent tar entry
        // that wants to land inside them with EACCES. Cumulative
        // effect on polyglot-builder: 20k+ extraction failures,
        // including the Layer 1 updated rpmdb.sqlite that would
        // otherwise carry 500+ rpm components.
        //
        // The fix: force the entry's parent directory to owner-rwx
        // (+0o700) before each unpack_in. The extracted rootfs is
        // a throw-away tempdir — original permission semantics don't
        // matter for SBOM reading. `symlink_metadata` so we don't
        // follow a legitimate symlink.
        #[cfg(unix)]
        if let Some(parent) = abs.parent() {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::symlink_metadata(parent) {
                let mode = meta.permissions().mode();
                if mode & 0o700 != 0o700 {
                    let mut p = meta.permissions();
                    p.set_mode(mode | 0o700);
                    let _ = fs::set_permissions(parent, p);
                }
            }
        }

        // Track entry type BEFORE the unpack consumes the entry.
        // Directories aren't recorded — only regular files + symlinks
        // are useful for the milestone-133 US2.2 layer-digest map (a
        // component's source path is always a file).
        let entry_type = e.header().entry_type();
        let is_file_like = matches!(
            entry_type,
            tar::EntryType::Regular | tar::EntryType::Symlink | tar::EntryType::Continuous,
        );

        // Issue #401 — symlink-target host-fs escape. Container
        // images legitimately ship absolute symlinks for distro
        // compatibility (`var/run -> /run`, `usr/lib64 -> /lib64`,
        // `bin -> /usr/bin`, ...). When `unpack_in` creates these
        // verbatim, the symlink at `<rootfs>/var/run` points at the
        // HOST's `/run` — every reader that touches that path
        // (deep-hash, OS-package readers, os_release) then reads
        // host content. PR #397 fixed the `safe_walk` directory
        // walker; this fix closes the per-file-read variant.
        //
        // Treatment: rewrite absolute targets to be relative +
        // rootfs-anchored. `<rootfs>/var/run -> /run` becomes
        // `<rootfs>/var/run -> ../run`, which resolves to
        // `<rootfs>/run` instead of host `/run`. In-image semantics
        // preserved; host escape blocked.
        #[cfg(unix)]
        if entry_type == tar::EntryType::Symlink {
            if let Some(rewrite) = rewrite_symlink_target_if_absolute(&path, &e) {
                let link_abs = rootfs.join(&path);
                if link_abs.exists() {
                    // Layer overlay: later layer's symlink replaces
                    // an earlier file/symlink at the same path.
                    let _ = fs::remove_file(&link_abs);
                }
                match std::os::unix::fs::symlink(&rewrite, &link_abs) {
                    Ok(()) => {
                        tracing::debug!(
                            link = %path.display(),
                            rewritten = %rewrite.display(),
                            "rewrote absolute symlink target to rootfs-anchored form (#401)"
                        );
                        paths_written.push(path.to_string_lossy().into_owned());
                        continue;
                    }
                    Err(err) => {
                        tracing::debug!(
                            link = %path.display(),
                            error = %err,
                            "failed to create rewritten symlink; falling back to default unpack"
                        );
                    }
                }
            }
        }

        if let Err(err) = e.unpack_in(rootfs) {
            tracing::debug!(path = %path.display(), error = %err, "failed to unpack entry");
            continue;
        }
        if is_file_like {
            paths_written.push(path.to_string_lossy().into_owned());
        }
    }

    // Second mini-pass: create hardlinks now that their targets are in
    // place. If `fs::hard_link` fails (e.g. cross-device, target missing
    // even here), fall back to a full copy so we don't silently lose a
    // binary the SBOM should see.
    for (link_rel, target_rel) in &deferred_links {
        let link_abs = rootfs.join(link_rel);
        let target_abs = rootfs.join(target_rel);
        if let Some(parent) = link_abs.parent() {
            let _ = fs::create_dir_all(parent);
            // v9 Phase N: same chmod fix as the main pass — ensure
            // the parent is owner-writable before linking, or the
            // hard_link / copy fallback will fail with EACCES under
            // Fedora's read-only-by-design directories.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = fs::symlink_metadata(parent) {
                    let mode = meta.permissions().mode();
                    if mode & 0o700 != 0o700 {
                        let mut p = meta.permissions();
                        p.set_mode(mode | 0o700);
                        let _ = fs::set_permissions(parent, p);
                    }
                }
            }
        }
        // If the link already exists from a prior layer, remove it so
        // the new hardlink can be created (fs::hard_link fails when
        // the destination exists).
        if link_abs.exists() {
            let _ = fs::remove_file(&link_abs);
        }
        match fs::hard_link(&target_abs, &link_abs) {
            Ok(()) => {
                paths_written.push(link_rel.to_string_lossy().into_owned());
            }
            Err(hard_err) => match fs::copy(&target_abs, &link_abs) {
                Ok(_) => {
                    tracing::debug!(
                        link = %link_rel.display(),
                        target = %target_rel.display(),
                        "hardlink failed; copied target instead",
                    );
                    paths_written.push(link_rel.to_string_lossy().into_owned());
                }
                Err(copy_err) => {
                    tracing::debug!(
                        link = %link_rel.display(),
                        target = %target_rel.display(),
                        hard_err = %hard_err,
                        copy_err = %copy_err,
                        "hardlink + copy both failed; entry dropped",
                    );
                }
            },
        }
    }

    Ok(paths_written)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// `Seek` is unused in the public surface but kept in the import list so
// future streaming extractors (layer decompression) don't regress.
#[allow(dead_code)]
fn _keep_seek_in_scope<T: Seek>(_: T) {
    let _ = SeekFrom::Start(0);
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// Build a minimal but spec-compliant docker-save tarball in memory:
    /// outer tar holding `manifest.json` + one layer tar (which itself
    /// contains a single file at `usr/local/bin/rg`).
    fn build_fake_image(layer_name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        // Build inner layer tar.
        let mut layer_bytes = Vec::new();
        {
            let mut layer_tar = tar::Builder::new(&mut layer_bytes);
            for (path, content) in files {
                let mut header = tar::Header::new_ustar();
                header.set_path(path).unwrap();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                layer_tar.append(&header, *content).unwrap();
            }
            layer_tar.finish().unwrap();
        }

        // Manifest referring to that layer by name.
        let manifest = format!(
            r#"[{{"Config":"config.json","RepoTags":["demo:latest"],"Layers":["{layer_name}"]}}]"#
        );

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        let file = tmp.reopen().unwrap();
        let mut outer = tar::Builder::new(file);

        let mut manifest_header = tar::Header::new_ustar();
        manifest_header.set_path("manifest.json").unwrap();
        manifest_header.set_size(manifest.len() as u64);
        manifest_header.set_mode(0o644);
        manifest_header.set_cksum();
        outer.append(&manifest_header, manifest.as_bytes()).unwrap();

        let mut layer_header = tar::Header::new_ustar();
        layer_header.set_path(layer_name).unwrap();
        layer_header.set_size(layer_bytes.len() as u64);
        layer_header.set_mode(0o644);
        layer_header.set_cksum();
        outer.append(&layer_header, layer_bytes.as_slice()).unwrap();

        outer.into_inner().unwrap().flush().unwrap();
        // Forget the tmp so it isn't dropped+removed before the test reads it.
        let _ = tmp.persist(&path);
        path
    }

    #[test]
    fn manifest_json_with_null_repo_tags_deserializes() {
        // Regression: `docker save` on an image pulled by digest writes
        // `"RepoTags": null` (a present field with literal null), which
        // a `Vec<String>` field with `#[serde(default)]` did NOT tolerate
        // (default only catches *missing* fields). Discovered during
        // milestone-132 MVP SC verification when scanning the audit image
        // by `@sha256:<digest>`. The fix is `Option<Vec<String>>` +
        // `unwrap_or_default()` at the use-site.
        let manifest = r#"[{"Config":"config.json","RepoTags":null,"Layers":["layer0/layer.tar"]}]"#;
        let entries: Vec<DockerManifestEntry> =
            serde_json::from_str(manifest).expect("null RepoTags must deserialize");
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].repo_tags.is_none(),
            "null RepoTags should land as Option::None, not be coerced to []",
        );
        assert_eq!(entries[0].layers, vec!["layer0/layer.tar".to_string()]);
    }

    #[test]
    fn manifest_json_with_missing_repo_tags_deserializes() {
        // Companion: `#[serde(default)]` still covers the missing-field
        // case. Preserved coverage so future maintainers don't drop the
        // `default` attribute thinking `Option` alone is enough — both
        // missing AND null must work.
        let manifest = r#"[{"Config":"config.json","Layers":["layer0/layer.tar"]}]"#;
        let entries: Vec<DockerManifestEntry> =
            serde_json::from_str(manifest).expect("missing RepoTags must deserialize");
        assert!(entries[0].repo_tags.is_none());
    }

    #[test]
    fn manifest_json_with_populated_repo_tags_deserializes() {
        // Companion: tagged-image path still works (the existing
        // build_fake_image helper exercises this end-to-end; the focused
        // deserializer test below is here so all three RepoTags states
        // are covered in one spot).
        let manifest = r#"[{"Config":"config.json","RepoTags":["demo:latest"],"Layers":["layer0/layer.tar"]}]"#;
        let entries: Vec<DockerManifestEntry> =
            serde_json::from_str(manifest).expect("populated RepoTags must deserialize");
        assert_eq!(
            entries[0].repo_tags.as_deref(),
            Some(&["demo:latest".to_string()][..]),
        );
    }

    #[test]
    fn extract_minimal_image_populates_rootfs() {
        let tarball = build_fake_image(
            "layer0/layer.tar",
            &[("usr/local/bin/rg", b"rg-binary-bytes")],
        );

        let img = extract(&tarball).expect("extract");
        assert_eq!(img.repo_tag.as_deref(), Some("demo:latest"));
        assert_eq!(img.manifest_digest.len(), 64);
        let rg = img.rootfs.join("usr/local/bin/rg");
        assert!(rg.is_file(), "rootfs should contain unpacked file: {rg:?}");
        let content = fs::read(&rg).unwrap();
        assert_eq!(content, b"rg-binary-bytes");
    }

    #[test]
    fn extract_populates_layer_path_map() {
        // Milestone 133 US2.2 (FR-013): every file written by a layer
        // appears in `layer_path_map` keyed by the rootfs-relative path,
        // with the value being `sha256:<hex>` of the layer-blob bytes.
        let tarball = build_fake_image(
            "layer0/layer.tar",
            &[("usr/local/bin/rg", b"rg-binary-bytes")],
        );

        let img = extract(&tarball).expect("extract");
        let digest = img.layer_path_map.get("usr/local/bin/rg");
        assert!(digest.is_some(), "rootfs file should appear in layer_path_map");
        let d = digest.unwrap();
        assert!(d.starts_with("sha256:"), "layer-digest must start with sha256: prefix");
        assert_eq!(d.len(), 7 + 64, "sha256:<64-hex> = 71 chars");
    }

    #[test]
    fn extract_layer_path_map_later_layer_wins_when_same_path() {
        // Two layers each write `etc/config` with different content. The
        // map should carry the LATER layer's digest (OCI overlay
        // semantics — later layers shadow earlier writes; "which layer
        // introduced this content" wants the latest writer).
        let outer = {
            let tmp = tempfile::NamedTempFile::new().unwrap();
            let path = tmp.path().to_path_buf();
            let file = tmp.reopen().unwrap();
            let mut outer_tar = tar::Builder::new(file);
            let mut l0 = Vec::new();
            {
                let mut t = tar::Builder::new(&mut l0);
                let mut h = tar::Header::new_ustar();
                h.set_path("etc/config").unwrap();
                h.set_size(8);
                h.set_mode(0o644);
                h.set_cksum();
                t.append(&h, b"layer0\n\n".as_slice()).unwrap();
                t.finish().unwrap();
            }
            let mut l1 = Vec::new();
            {
                let mut t = tar::Builder::new(&mut l1);
                let mut h = tar::Header::new_ustar();
                h.set_path("etc/config").unwrap();
                h.set_size(8);
                h.set_mode(0o644);
                h.set_cksum();
                t.append(&h, b"layer1\n\n".as_slice()).unwrap();
                t.finish().unwrap();
            }
            let manifest = r#"[{"Config":"config.json","RepoTags":["overlay:latest"],"Layers":["l0/layer.tar","l1/layer.tar"]}]"#;
            let mut h = tar::Header::new_ustar();
            h.set_path("manifest.json").unwrap();
            h.set_size(manifest.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer_tar.append(&h, manifest.as_bytes()).unwrap();
            let mut h = tar::Header::new_ustar();
            h.set_path("l0/layer.tar").unwrap();
            h.set_size(l0.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer_tar.append(&h, l0.as_slice()).unwrap();
            let mut h = tar::Header::new_ustar();
            h.set_path("l1/layer.tar").unwrap();
            h.set_size(l1.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer_tar.append(&h, l1.as_slice()).unwrap();
            outer_tar.into_inner().unwrap().flush().unwrap();
            tmp.persist(&path).unwrap();
            path
        };

        let img = extract(&outer).expect("extract");
        // Compute the digest we expect: SHA-256 of l1's bytes.
        let l1_bytes = read_entry(&outer, "l1/layer.tar").unwrap();
        let expected = format!("sha256:{}", sha256_hex(&l1_bytes));
        assert_eq!(img.layer_path_map.get("etc/config"), Some(&expected));
    }

    #[test]
    fn whiteout_removes_earlier_layer_file() {
        // Layer 0 adds a file; layer 1 whites it out.
        let outer = {
            let tmp = tempfile::NamedTempFile::new().unwrap();
            let path = tmp.path().to_path_buf();
            let file = tmp.reopen().unwrap();
            let mut outer_tar = tar::Builder::new(file);

            // Inner layer 0
            let mut l0 = Vec::new();
            {
                let mut t = tar::Builder::new(&mut l0);
                let mut h = tar::Header::new_ustar();
                h.set_path("etc/config").unwrap();
                h.set_size(4);
                h.set_mode(0o644);
                h.set_cksum();
                t.append(&h, b"old\n".as_slice()).unwrap();
                t.finish().unwrap();
            }
            // Inner layer 1: whiteout
            let mut l1 = Vec::new();
            {
                let mut t = tar::Builder::new(&mut l1);
                let mut h = tar::Header::new_ustar();
                h.set_path("etc/.wh.config").unwrap();
                h.set_size(0);
                h.set_mode(0o644);
                h.set_cksum();
                t.append(&h, &[][..]).unwrap();
                t.finish().unwrap();
            }

            let manifest = r#"[{"Config":"config.json","RepoTags":["wh:latest"],"Layers":["l0/layer.tar","l1/layer.tar"]}]"#;

            let mut h = tar::Header::new_ustar();
            h.set_path("manifest.json").unwrap();
            h.set_size(manifest.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer_tar.append(&h, manifest.as_bytes()).unwrap();
            let mut h = tar::Header::new_ustar();
            h.set_path("l0/layer.tar").unwrap();
            h.set_size(l0.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer_tar.append(&h, l0.as_slice()).unwrap();
            let mut h = tar::Header::new_ustar();
            h.set_path("l1/layer.tar").unwrap();
            h.set_size(l1.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            outer_tar.append(&h, l1.as_slice()).unwrap();
            outer_tar.into_inner().unwrap().flush().unwrap();
            tmp.persist(&path).unwrap();
            path
        };

        let img = extract(&outer).expect("extract");
        let etc_config = img.rootfs.join("etc/config");
        assert!(!etc_config.exists(), "whiteout should have removed etc/config");
    }

    #[test]
    fn missing_manifest_errors() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let file = tmp.reopen().unwrap();
        let outer = tar::Builder::new(file);
        // Empty archive — no manifest.json
        outer.into_inner().unwrap().flush().unwrap();

        let err = extract(tmp.path()).expect_err("expected failure");
        let msg = format!("{err:#}");
        assert!(msg.contains("manifest.json"), "error should mention manifest: {msg}");
    }

    // --- v6 Phase F: hardlink two-pass extraction ---

    /// Build a tar layer (uncompressed) containing two entries whose
    /// ORDER PUTS THE HARDLINK BEFORE ITS TARGET. Returns the tar bytes
    /// ready to hand to `extract_layer_over_rootfs`.
    fn build_tar_with_out_of_order_hardlink() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        let mut ar = tar::Builder::new(&mut buf);

        // Entry 1: a hardlink at usr/bin/rpm2archive pointing to usr/bin/rpm.
        // Target doesn't exist yet — this is the scenario that breaks
        // single-pass extraction.
        let mut hdr = tar::Header::new_gnu();
        hdr.set_path("usr/bin/rpm2archive").unwrap();
        hdr.set_size(0);
        hdr.set_entry_type(tar::EntryType::Link);
        hdr.set_link_name("usr/bin/rpm").unwrap();
        hdr.set_mode(0o755);
        hdr.set_cksum();
        ar.append(&hdr, std::io::empty()).unwrap();

        // Entry 2: the target file with actual contents.
        let contents = b"fake rpm binary contents\n";
        let mut hdr2 = tar::Header::new_gnu();
        hdr2.set_path("usr/bin/rpm").unwrap();
        hdr2.set_size(contents.len() as u64);
        hdr2.set_entry_type(tar::EntryType::Regular);
        hdr2.set_mode(0o755);
        hdr2.set_cksum();
        ar.append(&hdr2, contents.as_slice()).unwrap();

        ar.into_inner().unwrap();
        buf
    }

    #[test]
    fn hardlink_out_of_order_extracts_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let tar = build_tar_with_out_of_order_hardlink();
        extract_layer_over_rootfs(&tar, dir.path()).unwrap();

        let target = dir.path().join("usr/bin/rpm");
        let link = dir.path().join("usr/bin/rpm2archive");
        assert!(target.is_file(), "target binary must extract");
        assert!(
            link.is_file(),
            "hardlink must resolve post-extract (out-of-order v6 fix)"
        );
        let target_bytes = std::fs::read(&target).unwrap();
        let link_bytes = std::fs::read(&link).unwrap();
        assert_eq!(
            target_bytes, link_bytes,
            "hardlink contents must match target"
        );
    }

    /// Issue #399 — hand-write a single-entry ustar tar containing a
    /// `..`-bearing path. The `tar` crate's `Header::set_path`
    /// rejects `..` at the writer API (safe-by-default), so a
    /// malicious tarball can only be simulated by writing the raw
    /// 512-byte header bytes directly. Returns a complete tar stream
    /// (header + two trailing zero blocks).
    fn build_unsafe_tar_with_path(name: &[u8]) -> Vec<u8> {
        assert!(name.len() <= 100, "ustar name field is 100 bytes max");
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name);
        // mode = 0644 → "0000644\0"
        header[100..108].copy_from_slice(b"0000644\0");
        // uid + gid = "0000000\0"
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        // size = 0 → "00000000000\0"
        header[124..136].copy_from_slice(b"00000000000\0");
        // mtime = 0
        header[136..148].copy_from_slice(b"00000000000\0");
        // checksum field: 8 spaces during the calc, then ASCII octal + space + NUL
        header[148..156].copy_from_slice(b"        ");
        // typeflag = '0' (regular file)
        header[156] = b'0';
        // magic: ustar with two-space version
        header[257..263].copy_from_slice(b"ustar ");
        header[263..265].copy_from_slice(b" \0");
        // Compute checksum (sum of all 512 header bytes, treating
        // the checksum field as spaces).
        let cksum: u32 = header.iter().map(|b| *b as u32).sum();
        let cksum_str = format!("{cksum:06o}\0 ");
        header[148..156].copy_from_slice(cksum_str.as_bytes());

        let mut buf = Vec::with_capacity(512 * 3);
        buf.extend_from_slice(&header);
        // Two 512-byte zero blocks signal end-of-archive.
        buf.extend_from_slice(&[0u8; 512]);
        buf.extend_from_slice(&[0u8; 512]);
        buf
    }

    /// Issue #399 — a malicious image with a whiteout entry whose
    /// path contains `..` MUST NOT cause `extract_layer_over_rootfs`
    /// to delete files outside the extraction tempdir. Pre-#399 the
    /// whiteout collection loop accepted the entry path as-is, and
    /// the cleanup loop's `rootfs.join(<wh-path>)` + `fs::remove_*`
    /// resolved `..` during the stat call — granting a malicious
    /// image a delete primitive on the operator's host filesystem.
    /// Issue #401 — tar Symlink entries with absolute targets must
    /// be rewritten at extraction time so the resulting symlink
    /// resolves inside the rootfs. Without this, every downstream
    /// reader (deep-hash, dpkg, apk, os_release) that touches the
    /// path follows the absolute target to the HOST filesystem.
    #[cfg(unix)]
    #[test]
    fn absolute_symlink_target_is_rewritten_to_rootfs_anchored_form() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();

        let mut buf: Vec<u8> = Vec::new();
        {
            let mut ar = tar::Builder::new(&mut buf);

            let mut h1 = tar::Header::new_gnu();
            h1.set_path("var/run").unwrap();
            h1.set_size(0);
            h1.set_entry_type(tar::EntryType::Symlink);
            h1.set_link_name("/run").unwrap();
            h1.set_mode(0o777);
            h1.set_cksum();
            ar.append(&h1, std::io::empty()).unwrap();

            let mut h2 = tar::Header::new_gnu();
            h2.set_path("usr/lib64").unwrap();
            h2.set_size(0);
            h2.set_entry_type(tar::EntryType::Symlink);
            h2.set_link_name("/lib64").unwrap();
            h2.set_mode(0o777);
            h2.set_cksum();
            ar.append(&h2, std::io::empty()).unwrap();

            let mut h3 = tar::Header::new_gnu();
            h3.set_path("var/lib/dpkg/status").unwrap();
            h3.set_size(0);
            h3.set_entry_type(tar::EntryType::Symlink);
            h3.set_link_name("/etc/passwd").unwrap();
            h3.set_mode(0o644);
            h3.set_cksum();
            ar.append(&h3, std::io::empty()).unwrap();

            ar.into_inner().unwrap();
        }

        extract_layer_over_rootfs(&buf, &rootfs).unwrap();

        assert_eq!(
            std::fs::read_link(rootfs.join("var/run")).unwrap(),
            PathBuf::from("../run"),
            "var/run -> /run must rewrite to ../run"
        );
        assert_eq!(
            std::fs::read_link(rootfs.join("usr/lib64")).unwrap(),
            PathBuf::from("../lib64"),
            "usr/lib64 -> /lib64 must rewrite to ../lib64"
        );
        assert_eq!(
            std::fs::read_link(rootfs.join("var/lib/dpkg/status")).unwrap(),
            PathBuf::from("../../../etc/passwd"),
            "var/lib/dpkg/status -> /etc/passwd must rewrite to ../../../etc/passwd"
        );

        // Sandbox property: every rewritten symlink must be relative.
        for p in ["var/run", "usr/lib64", "var/lib/dpkg/status"] {
            let target = std::fs::read_link(rootfs.join(p)).unwrap();
            assert!(
                !target.is_absolute(),
                "rewritten symlink at {p} target {target:?} must be relative"
            );
        }
    }

    /// Issue #401 — relative symlink targets (already in-image-safe)
    /// pass through unchanged.
    #[cfg(unix)]
    #[test]
    fn relative_symlink_targets_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let rootfs = dir.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();

        let mut buf: Vec<u8> = Vec::new();
        {
            let mut ar = tar::Builder::new(&mut buf);
            let mut h = tar::Header::new_gnu();
            h.set_path("opt/symlink").unwrap();
            h.set_size(0);
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_link_name("../in-rootfs-target").unwrap();
            h.set_mode(0o777);
            h.set_cksum();
            ar.append(&h, std::io::empty()).unwrap();
            ar.into_inner().unwrap();
        }
        extract_layer_over_rootfs(&buf, &rootfs).unwrap();

        assert_eq!(
            std::fs::read_link(rootfs.join("opt/symlink")).unwrap(),
            PathBuf::from("../in-rootfs-target"),
            "relative symlinks must pass through unchanged"
        );
    }

    #[test]
    fn whiteout_with_parent_dir_escape_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        // Layout:
        //   <tmp>/rootfs/         (extraction sandbox)
        //   <tmp>/host-area/sentinel  (a host file the attacker wants to delete)
        let rootfs = dir.path().join("rootfs");
        let host_area = dir.path().join("host-area");
        std::fs::create_dir_all(&rootfs).unwrap();
        std::fs::create_dir_all(&host_area).unwrap();
        let sentinel = host_area.join("sentinel");
        std::fs::write(&sentinel, b"important host content").unwrap();

        // Build a tar containing a whiteout entry whose path uses
        // `..` to escape the rootfs:
        //   ../host-area/.wh.sentinel
        // Pre-#399, this would resolve at cleanup time to
        //   <rootfs>/../host-area/sentinel = <tmp>/host-area/sentinel
        // and the file would be deleted by `fs::remove_file`.
        //
        // The `tar` crate's `Header::set_path` rejects `..` at the
        // writer API (safe-by-default). To simulate a malicious
        // tarball we write the 512-byte ustar header bytes directly.
        let buf = build_unsafe_tar_with_path(b"../host-area/.wh.sentinel");

        // Extraction should complete cleanly — the unsafe whiteout
        // is silently dropped, not propagated.
        extract_layer_over_rootfs(&buf, &rootfs).unwrap();

        assert!(
            sentinel.is_file(),
            "host sentinel file must still exist after extracting a malicious whiteout (#399 regression)"
        );
    }

    #[test]
    fn hardlink_missing_target_does_not_crash() {
        // Tar with a hardlink whose target is never written. The
        // deferred-link pass should log debug and move on without
        // panicking.
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut ar = tar::Builder::new(&mut buf);
            let mut hdr = tar::Header::new_gnu();
            hdr.set_path("usr/bin/orphan").unwrap();
            hdr.set_size(0);
            hdr.set_entry_type(tar::EntryType::Link);
            hdr.set_link_name("usr/bin/never-existed").unwrap();
            hdr.set_mode(0o755);
            hdr.set_cksum();
            ar.append(&hdr, std::io::empty()).unwrap();
            ar.into_inner().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        // Should not panic or error — the hardlink is silently dropped.
        extract_layer_over_rootfs(&buf, dir.path()).unwrap();
        assert!(
            !dir.path().join("usr/bin/orphan").exists(),
            "orphan hardlink should not exist when target is missing"
        );
    }

    /// v7 Phase I — a tar containing a file entry whose parent
    /// directories were never declared as separate tar entries must
    /// still extract (the tar crate's default `unpack_in` fails in
    /// this case; we pre-create parents to fix it). Simulates the
    /// Fedora image layer pattern that dropped rpmdb.sqlite and rpm
    /// binaries.
    #[test]
    fn unpack_layer_creates_missing_parent_dirs() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut ar = tar::Builder::new(&mut buf);
            // Deep path with no intermediate directory entries. Matches
            // the Fedora `usr/lib/sysimage/rpm/rpmdb.sqlite` layout.
            let contents = b"synthetic payload\n";
            let mut hdr = tar::Header::new_gnu();
            hdr.set_path("usr/lib/sysimage/rpm/rpmdb.sqlite").unwrap();
            hdr.set_size(contents.len() as u64);
            hdr.set_entry_type(tar::EntryType::Regular);
            hdr.set_mode(0o644);
            hdr.set_cksum();
            ar.append(&hdr, contents.as_slice()).unwrap();
            ar.into_inner().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        extract_layer_over_rootfs(&buf, dir.path()).unwrap();
        let target = dir.path().join("usr/lib/sysimage/rpm/rpmdb.sqlite");
        assert!(
            target.is_file(),
            "deep-path file must extract even without intermediate dir entries"
        );
        let observed = std::fs::read(&target).unwrap();
        assert_eq!(observed, b"synthetic payload\n");
    }

    // --- v9 Phase N: read-only directories mustn't block extraction ---

    /// N1 — a tar layer containing (a) a directory with mode 0555
    /// followed by (b) a regular-file entry inside that directory
    /// must produce BOTH the dir and the file on disk. Without the
    /// chmod fix, (b) fails with EACCES because (a) left the parent
    /// non-writable.
    #[cfg(unix)]
    #[test]
    fn unpack_layer_survives_readonly_dir() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut ar = tar::Builder::new(&mut buf);
            let mut dhdr = tar::Header::new_gnu();
            dhdr.set_path("readonly/").unwrap();
            dhdr.set_size(0);
            dhdr.set_entry_type(tar::EntryType::Directory);
            dhdr.set_mode(0o555);
            dhdr.set_cksum();
            ar.append(&dhdr, std::io::empty()).unwrap();

            let contents = b"hello\n";
            let mut fhdr = tar::Header::new_gnu();
            fhdr.set_path("readonly/file.txt").unwrap();
            fhdr.set_size(contents.len() as u64);
            fhdr.set_entry_type(tar::EntryType::Regular);
            fhdr.set_mode(0o644);
            fhdr.set_cksum();
            ar.append(&fhdr, contents.as_slice()).unwrap();
            ar.into_inner().unwrap();
        }

        let dir = tempfile::tempdir().unwrap();
        extract_layer_over_rootfs(&buf, dir.path()).unwrap();

        assert!(dir.path().join("readonly").is_dir());
        let f = dir.path().join("readonly/file.txt");
        assert!(f.is_file(), "file inside read-only dir must extract");
        assert_eq!(std::fs::read(&f).unwrap(), b"hello\n");
    }

    /// N2 — realistic Fedora-style layout: mode-0555 directory with
    /// multiple symlink entries inside. Mirrors
    /// `/etc/pki/ca-trust/extracted/pem/directory-hash/` behaviour
    /// where the polyglot extraction failed thousands of times.
    #[cfg(unix)]
    #[test]
    fn unpack_layer_survives_readonly_symlink_chain() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut ar = tar::Builder::new(&mut buf);

            // Read-only parent dir
            let mut dhdr = tar::Header::new_gnu();
            dhdr.set_path("trust/hashes/").unwrap();
            dhdr.set_size(0);
            dhdr.set_entry_type(tar::EntryType::Directory);
            dhdr.set_mode(0o555);
            dhdr.set_cksum();
            ar.append(&dhdr, std::io::empty()).unwrap();

            // 5 symlink entries inside
            for i in 0..5 {
                let mut shdr = tar::Header::new_gnu();
                shdr.set_path(format!("trust/hashes/hash_{i}.0")).unwrap();
                shdr.set_size(0);
                shdr.set_entry_type(tar::EntryType::Symlink);
                shdr.set_link_name(format!("cert_{i}.pem")).unwrap();
                shdr.set_mode(0o777);
                shdr.set_cksum();
                ar.append(&shdr, std::io::empty()).unwrap();
            }
            ar.into_inner().unwrap();
        }

        let dir = tempfile::tempdir().unwrap();
        extract_layer_over_rootfs(&buf, dir.path()).unwrap();

        for i in 0..5 {
            let p = dir.path().join(format!("trust/hashes/hash_{i}.0"));
            assert!(
                p.symlink_metadata().is_ok(),
                "symlink {} must extract even under a 0555 parent dir",
                p.display()
            );
        }
    }

    // ----------------------------------------------------------------
    // Issue #281 — ko-built images: layer entries with absolute paths
    // (`/ko-app/<binary>`) MUST be extracted under the rootfs, NOT
    // silently dropped. The pre-fix behavior rejected anything where
    // `path.is_absolute() == true`, which dropped the entire ko app
    // layer and produced an SBOM missing the Go binary + all its
    // BuildInfo-embedded Go modules. The fix strips the leading `/`
    // and unpacks; `..` components remain rejected (real escape risk).
    // ----------------------------------------------------------------
    //
    // The `tar` crate's `Header::set_path` validates and rejects
    // absolute paths + `..` components at the WRITE side, so we
    // can't synthesize these tarball shapes with the standard
    // `build_fake_image` helper (which uses `set_path` under the
    // hood). Real ko / docker save tarballs are built by other tools
    // whose writers don't have this defense — that's how the absolute
    // paths get into real-world tar streams that waybill READS. Below
    // we write the raw 512-byte ustar header bytes directly to mimic
    // those real tarballs.

    /// Encode a ustar-format tar header for a regular file entry at
    /// `name` with the given byte payload. Returns the 512-byte
    /// header bytes. Bypasses `tar::Header::set_path`'s validation
    /// so we can put absolute paths and `..` paths through the
    /// reader — exactly the inputs that real ko / docker save
    /// tarballs produce.
    fn raw_ustar_header(name: &str, content_len: usize) -> [u8; 512] {
        let mut header = [0u8; 512];
        // Name field: bytes 0-99 (NUL-padded). ustar allows up to 100
        // bytes here; the prefix field at byte 345 can hold an
        // additional 155, but for our test names this is plenty.
        let name_bytes = name.as_bytes();
        let n = name_bytes.len().min(100);
        header[..n].copy_from_slice(&name_bytes[..n]);
        // Mode: bytes 100-107, octal NUL-terminated. "0000644 ".
        header[100..108].copy_from_slice(b"0000644\0");
        // UID/GID: bytes 108-123, octal "0000000 ".
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        // Size: bytes 124-135, 11 octal digits + NUL.
        let size_str = format!("{content_len:011o}\0");
        header[124..136].copy_from_slice(size_str.as_bytes());
        // Mtime: bytes 136-147, octal NUL.
        header[136..148].copy_from_slice(b"00000000000\0");
        // Checksum field is bytes 148-155; per spec, compute as the
        // sum of all bytes treating this field as 8 spaces.
        header[148..156].copy_from_slice(b"        ");
        // Type flag (byte 156): '0' for regular file.
        header[156] = b'0';
        // ustar magic: bytes 257-263 = "ustar\0", then 264-265 = "00".
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        // Compute checksum.
        let sum: u32 = header.iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(cksum_str.as_bytes());
        header
    }

    /// Build a layer.tar with one regular-file entry per (name,
    /// content) tuple. Names may be absolute (start with `/`) or
    /// contain `..` — useful for exercising the entry-path
    /// liberalization fix from #281.
    fn build_raw_layer_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, content) in entries {
            let hdr = raw_ustar_header(name, content.len());
            out.extend_from_slice(&hdr);
            out.extend_from_slice(content);
            // Pad content to 512-byte block boundary.
            let pad = (512 - (content.len() % 512)) % 512;
            out.extend(std::iter::repeat_n(0u8, pad));
        }
        // Two 512-byte zero blocks mark end-of-archive.
        out.extend(std::iter::repeat_n(0u8, 1024));
        out
    }

    #[test]
    fn absolute_path_entry_extracts_at_rootfs_root() {
        // Regression test for issue #281. Real ko-built layers carry
        // entries like `/ko-app/<binary>` with leading slash; before
        // the fix these were silently dropped. After the fix, the
        // leading `/` is stripped and the entry extracts at
        // `<rootfs>/ko-app/<binary>`.
        let layer = build_raw_layer_tar(&[("/ko-app/myapp", b"go-binary-bytes")]);
        let dir = tempfile::tempdir().unwrap();
        extract_layer_over_rootfs(&layer, dir.path()).expect("extract layer");
        let app = dir.path().join("ko-app/myapp");
        assert!(
            app.is_file(),
            "absolute-path entry must extract relative to rootfs root: expected {app:?}"
        );
        assert_eq!(fs::read(&app).unwrap(), b"go-binary-bytes");
    }

    #[test]
    fn relative_and_absolute_path_entries_extract_together() {
        // Mixed layer: one relative-path entry (Docker convention)
        // plus one absolute-path entry (ko convention). Both MUST
        // land in the rootfs.
        let layer = build_raw_layer_tar(&[
            ("usr/bin/regular", b"docker-style"),
            ("/ko-app/koapp", b"ko-style"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        extract_layer_over_rootfs(&layer, dir.path()).expect("extract layer");
        let regular = dir.path().join("usr/bin/regular");
        let koapp = dir.path().join("ko-app/koapp");
        assert!(regular.is_file(), "relative-path entry should extract at {regular:?}");
        assert!(koapp.is_file(), "absolute-path entry should extract at {koapp:?}");
        assert_eq!(fs::read(&regular).unwrap(), b"docker-style");
        assert_eq!(fs::read(&koapp).unwrap(), b"ko-style");
    }

    #[test]
    fn parent_dir_escape_still_rejected() {
        // Regression guard for the security property the original
        // check was reaching for: `..` components MUST still be
        // dropped, even after the absolute-path liberalization. If
        // this test fails, the fix went too far and reintroduced a
        // rootfs-escape vulnerability.
        let layer = build_raw_layer_tar(&[
            ("../escaped", b"should-be-dropped"),
            ("safe/file", b"should-extract"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        // The extract may return an error or succeed silently (the
        // current code uses `tracing::debug!` + continue per entry);
        // either way, the assertions below check the on-disk result.
        let _ = extract_layer_over_rootfs(&layer, dir.path());

        assert!(
            dir.path().join("safe/file").is_file(),
            "safe path should extract"
        );
        // `../escaped` would land at `<rootfs>/../escaped` if it
        // weren't rejected — i.e. as a sibling of the rootfs tempdir.
        // The fix's `..`-check MUST prevent this.
        let escape_target = dir.path().parent().unwrap().join("escaped");
        assert!(
            !escape_target.exists(),
            "parent-dir escape MUST be rejected; found leaked file at {escape_target:?}"
        );
    }

    #[test]
    fn absolute_path_with_parent_dir_inside_still_rejected() {
        // Hostile input: `/../../escaped` looks like a leading-slash
        // entry but, after our strip, becomes `../../escaped` —
        // which the `..`-check then catches. Belt-and-suspenders
        // verification that the fix can't be bypassed by adding a
        // leading slash.
        let layer = build_raw_layer_tar(&[("/../../escaped", b"should-be-dropped")]);
        let dir = tempfile::tempdir().unwrap();
        let _ = extract_layer_over_rootfs(&layer, dir.path());
        // Check several candidate escape targets.
        let parent = dir.path().parent().unwrap();
        let candidates = [
            parent.join("escaped"),
            parent.parent().map(|p| p.join("escaped")).unwrap_or_default(),
            dir.path().join("escaped"),
        ];
        for c in &candidates {
            if c.as_os_str().is_empty() {
                continue;
            }
            assert!(
                !c.exists() || c.starts_with(dir.path()),
                "/../../<name> must not escape rootfs; found {c:?}"
            );
        }
    }
}
