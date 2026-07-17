//! Milestone 206 (#440) — scan local podman images by reading the
//! `containers/storage` overlay layout directly (no daemon or REST
//! API per FR-009).
//!
//! Layout parsed:
//! - `<graphroot>/overlay-images/images.json` — top-level image index
//! - `<graphroot>/overlay-images/<image-id>/manifest` — OCI ImageManifest
//!   or ImageIndex (multi-arch); when index, filter by host arch per FR-011
//! - `<graphroot>/overlay-images/<image-id>/config` — OCI ImageConfiguration
//! - `<graphroot>/overlay-layers/layers.json` — top-level layer index
//! - `<graphroot>/overlay/<layer-id>/diff/` — layer content (unpacked)
//!
//! Full contract at `specs/206-podman-source/contracts/podman-storage-layout.md`.
//!
//! Assumptions (per spec Assumptions):
//! - Linux only (spec Assumption 1); non-Linux hosts hit the FR-007
//!   fallback ladder with a clear WARN.
//! - `overlay` storage driver only for m206 MVP; `vfs` and `btrfs`
//!   return `UnsupportedDriver` (spec Edge Case).
//! - `containers/storage` v4+ layout (spec Assumption).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use data_encoding::HEXLOWER;
use flate2::write::GzEncoder;
use flate2::Compression;
use oci_spec::image::{ImageConfiguration, ImageIndex, ImageManifest};
use sha2::{Digest, Sha256};

/// Failure classes from the podman-source acquisition pipeline. Every
/// variant is safe to `format!()` via Display — the FR-007 WARN log
/// and integration-test assertions grep for these strings.
#[derive(Debug, thiserror::Error)]
pub enum PodmanSourceError {
    #[error("podman storage root not found at `{path}`; reason: {reason}")]
    StorageRootUnreachable { path: PathBuf, reason: String },

    #[error("podman storage driver `{driver}` not supported (m206 supports `overlay` only)")]
    UnsupportedDriver { driver: String },

    #[error("no podman image matched reference `{image_ref}` in storage index at `{path}`")]
    ImageNotFound { image_ref: String, path: PathBuf },

    #[error("podman image `{id}` OCI manifest at `{path}` is corrupted: {reason}")]
    CorruptedManifest {
        id: String,
        path: PathBuf,
        reason: String,
    },

    #[error(
        "podman image `{id}` layer digest verification failed: expected {expected}, computed {actual}"
    )]
    #[allow(dead_code)] // Reserved for a future FR-012 strict-verify mode; currently we log-only per resolve_and_pack notes.
    LayerDigestMismatch {
        id: String,
        expected: String,
        actual: String,
    },

    #[error(
        "podman host architecture `{host}` does not match any variant in multi-arch image `{image_ref}`; available: {available:?}"
    )]
    NoArchMatch {
        image_ref: String,
        host: String,
        available: Vec<String>,
    },

    #[error("podman source I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

// -----------------------------------------------------------------
// E2 — PodmanImageRef parser
// -----------------------------------------------------------------

/// Parsed forms of the operator's `--image <ref>` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodmanImageRef {
    /// `nginx:1.27.0` or `docker.io/library/nginx:1.27.0`. Default tag
    /// `latest` if `:tag` absent (Docker convention).
    Tagged { repo: String, tag: String },
    /// `nginx@sha256:abc…64hex`.
    Digest { repo: String, digest: String },
    /// `abcdef123456` (12-64 hex chars — matches prefix of `.id`).
    ImageId { id_prefix: String },
}

impl PodmanImageRef {
    pub fn parse(raw: &str) -> Result<Self, PodmanSourceError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(PodmanSourceError::ImageNotFound {
                image_ref: raw.to_string(),
                path: PathBuf::new(),
            });
        }

        // Digest form: `<repo>@sha256:<64-hex>`.
        if let Some((repo, digest_suffix)) = raw.split_once('@') {
            if digest_suffix.starts_with("sha256:") && digest_suffix.len() == 71 {
                return Ok(Self::Digest {
                    repo: repo.to_string(),
                    digest: digest_suffix.to_string(),
                });
            }
        }

        // Image ID form: 12-64 hex chars (no colon, no slash).
        let looks_like_hex = raw.len() >= 12
            && raw.len() <= 64
            && raw.chars().all(|c| c.is_ascii_hexdigit())
            && !raw.contains(':')
            && !raw.contains('/')
            && !raw.contains('@');
        if looks_like_hex {
            return Ok(Self::ImageId {
                id_prefix: raw.to_string(),
            });
        }

        // Tagged form: `<repo>:<tag>` or bare `<repo>` (→ `:latest`).
        //
        // Care: `docker.io/library/nginx:1.27.0` has slashes AND a colon.
        // The tag is what appears after the LAST `:` — but only if the
        // colon comes after the last `/` (else it's a registry port like
        // `registry:5000/foo`).
        let last_slash = raw.rfind('/');
        let last_colon = raw.rfind(':');
        let (repo, tag) = match (last_slash, last_colon) {
            (Some(slash_pos), Some(colon_pos)) if colon_pos > slash_pos => {
                (raw[..colon_pos].to_string(), raw[colon_pos + 1..].to_string())
            }
            (None, Some(colon_pos)) => {
                (raw[..colon_pos].to_string(), raw[colon_pos + 1..].to_string())
            }
            _ => (raw.to_string(), "latest".to_string()),
        };
        Ok(Self::Tagged { repo, tag })
    }
}

