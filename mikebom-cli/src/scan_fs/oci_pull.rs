//! OCI registry image pull (milestone 031).
//!
//! This module is gated behind the default-off `oci-registry` Cargo
//! feature. When enabled, the `--image <ref>` CLI argument accepts
//! an OCI image reference (e.g. `alpine:3.19`,
//! `gcr.io/foo/bar@sha256:...`) in addition to the existing
//! docker-save tarball path. The reference is parsed via
//! `oci_client::Reference`, the manifest + layer blobs are pulled,
//! gzipped layers are decompressed, and a docker-save-format
//! tarball is written to a tempdir before being routed through the
//! existing `scan_fs::docker_image::extract` path.
//!
//! Sub-scope (milestone 031):
//!   * Anonymous public registries only.
//!   * Host-arch image selection only (no `--image-platform` flag).
//!   * Gzipped layers only (zstd → clear "not yet supported" error).
//!
//! Deferred:
//!   * 031.x — authenticated pulls (Docker keychain + cred helpers).
//!   * 031.y — `--image-platform linux/arch` flag.
//!   * 031.z — layer caching.
//!
//! Async-to-sync bridge: `oci-client` is async/tokio-native;
//! mikebom's CLI scan path is synchronous. We construct a
//! `tokio::runtime::Runtime` inside `pull_to_tarball` and use
//! `block_on(...)` to bridge — keeping the rest of the CLI path
//! unchanged. The runtime is dropped on function exit.

use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::path::Path;

use oci_client::client::{ClientConfig, ImageData};
use oci_client::manifest::{
    ImageIndexEntry, IMAGE_CONFIG_MEDIA_TYPE, IMAGE_DOCKER_CONFIG_MEDIA_TYPE,
    IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE, IMAGE_DOCKER_LAYER_TAR_MEDIA_TYPE,
    IMAGE_LAYER_GZIP_MEDIA_TYPE, IMAGE_LAYER_MEDIA_TYPE, IMAGE_MANIFEST_MEDIA_TYPE,
    OCI_IMAGE_MEDIA_TYPE,
};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, Reference};

/// Pull an OCI image reference and write a docker-save-format
/// tarball to a tempdir. Returns the TempDir handle so the
/// caller can keep it alive through the subsequent
/// `docker_image::extract` call. The tarball lives at
/// `<tempdir>/image.tar`.
///
/// `image_ref` is the original ref string (e.g. `alpine:3.19`);
/// it gets recorded as the manifest's `RepoTags[0]` so the SBOM's
/// subject name carries the human-readable reference.
///
/// Multi-arch image indexes resolve via oci-client's default
/// `current_platform_resolver`, which picks the variant matching
/// `std::env::consts::ARCH/OS`. Cross-arch selection deferred
/// to milestone 031.y (`--image-platform` flag).
///
/// Anonymous pulls only in milestone 031. Auth handling lives in
/// the deferred 031.x follow-on.
///
/// Async by design — mikebom's CLI is already
/// `#[tokio::main]`-bootstrapped, so callers can `.await` this
/// directly without bridging. (An earlier draft constructed its
/// own runtime and panicked with "Cannot start a runtime from
/// within a runtime" under the existing async-main; making this
/// async-native sidesteps that.)
pub async fn pull_to_tarball(image_ref: &str) -> Result<tempfile::TempDir> {
    let reference: Reference = image_ref
        .parse()
        .with_context(|| format!("parsing OCI image reference `{image_ref}`"))?;
    tracing::info!(
        registry = %reference.resolve_registry(),
        repository = %reference.repository(),
        tag = ?reference.tag(),
        digest = ?reference.digest(),
        "pulling OCI image"
    );

    // Custom platform resolver: always pick `linux/<host-arch>`
    // regardless of host OS. SBOM scanning of containers is
    // Linux-bound — even on macOS / Windows hosts we scan Linux
    // images. oci-client's default `current_platform_resolver`
    // would look for `darwin/arm64` on macOS, which never matches
    // a Linux-only image like distroless.
    let host_arch = host_oci_arch()
        .context("mapping host architecture to OCI platform name")?;
    let config = ClientConfig {
        platform_resolver: Some(Box::new(move |entries: &[ImageIndexEntry]| {
            entries
                .iter()
                .find(|e| {
                    e.platform.as_ref().is_some_and(|p| {
                        p.os == "linux" && p.architecture == host_arch
                    })
                })
                .map(|e| e.digest.clone())
        })),
        ..ClientConfig::default()
    };
    let client = Client::new(config);
    let auth = RegistryAuth::Anonymous;
    // Tell the registry which media types we accept. We accept
    // both Docker v2 and OCI manifest types, plus tar and gzipped
    // tar layer types. zstd-compressed layers (a separate OCI
    // media type) are NOT accepted — registries that prefer
    // those will return them anyway, in which case
    // `assert_layers_supported` below catches it with a clear
    // "not yet supported" error per FR-009.
    let accepted = vec![
        IMAGE_MANIFEST_MEDIA_TYPE,
        OCI_IMAGE_MEDIA_TYPE,
        IMAGE_LAYER_MEDIA_TYPE,
        IMAGE_LAYER_GZIP_MEDIA_TYPE,
        IMAGE_DOCKER_LAYER_TAR_MEDIA_TYPE,
        IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE,
        IMAGE_CONFIG_MEDIA_TYPE,
        IMAGE_DOCKER_CONFIG_MEDIA_TYPE,
    ];
    let image: ImageData = client
        .pull(&reference, &auth, accepted)
        .await
        .with_context(|| format!("pulling image `{image_ref}`"))?;

    assert_layers_supported(&image)?;

    let tempdir = tempfile::Builder::new()
        .prefix("mikebom-oci-pull-")
        .tempdir()
        .context("creating tempdir for OCI pull tarball")?;
    let tarball_path = tempdir.path().join("image.tar");
    assemble_docker_save_tarball(&image, image_ref, &tarball_path)
        .context("assembling docker-save-format tarball from pulled image")?;
    Ok(tempdir)
}