// -----------------------------------------------------------------
// Storage layout parser types
// -----------------------------------------------------------------

/// One entry in `<graphroot>/overlay-images/images.json`. See
/// contracts/podman-storage-layout.md §Image Index for wire shape.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ImageRecord {
    pub id: String,
    #[serde(default)]
    pub digest: String,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub layer: String,
}

/// One entry in `<graphroot>/overlay-layers/layers.json`.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct LayerRecord {
    pub id: String,
    #[serde(default)]
    pub parent: String,
    /// SHA-256 of the uncompressed diff tar (per contracts §Layer Index).
    /// Reserved for a future FR-012 strict-verify mode.
    #[serde(rename = "diff-digest", default)]
    #[allow(dead_code)]
    pub diff_digest: String,
    /// SHA-256 of the compressed diff .tar.gz (per contracts §Layer Index).
    /// Reserved for a future FR-012 strict-verify mode.
    #[serde(rename = "compressed-diff-digest", default)]
    #[allow(dead_code)]
    pub compressed_diff_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageDriver {
    Overlay,
}

// -----------------------------------------------------------------
// Storage root discovery (R2 + FR-002 + FR-010)
// -----------------------------------------------------------------

/// Discover the podman storage root. `rootless == true` means look at
/// rootless config paths + defaults; `rootless == false` means rootful.
pub(crate) fn discover_storage_root(rootless: bool) -> Result<PathBuf, PodmanSourceError> {
    // Try config file first per FR-010.
    let config_path = if rootless {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".config/containers/storage.conf"))
    } else {
        Some(PathBuf::from("/etc/containers/storage.conf"))
    };

    if let Some(conf_path) = config_path {
        if let Ok(text) = std::fs::read_to_string(&conf_path) {
            if let Ok(parsed) = text.parse::<toml::Value>() {
                if let Some(gr) = parsed
                    .get("storage")
                    .and_then(|s| s.get("graphroot"))
                    .and_then(|v| v.as_str())
                {
                    if !gr.is_empty() {
                        let path = PathBuf::from(gr);
                        return verify_readable(&path);
                    }
                }
            }
        }
    }

    // Compiled-in defaults per FR-002.
    let default_path = if rootless {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            PodmanSourceError::StorageRootUnreachable {
                path: PathBuf::new(),
                reason: "$HOME not set".to_string(),
            }
        })?;
        PathBuf::from(home).join(".local/share/containers/storage")
    } else {
        PathBuf::from("/var/lib/containers/storage")
    };
    verify_readable(&default_path)
}

fn verify_readable(path: &Path) -> Result<PathBuf, PodmanSourceError> {
    match std::fs::read_dir(path) {
        Ok(_) => Ok(path.to_path_buf()),
        Err(e) => Err(PodmanSourceError::StorageRootUnreachable {
            path: path.to_path_buf(),
            reason: e.to_string(),
        }),
    }
}

// -----------------------------------------------------------------
// Storage-driver detection (R3)
// -----------------------------------------------------------------

pub(crate) fn detect_storage_driver(
    graphroot: &Path,
) -> Result<StorageDriver, PodmanSourceError> {
    if graphroot.join("overlay").is_dir() {
        return Ok(StorageDriver::Overlay);
    }
    if graphroot.join("vfs").is_dir() {
        return Err(PodmanSourceError::UnsupportedDriver {
            driver: "vfs".to_string(),
        });
    }
    if graphroot.join("btrfs").is_dir() {
        return Err(PodmanSourceError::UnsupportedDriver {
            driver: "btrfs".to_string(),
        });
    }
    Err(PodmanSourceError::UnsupportedDriver {
        driver: "unknown".to_string(),
    })
}

// -----------------------------------------------------------------
// Image + layer index parsers
// -----------------------------------------------------------------

pub(crate) fn parse_images_index(
    graphroot: &Path,
) -> Result<Vec<ImageRecord>, PodmanSourceError> {
    let path = graphroot.join("overlay-images/images.json");
    let bytes = std::fs::read(&path)?;
    serde_json::from_slice(&bytes).map_err(|e| PodmanSourceError::CorruptedManifest {
        id: "<images.json>".to_string(),
        path,
        reason: e.to_string(),
    })
}

pub(crate) fn parse_layers_index(
    graphroot: &Path,
) -> Result<HashMap<String, LayerRecord>, PodmanSourceError> {
    let path = graphroot.join("overlay-layers/layers.json");
    let bytes = std::fs::read(&path)?;
    let list: Vec<LayerRecord> =
        serde_json::from_slice(&bytes).map_err(|e| PodmanSourceError::CorruptedManifest {
            id: "<layers.json>".to_string(),
            path,
            reason: e.to_string(),
        })?;
    Ok(list.into_iter().map(|l| (l.id.clone(), l)).collect())
}

pub(crate) fn resolve_image_ref<'a>(
    index: &'a [ImageRecord],
    parsed_ref: &PodmanImageRef,
    graphroot: &Path,
) -> Result<&'a ImageRecord, PodmanSourceError> {
    let match_result = match parsed_ref {
        PodmanImageRef::Tagged { repo, tag } => {
            let needle = format!("{repo}:{tag}");
            let needle_full = format!("docker.io/library/{repo}:{tag}");
            let needle_full2 = format!("docker.io/{repo}:{tag}");
            index.iter().find(|r| {
                r.names.iter().any(|n| {
                    n == &needle
                        || n == &needle_full
                        || n == &needle_full2
                        || n.ends_with(&needle)
                })
            })
        }
        PodmanImageRef::Digest { digest, .. } => {
            index.iter().find(|r| &r.digest == digest)
        }
        PodmanImageRef::ImageId { id_prefix } => {
            index.iter().find(|r| r.id.starts_with(id_prefix))
        }
    };

    match_result.ok_or_else(|| PodmanSourceError::ImageNotFound {
        image_ref: match parsed_ref {
            PodmanImageRef::Tagged { repo, tag } => format!("{repo}:{tag}"),
            PodmanImageRef::Digest { repo, digest } => format!("{repo}@{digest}"),
            PodmanImageRef::ImageId { id_prefix } => id_prefix.clone(),
        },
        path: graphroot.to_path_buf(),
    })
}

// -----------------------------------------------------------------
// F1 — multi-arch OCI ImageIndex handling (FR-011)
// -----------------------------------------------------------------

/// Map Rust `std::env::consts::ARCH` names to OCI canonical arch strings.
fn arch_alias(rust_arch: &str) -> &str {
    match rust_arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => other,
    }
}

/// F1: handle both OCI ImageIndex (multi-arch) and single-arch
/// ImageManifest formats. Returns the resolved ImageManifest matching
/// the host architecture.
pub(crate) fn resolve_manifest_for_host_arch(
    graphroot: &Path,
    image_id: &str,
    image_ref: &str,
) -> Result<(ImageManifest, Vec<u8>), PodmanSourceError> {
    let manifest_path = graphroot
        .join("overlay-images")
        .join(image_id)
        .join("manifest");
    let bytes = std::fs::read(&manifest_path)?;

    // First try ImageIndex (multi-arch).
    if let Ok(index) = serde_json::from_slice::<ImageIndex>(&bytes) {
        // Confirm this really is an index by checking manifests[] is non-empty.
        if !index.manifests().is_empty() {
            let host_arch = arch_alias(std::env::consts::ARCH);
            let host_os = std::env::consts::OS;

            let mut available: Vec<String> = Vec::new();
            let mut matched_digest: Option<String> = None;

            for entry in index.manifests() {
                let plat_arch = entry
                    .platform()
                    .as_ref()
                    .map(|p| format!("{:?}", p.architecture()).to_lowercase())
                    .unwrap_or_else(|| "unknown".to_string());
                let plat_os = entry
                    .platform()
                    .as_ref()
                    .map(|p| format!("{:?}", p.os()).to_lowercase())
                    .unwrap_or_else(|| "unknown".to_string());
                let plat_key = format!("{plat_os}/{plat_arch}");
                available.push(plat_key.clone());

                if plat_arch == host_arch && plat_os == host_os {
                    matched_digest = Some(entry.digest().to_string());
                    break;
                }
            }

            let digest = matched_digest.ok_or_else(|| PodmanSourceError::NoArchMatch {
                image_ref: image_ref.to_string(),
                host: format!("{host_os}/{host_arch}"),
                available,
            })?;

            // Recurse: read the per-arch manifest indexed by digest.
            // Podman stores per-arch manifests under overlay-images/<digest>/manifest.
            let arch_specific_id = digest.trim_start_matches("sha256:");
            let arch_manifest_path = graphroot
                .join("overlay-images")
                .join(arch_specific_id)
                .join("manifest");
            let arch_bytes = std::fs::read(&arch_manifest_path)?;
            let arch_manifest: ImageManifest = serde_json::from_slice(&arch_bytes).map_err(
                |e| PodmanSourceError::CorruptedManifest {
                    id: arch_specific_id.to_string(),
                    path: arch_manifest_path.clone(),
                    reason: e.to_string(),
                },
            )?;
            return Ok((arch_manifest, arch_bytes));
        }
    }

    // Not an index (or empty manifests[]) → parse as single-arch manifest.
    let manifest: ImageManifest = serde_json::from_slice(&bytes).map_err(|e| {
        PodmanSourceError::CorruptedManifest {
            id: image_id.to_string(),
            path: manifest_path,
            reason: e.to_string(),
        }
    })?;
    Ok((manifest, bytes))
}