/// Reject layers we can't decompress. zstd is the main risk in
/// modern OCI images.
fn assert_layers_supported(image: &ImageData) -> Result<()> {
    for (idx, layer) in image.layers.iter().enumerate() {
        match layer.media_type.as_str() {
            // Plain tar — no decompression needed.
            IMAGE_LAYER_MEDIA_TYPE | IMAGE_DOCKER_LAYER_TAR_MEDIA_TYPE => {}
            // Gzipped tar — handled by `flate2::read::GzDecoder`.
            IMAGE_LAYER_GZIP_MEDIA_TYPE | IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE => {}
            other => {
                return Err(anyhow!(
                    "image layer {idx} has unsupported media type `{other}`. \
                     Milestone 031 supports plain tar and gzipped tar; zstd-compressed \
                     and other layer types are deferred to a future milestone."
                ));
            }
        }
    }
    Ok(())
}

/// Build a `docker save`-format tarball at `out_path` from the
/// pulled image data. The format (per moby's image-spec):
///
/// - `manifest.json`: top-level array with one entry containing
///   `Config`, `RepoTags`, `Layers`.
/// - `<config-digest>.json`: the image config JSON blob.
/// - `<layer-digest>/layer.tar`: per-layer plain-tar bytes.
///
/// We name layer files by their **uncompressed** tar's SHA-256.
/// That's what `docker_image::extract` expects: it reads each
/// `Layers[]` entry as a plain tar file.
fn assemble_docker_save_tarball(
    image: &ImageData,
    image_ref: &str,
    out_path: &Path,
) -> Result<()> {
    let out = std::fs::File::create(out_path)
        .with_context(|| format!("creating tarball at {}", out_path.display()))?;
    let mut builder = tar::Builder::new(std::io::BufWriter::new(out));

    // Decompress each layer (if needed) and stage the resulting
    // tar bytes for inclusion. Note the digest tracked here is the
    // digest of the UNCOMPRESSED tar — that's what docker save
    // names the file by, even when the original layer descriptor's
    // digest was the gzipped form.
    let mut layer_paths_for_manifest: Vec<String> = Vec::new();
    let mut staged_layers: Vec<(String, Vec<u8>)> = Vec::new();
    for layer in &image.layers {
        let decompressed = decompress_layer(layer)?;
        let digest = sha256_hex(&decompressed);
        let layer_path_in_tarball = format!("{digest}/layer.tar");
        layer_paths_for_manifest.push(layer_path_in_tarball.clone());
        staged_layers.push((layer_path_in_tarball, decompressed));
    }

    // Stage the config JSON.
    let config_digest = sha256_hex(&image.config.data);
    let config_filename = format!("{config_digest}.json");

    // manifest.json
    let manifest_json = serde_json::json!([
        {
            "Config": config_filename,
            "RepoTags": [image_ref],
            "Layers": layer_paths_for_manifest,
        }
    ]);
    let manifest_bytes = serde_json::to_vec(&manifest_json)
        .context("serializing manifest.json")?;
    append_tarball_entry(&mut builder, "manifest.json", &manifest_bytes)?;

    // <config-digest>.json — config blob.
    append_tarball_entry(&mut builder, &config_filename, &image.config.data)?;

    // Per-layer entries.
    for (layer_path, layer_bytes) in &staged_layers {
        append_tarball_entry(&mut builder, layer_path, layer_bytes)?;
    }

    let buf_writer = builder
        .into_inner()
        .context("finalizing tarball (tar::Builder::into_inner)")?;
    let mut file = buf_writer
        .into_inner()
        .map_err(|e| anyhow!("BufWriter flush failed: {e}"))?;
    file.flush().context("flushing tarball file")?;
    file.sync_all().context("sync_all on tarball file")?;
    Ok(())
}