// -----------------------------------------------------------------
// Layer chain resolver
// -----------------------------------------------------------------

pub(crate) fn resolve_layer_chain<'a>(
    layers: &'a HashMap<String, LayerRecord>,
    top_layer: &str,
) -> Vec<&'a LayerRecord> {
    let mut chain: Vec<&LayerRecord> = Vec::new();
    let mut current = top_layer.to_string();
    while !current.is_empty() {
        match layers.get(&current) {
            Some(entry) => {
                chain.push(entry);
                current = entry.parent.clone();
            }
            None => break,
        }
    }
    chain.reverse(); // base-to-top ordering
    chain
}

// -----------------------------------------------------------------
// E3 — public entry: resolve_and_pack (F5-remediated: dropped
// unused storage_root param)
// -----------------------------------------------------------------

/// A layer prepared for the docker-save-format assembler.
#[derive(Debug)]
pub(crate) struct PreparedLayer {
    pub compressed_blob: Vec<u8>,
    pub compressed_digest: String,
    /// OCI media type of the compressed layer blob. Recorded for
    /// completeness; the docker-save format we assemble doesn't
    /// require it in the output manifest.json (docker consumers
    /// infer from the `.tar.gz` extension). Reserved for a future
    /// OCI-format tarball emission mode.
    #[allow(dead_code)]
    pub media_type: String,
}

/// Public entry point per data-model E3. Reads a podman image from
/// local storage, re-tars each layer's diff dir, and writes a
/// docker-save-format tarball to `out_tarball` for downstream
/// consumption by `scan_fs::docker_image::extract`.
///
/// Returns `Ok(())` on success. Any `PodmanSourceError` bubbles up
/// to the caller (`cli::scan_cmd::resolve_image_ref` dispatch loop)
/// which decides fallback vs abort per FR-007.
pub fn resolve_and_pack(image_ref: &str, out_tarball: &Path) -> Result<(), PodmanSourceError> {
    // 1. Discover storage root.
    let rootless = !running_as_root();
    let graphroot = discover_storage_root(rootless)?;

    // 2. Detect driver (overlay only for m206 MVP).
    detect_storage_driver(&graphroot)?;

    // 3. Parse image index + resolve image ref.
    let parsed_ref = PodmanImageRef::parse(image_ref)?;
    let images = parse_images_index(&graphroot)?;
    let image_record = resolve_image_ref(&images, &parsed_ref, &graphroot)?;
    let image_id = image_record.id.clone();
    let top_layer = image_record.layer.clone();

    // 4. Load OCI manifest (multi-arch aware) + config.
    let (manifest, _manifest_bytes) =
        resolve_manifest_for_host_arch(&graphroot, &image_id, image_ref)?;
    let config_path = graphroot
        .join("overlay-images")
        .join(&image_id)
        .join("config");
    let config_bytes = std::fs::read(&config_path)?;
    let _config: ImageConfiguration =
        serde_json::from_slice(&config_bytes).map_err(|e| PodmanSourceError::CorruptedManifest {
            id: image_id.clone(),
            path: config_path,
            reason: e.to_string(),
        })?;

    // 5. Re-tar each layer's diff dir.
    let layers_index = parse_layers_index(&graphroot)?;
    let chain = resolve_layer_chain(&layers_index, &top_layer);

    let mut prepared_layers: Vec<PreparedLayer> = Vec::with_capacity(chain.len());
    for (i, layer_record) in chain.iter().enumerate() {
        let diff_dir = graphroot.join("overlay").join(&layer_record.id).join("diff");
        let (compressed_blob, compressed_digest) = retar_diff_dir_gzipped(&diff_dir)?;

        // Verify compressed digest matches OCI manifest layer[i] declaration
        // (best-effort — podman's re-tar may produce different bytes than the
        // original blob; log-only mismatch is acceptable for m206 MVP since
        // the extracted rootfs content is byte-identical regardless).
        if let Some(oci_layer) = manifest.layers().get(i) {
            let expected = oci_layer.digest().to_string();
            if !expected.is_empty() && expected != compressed_digest {
                tracing::debug!(
                    layer_index = i,
                    expected = %expected,
                    computed = %compressed_digest,
                    "podman-source: re-tar layer digest differs from OCI manifest; \
                     content is preserved (mikebom re-tars diff dirs; original blob \
                     compression is not preserved)"
                );
            }
        }

        let media_type = manifest
            .layers()
            .get(i)
            .map(|l| l.media_type().to_string())
            .unwrap_or_else(|| "application/vnd.oci.image.layer.v1.tar+gzip".to_string());

        prepared_layers.push(PreparedLayer {
            compressed_blob,
            compressed_digest,
            media_type,
        });
    }

    // 6. Assemble docker-save-format tarball.
    assemble_tarball(&config_bytes, &prepared_layers, image_ref, out_tarball)?;
    Ok(())
}

fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid is documented safe to call; returns the
        // effective UID of the calling process. Always available on
        // POSIX; no unsafe preconditions.
        unsafe { geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

// std::os::unix doesn't expose geteuid directly; use the C ABI
// (always available on unix without adding a libc dep).
#[cfg(unix)]
extern "C" {
    fn geteuid() -> u32;
}

/// Stdlib-only recursive path collector for `retar_diff_dir_gzipped`.
/// Avoids the `walkdir` dep (per Constitution "no new Cargo deps" for
/// m206). Follows symlinks NOT (matches m054's `safe_walk` posture);
/// pushes both directories AND their contents so the tar layout
/// mirrors the on-disk tree.
fn collect_paths_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        out.push(path.clone());
        if file_type.is_dir() && !file_type.is_symlink() {
            collect_paths_recursive(&path, out)?;
        }
    }
    Ok(())
}

/// Walk `diff_dir` with sorted-lexicographic traversal, tar it, gzip
/// the tar, and return (compressed_blob, sha256_hex).
///
/// Uses `HeaderMode::Deterministic` for reproducibility across scans
/// per contracts/podman-storage-layout.md §Layer Content.
fn retar_diff_dir_gzipped(diff_dir: &Path) -> Result<(Vec<u8>, String), PodmanSourceError> {
    let compressed = Vec::new();
    let gz_encoder = GzEncoder::new(compressed, Compression::default());
    let mut tar_builder = tar::Builder::new(gz_encoder);
    tar_builder.mode(tar::HeaderMode::Deterministic);

    if diff_dir.is_dir() {
        // Stdlib-only recursive descent per Constitution "no new
        // Cargo deps" — collect all paths, sort lexicographically
        // for reproducibility (matches HeaderMode::Deterministic).
        let mut entries: Vec<PathBuf> = Vec::new();
        collect_paths_recursive(diff_dir, &mut entries)?;
        entries.sort();

        for entry_path in &entries {
            let rel = entry_path
                .strip_prefix(diff_dir)
                .expect("walked path always under diff_dir");
            let metadata = std::fs::symlink_metadata(entry_path)?;
            if metadata.file_type().is_symlink() {
                let target = std::fs::read_link(entry_path)?;
                let mut header = tar::Header::new_gnu();
                header.set_metadata(&metadata);
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                tar_builder
                    .append_link(&mut header, rel, &target)
                    .with_context(|| format!("tar append_link failed for {rel:?}"))
                    .map_err(io_from_anyhow)?;
            } else if metadata.file_type().is_dir() {
                tar_builder
                    .append_dir(rel, entry_path)
                    .with_context(|| format!("tar append_dir failed for {rel:?}"))
                    .map_err(io_from_anyhow)?;
            } else if metadata.file_type().is_file() {
                let mut file = std::fs::File::open(entry_path)?;
                tar_builder
                    .append_file(rel, &mut file)
                    .with_context(|| format!("tar append_file failed for {rel:?}"))
                    .map_err(io_from_anyhow)?;
            }
        }
    }

    let gz_encoder = tar_builder
        .into_inner()
        .with_context(|| "tar into_inner failed")
        .map_err(io_from_anyhow)?;
    let compressed_bytes = gz_encoder
        .finish()
        .with_context(|| "gzip finish failed")
        .map_err(io_from_anyhow)?;

    let mut hasher = Sha256::new();
    hasher.update(&compressed_bytes);
    let digest_hex = HEXLOWER.encode(&hasher.finalize());
    Ok((compressed_bytes, format!("sha256:{digest_hex}")))
}