fn append_tarball_entry<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .with_context(|| format!("appending {path} to tarball"))?;
    Ok(())
}

fn decompress_layer(layer: &oci_client::client::ImageLayer) -> Result<Vec<u8>> {
    match layer.media_type.as_str() {
        IMAGE_LAYER_MEDIA_TYPE | IMAGE_DOCKER_LAYER_TAR_MEDIA_TYPE => Ok(layer.data.clone()),
        IMAGE_LAYER_GZIP_MEDIA_TYPE | IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE => {
            let mut decoder = flate2::read::GzDecoder::new(layer.data.as_slice());
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .context("decompressing gzipped layer")?;
            Ok(out)
        }
        other => Err(anyhow!(
            "unexpected layer media type `{other}` (should have been caught by assert_layers_supported)"
        )),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Distinguish a `--image` argument as either a path on disk
/// (existing tarball-extract path) or an OCI image reference
/// (the new registry-pull path).
///
/// Detection rules (priority order):
///  1. If a file exists at the given path → treat as tarball.
///  2. Else if the string parses as `Reference` → treat as ref.
///  3. Else → return `Invalid` so the caller can surface a clear
///     "neither a file nor a parseable image ref" error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageArgKind {
    /// Path to a docker-save-format tarball on disk.
    Path,
    /// OCI image reference (e.g. `alpine:3.19`).
    OciRef,
    /// Neither — error.
    Invalid,
}

pub fn detect_image_arg_kind(arg: &Path) -> ImageArgKind {
    if arg.is_file() {
        return ImageArgKind::Path;
    }
    let s = match arg.to_str() {
        Some(s) => s,
        None => return ImageArgKind::Invalid,
    };
    match s.parse::<Reference>() {
        Ok(_) => ImageArgKind::OciRef,
        Err(_) => ImageArgKind::Invalid,
    }
}

/// Map `std::env::consts::ARCH` to an OCI platform-arch name. The
/// OCI image-spec uses Go's GOARCH naming (`amd64`, `arm64`,
/// `arm`, `riscv64`, etc.) which differs from Rust's `ARCH`
/// constant (`x86_64`, `aarch64`, etc.).
///
/// Returns an error for unmapped host architectures so the
/// caller can surface a clear "host arch X not supported, please
/// use --image-platform <linux/...> when 031.y ships" message.
#[allow(dead_code)] // wired in commit 2 (031/cli-dispatch-and-pull)
pub fn host_oci_arch() -> Result<&'static str> {
    Ok(match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        "riscv64" => "riscv64",
        "powerpc64" => "ppc64le", // typical OCI naming
        "s390x" => "s390x",
        other => {
            return Err(anyhow!(
                "host architecture `{other}` not mapped to an OCI platform name; \
                 milestone 031 supports x86_64/aarch64/arm/riscv64/powerpc64/s390x. \
                 Cross-arch image pulls (`--image-platform linux/<arch>`) deferred to milestone 031.y."
            ));
        }
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn host_oci_arch_returns_a_known_value_for_typical_hosts() {
        // CI runs on x86_64 (Linux) and aarch64 (macOS arm). Both
        // must map cleanly. Other hosts may be unmapped — we don't
        // assert a specific value to keep the test cross-platform.
        let arch = host_oci_arch();
        assert!(arch.is_ok(), "host_oci_arch failed: {arch:?}");
        let arch = arch.unwrap();
        assert!(
            ["amd64", "arm64", "arm", "riscv64", "ppc64le", "s390x"].contains(&arch),
            "unexpected OCI arch `{arch}` for std::env::consts::ARCH = {}",
            std::env::consts::ARCH,
        );
    }

    #[test]
    fn detect_image_arg_kind_recognizes_existing_file_as_path() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Newly-created tempfile exists on disk → Path.
        assert_eq!(detect_image_arg_kind(tmp.path()), ImageArgKind::Path);
    }

    #[test]
    fn detect_image_arg_kind_recognizes_typical_image_refs() {
        // Common shapes: `name:tag`, `lib/name:tag`, `host/name:tag`,
        // `host/path/name@sha256:...`. None of these exist as files.
        let cases = &[
            "alpine:3.19",
            "library/alpine:3.19",
            "docker.io/library/alpine:3.19",
            "gcr.io/distroless/static-debian12:latest",
            "ghcr.io/foo/bar@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ];
        for case in cases {
            let p = Path::new(case);
            assert_eq!(
                detect_image_arg_kind(p),
                ImageArgKind::OciRef,
                "expected OciRef for `{case}`",
            );
        }
    }

    #[test]
    fn detect_image_arg_kind_rejects_garbage() {
        let p = Path::new("");
        assert_eq!(detect_image_arg_kind(p), ImageArgKind::Invalid);
    }

    /// Build a synthetic ImageData from hand-crafted fake layers +
    /// a fake config blob, run `assemble_docker_save_tarball`, and
    /// assert the resulting tarball is parseable by
    /// `docker_image::extract` AND yields the expected file in the
    /// rootfs.
    #[test]
    fn assemble_docker_save_tarball_round_trips_via_extract() {
        use crate::scan_fs::docker_image;
        use oci_client::client::{Config, ImageLayer};

        // Build a synthetic 2-file tar bytes for a "layer". The
        // simplest layer: a tar archive containing one file named
        // `etc/os-release` with a known body.
        let layer_uncompressed = {
            let mut builder = tar::Builder::new(Vec::<u8>::new());
            let body = b"ID=mikebom-test\nVERSION=0\n";
            let mut header = tar::Header::new_gnu();
            header.set_path("etc/os-release").unwrap();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, body.as_ref()).unwrap();
            builder.into_inner().unwrap()
        };

        // Gzip-compress the layer to simulate a real registry layer
        // (gzipped tar is the most common shape).
        let layer_compressed = {
            use std::io::Write;
            let mut encoder = flate2::write::GzEncoder::new(
                Vec::<u8>::new(),
                flate2::Compression::default(),
            );
            encoder.write_all(&layer_uncompressed).unwrap();
            encoder.finish().unwrap()
        };

        // Synthetic config blob (just a small JSON object — extract
        // doesn't read its contents, only references the filename
        // via manifest.json's Config field).
        let config_data = b"{\"architecture\":\"amd64\",\"os\":\"linux\"}".to_vec();

        let image = ImageData {
            layers: vec![ImageLayer {
                data: layer_compressed,
                media_type: IMAGE_LAYER_GZIP_MEDIA_TYPE.to_string(),
                annotations: None,
            }],
            digest: Some("sha256:fake".to_string()),
            config: Config {
                data: config_data,
                media_type: IMAGE_CONFIG_MEDIA_TYPE.to_string(),
                annotations: None,
            },
            manifest: None,
        };

        // Emit the tarball, then extract it.
        let tempdir = tempfile::tempdir().unwrap();
        let tarball = tempdir.path().join("image.tar");
        assemble_docker_save_tarball(&image, "test/sample:latest", &tarball)
            .expect("tarball assembly should succeed");
        assert!(tarball.exists(), "tarball file was not created");

        let extracted = docker_image::extract(&tarball)
            .expect("extract should accept the assembled tarball");

        // Verify the layer's content reached the rootfs.
        let os_release = extracted.rootfs.join("etc/os-release");
        assert!(
            os_release.exists(),
            "etc/os-release missing from extracted rootfs"
        );
        let body = std::fs::read_to_string(&os_release).unwrap();
        assert!(
            body.contains("ID=mikebom-test"),
            "os-release body unexpected: {body:?}"
        );

        // RepoTags round-trip — what we passed in should come back.
        assert_eq!(extracted.repo_tag.as_deref(), Some("test/sample:latest"));
    }

    #[test]
    fn assert_layers_supported_rejects_zstd() {
        use oci_client::client::{Config, ImageLayer};

        let image = ImageData {
            layers: vec![ImageLayer {
                data: vec![0u8; 16],
                media_type: "application/vnd.oci.image.layer.v1.tar+zstd".to_string(),
                annotations: None,
            }],
            digest: None,
            config: Config {
                data: Vec::new(),
                media_type: IMAGE_CONFIG_MEDIA_TYPE.to_string(),
                annotations: None,
            },
            manifest: None,
        };
        let err = assert_layers_supported(&image).unwrap_err();
        assert!(
            err.to_string().contains("zstd")
                || err.to_string().contains("unsupported media type"),
            "expected zstd-related error; got: {err}"
        );
    }
}