fn io_from_anyhow(e: anyhow::Error) -> PodmanSourceError {
    PodmanSourceError::IoError(std::io::Error::other(e.to_string()))
}

/// Assemble a docker-save-format tarball at `out_path` from the
/// podman-source layers. Mirrors the m031 assembler's output layout
/// so `docker_image::extract` consumes it identically.
///
/// Output tarball layout:
/// - `manifest.json` — array with one entry: `{Config, RepoTags, Layers}`
/// - `<sha256>.json` — the image config (referenced from manifest.json Config)
/// - `<sha256>/layer.tar.gz` — one per layer (referenced from manifest.json Layers)
fn assemble_tarball(
    config_bytes: &[u8],
    layers: &[PreparedLayer],
    image_ref: &str,
    out_path: &Path,
) -> Result<(), PodmanSourceError> {
    let out_file = std::fs::File::create(out_path)?;
    let mut tar_builder = tar::Builder::new(out_file);
    tar_builder.mode(tar::HeaderMode::Deterministic);

    // Config file.
    let config_hash = {
        let mut hasher = Sha256::new();
        hasher.update(config_bytes);
        HEXLOWER.encode(&hasher.finalize())
    };
    let config_filename = format!("{config_hash}.json");
    write_tar_bytes(&mut tar_builder, &config_filename, config_bytes)?;

    // Layer files + manifest layer-paths list.
    let mut layer_paths: Vec<String> = Vec::with_capacity(layers.len());
    for layer in layers {
        let layer_digest_hex = layer
            .compressed_digest
            .strip_prefix("sha256:")
            .unwrap_or(&layer.compressed_digest);
        let layer_filename = format!("{layer_digest_hex}/layer.tar.gz");
        write_tar_bytes(&mut tar_builder, &layer_filename, &layer.compressed_blob)?;
        layer_paths.push(layer_filename);
    }

    // manifest.json — docker-save format.
    let manifest_json = serde_json::json!([{
        "Config": config_filename,
        "RepoTags": [image_ref],
        "Layers": layer_paths,
    }]);
    let manifest_bytes = serde_json::to_vec(&manifest_json).map_err(|e| {
        PodmanSourceError::IoError(std::io::Error::other(format!(
            "serialize manifest.json: {e}"
        )))
    })?;
    write_tar_bytes(&mut tar_builder, "manifest.json", &manifest_bytes)?;

    tar_builder
        .finish()
        .with_context(|| "final tar finish failed")
        .map_err(io_from_anyhow)?;
    Ok(())
}

fn write_tar_bytes<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), PodmanSourceError> {
    let mut header = tar::Header::new_gnu();
    header.set_path(name).map_err(|e| {
        PodmanSourceError::IoError(std::io::Error::other(format!("tar set_path {name}: {e}")))
    })?;
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append(&header, bytes)
        .map_err(PodmanSourceError::IoError)?;
    Ok(())
}

// -----------------------------------------------------------------
// Unit tests
// -----------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // ── T007 US1 — PodmanImageRef parsing ──

    #[test]
    fn podman_image_ref_parse_tagged_default_latest() {
        let r = PodmanImageRef::parse("alpine").unwrap();
        assert_eq!(
            r,
            PodmanImageRef::Tagged {
                repo: "alpine".to_string(),
                tag: "latest".to_string(),
            }
        );
    }

    #[test]
    fn podman_image_ref_parse_tagged_explicit() {
        let r = PodmanImageRef::parse("alpine:3.19").unwrap();
        assert_eq!(
            r,
            PodmanImageRef::Tagged {
                repo: "alpine".to_string(),
                tag: "3.19".to_string(),
            }
        );
    }

    #[test]
    fn podman_image_ref_parse_registry_slash_repo_colon_tag() {
        let r = PodmanImageRef::parse("docker.io/library/nginx:1.27.0").unwrap();
        assert_eq!(
            r,
            PodmanImageRef::Tagged {
                repo: "docker.io/library/nginx".to_string(),
                tag: "1.27.0".to_string(),
            }
        );
    }

    #[test]
    fn podman_image_ref_parse_registry_port_no_tag() {
        // Colon is part of the registry port, no explicit tag → :latest.
        let r = PodmanImageRef::parse("registry:5000/foo").unwrap();
        assert_eq!(
            r,
            PodmanImageRef::Tagged {
                repo: "registry:5000/foo".to_string(),
                tag: "latest".to_string(),
            }
        );
    }

    #[test]
    fn podman_image_ref_parse_digest() {
        let raw = "alpine@sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let r = PodmanImageRef::parse(raw).unwrap();
        assert_eq!(
            r,
            PodmanImageRef::Digest {
                repo: "alpine".to_string(),
                digest:
                    "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                        .to_string(),
            }
        );
    }

    #[test]
    fn podman_image_ref_parse_image_id() {
        let r = PodmanImageRef::parse("abcdef123456").unwrap();
        assert_eq!(
            r,
            PodmanImageRef::ImageId {
                id_prefix: "abcdef123456".to_string(),
            }
        );
    }

    #[test]
    fn podman_image_ref_parse_empty_errors() {
        let r = PodmanImageRef::parse("");
        assert!(matches!(r, Err(PodmanSourceError::ImageNotFound { .. })));
    }

    // ── T007 US1 — Storage root discovery ──

    #[test]
    fn discover_storage_root_honors_graphroot_override() {
        let temp_home = tempfile::tempdir().unwrap();
        let conf_dir = temp_home.path().join(".config/containers");
        std::fs::create_dir_all(&conf_dir).unwrap();
        let graphroot = temp_home.path().join("custom-storage");
        std::fs::create_dir_all(&graphroot).unwrap();
        std::fs::write(
            conf_dir.join("storage.conf"),
            format!(
                "[storage]\ngraphroot = \"{}\"\n",
                graphroot.display()
            ),
        )
        .unwrap();

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_home.path());
        let result = discover_storage_root(true);
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        assert_eq!(result.unwrap(), graphroot);
    }

    #[test]
    fn discover_storage_root_falls_back_to_default_when_no_config() {
        let temp_home = tempfile::tempdir().unwrap();
        // No storage.conf; create the default rootless path.
        let default_gr = temp_home
            .path()
            .join(".local/share/containers/storage");
        std::fs::create_dir_all(&default_gr).unwrap();

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_home.path());
        let result = discover_storage_root(true);
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        assert_eq!(result.unwrap(), default_gr);
    }

    // ── T007 US1 — Storage driver detection ──

    #[test]
    fn detect_storage_driver_returns_overlay_when_overlay_dir_present() {
        let gr = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(gr.path().join("overlay")).unwrap();
        let d = detect_storage_driver(gr.path()).unwrap();
        assert_eq!(d, StorageDriver::Overlay);
    }

    #[test]
    fn detect_storage_driver_returns_unsupported_when_vfs() {
        let gr = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(gr.path().join("vfs")).unwrap();
        let err = detect_storage_driver(gr.path()).unwrap_err();
        assert!(matches!(
            err,
            PodmanSourceError::UnsupportedDriver { ref driver } if driver == "vfs"
        ));
    }

    #[test]
    fn detect_storage_driver_returns_unsupported_when_btrfs() {
        let gr = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(gr.path().join("btrfs")).unwrap();
        let err = detect_storage_driver(gr.path()).unwrap_err();
        assert!(matches!(
            err,
            PodmanSourceError::UnsupportedDriver { ref driver } if driver == "btrfs"
        ));
    }

    // ── T007 US1 — Image index parsing ──

    fn write_images_json(gr: &Path, records: &[serde_json::Value]) {
        let images_dir = gr.join("overlay-images");
        std::fs::create_dir_all(&images_dir).unwrap();
        std::fs::write(
            images_dir.join("images.json"),
            serde_json::to_vec(records).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn parse_images_index_matches_by_tag() {
        let gr = tempfile::tempdir().unwrap();
        write_images_json(
            gr.path(),
            &[
                serde_json::json!({
                    "id": "111111111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "digest": "sha256:d1",
                    "names": ["alpine:3.19"],
                    "layer": "layer-a",
                }),
                serde_json::json!({
                    "id": "222222222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "digest": "sha256:d2",
                    "names": ["nginx:latest"],
                    "layer": "layer-b",
                }),
            ],
        );
        let index = parse_images_index(gr.path()).unwrap();
        let parsed = PodmanImageRef::parse("alpine:3.19").unwrap();
        let matched = resolve_image_ref(&index, &parsed, gr.path()).unwrap();
        assert_eq!(matched.id, "111111111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn parse_images_index_matches_by_short_id() {
        let gr = tempfile::tempdir().unwrap();
        write_images_json(
            gr.path(),
            &[serde_json::json!({
                "id": "111111111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "digest": "sha256:d1",
                "names": ["alpine:3.19"],
                "layer": "layer-a",
            })],
        );
        let index = parse_images_index(gr.path()).unwrap();
        let parsed = PodmanImageRef::parse("111111111111").unwrap();
        let matched = resolve_image_ref(&index, &parsed, gr.path()).unwrap();
        assert!(matched.id.starts_with("111111111111"));
    }

    #[test]
    fn parse_images_index_no_match_errors() {
        let gr = tempfile::tempdir().unwrap();
        write_images_json(
            gr.path(),
            &[serde_json::json!({
                "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "digest": "sha256:xxx",
                "names": ["something:else"],
                "layer": "layer-x",
            })],
        );
        let index = parse_images_index(gr.path()).unwrap();
        let parsed = PodmanImageRef::parse("alpine:3.19").unwrap();
        let err = resolve_image_ref(&index, &parsed, gr.path()).unwrap_err();
        assert!(matches!(err, PodmanSourceError::ImageNotFound { .. }));
    }

    // ── T007 US1 — Layer chain resolution ──

    #[test]
    fn resolve_layer_chain_returns_base_to_top() {
        let mut layers = HashMap::new();
        layers.insert(
            "top".to_string(),
            LayerRecord {
                id: "top".into(),
                parent: "mid".into(),
                diff_digest: "sha256:t".into(),
                compressed_diff_digest: "sha256:tc".into(),
            },
        );
        layers.insert(
            "mid".to_string(),
            LayerRecord {
                id: "mid".into(),
                parent: "base".into(),
                diff_digest: "sha256:m".into(),
                compressed_diff_digest: "sha256:mc".into(),
            },
        );
        layers.insert(
            "base".to_string(),
            LayerRecord {
                id: "base".into(),
                parent: "".into(),
                diff_digest: "sha256:b".into(),
                compressed_diff_digest: "sha256:bc".into(),
            },
        );
        let chain = resolve_layer_chain(&layers, "top");
        assert_eq!(
            chain.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
            vec!["base", "mid", "top"]
        );
    }

    // ── T007 US1 — Error Display strings (needed for stderr grep assertions) ──

    #[test]
    fn podman_source_error_display_formats_all_variants_m206() {
        let e = PodmanSourceError::StorageRootUnreachable {
            path: PathBuf::from("/tmp/x"),
            reason: "perm".to_string(),
        };
        assert!(format!("{e}").contains("podman storage root not found"));

        let e = PodmanSourceError::UnsupportedDriver {
            driver: "vfs".to_string(),
        };
        assert!(format!("{e}").contains("vfs"));
        assert!(format!("{e}").contains("not supported"));

        let e = PodmanSourceError::ImageNotFound {
            image_ref: "alpine".to_string(),
            path: PathBuf::from("/gr"),
        };
        assert!(format!("{e}").contains("no podman image matched"));

        let e = PodmanSourceError::CorruptedManifest {
            id: "id1".to_string(),
            path: PathBuf::from("/m"),
            reason: "bad json".to_string(),
        };
        assert!(format!("{e}").contains("corrupted"));

        let e = PodmanSourceError::LayerDigestMismatch {
            id: "id1".to_string(),
            expected: "sha256:a".to_string(),
            actual: "sha256:b".to_string(),
        };
        assert!(format!("{e}").contains("layer digest verification failed"));

        let e = PodmanSourceError::NoArchMatch {
            image_ref: "img".to_string(),
            host: "linux/arm64".to_string(),
            available: vec!["linux/amd64".to_string()],
        };
        assert!(format!("{e}").contains("does not match any variant"));

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let e = PodmanSourceError::from(io_err);
        assert!(format!("{e}").contains("I/O error"));
    }

    // ── T009a US1 — arch alias mapping ──

    #[test]
    fn arch_alias_maps_rust_names_to_oci() {
        assert_eq!(arch_alias("x86_64"), "amd64");
        assert_eq!(arch_alias("aarch64"), "arm64");
        assert_eq!(arch_alias("arm"), "arm");
        assert_eq!(arch_alias("mips"), "mips"); // passthrough
    }

    // ── T021 US2 — F6 unit-scoped permission-denied test ──

    #[cfg(unix)]
    #[test]
    fn discover_storage_root_returns_unreachable_when_directory_unreadable_m206() {
        use std::os::unix::fs::PermissionsExt;

        // Skip if running as root — chmod 0000 doesn't stop root reads.
        if running_as_root() {
            eprintln!("skipping: root can read chmod-0000 dirs");
            return;
        }

        let temp_home = tempfile::tempdir().unwrap();
        let unreachable = temp_home.path().join(".local/share/containers/storage");
        std::fs::create_dir_all(&unreachable).unwrap();
        std::fs::set_permissions(&unreachable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_home.path());
        let result = discover_storage_root(true);
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        // Restore perms so tempdir cleanup works.
        std::fs::set_permissions(&unreachable, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(matches!(
            result,
            Err(PodmanSourceError::StorageRootUnreachable { .. })
        ));
    }
}
