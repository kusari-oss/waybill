//! OCI registry image pull (milestone 031, restructured into a
//! submodule directory by milestone 032).
//!
//! This module is gated behind the default-off `oci-registry`
//! Cargo feature. When enabled, the `--image <ref>` CLI argument
//! accepts an OCI image reference (e.g. `alpine:3.19`,
//! `gcr.io/foo/bar@sha256:...`) in addition to the existing
//! docker-save tarball path. The reference is parsed, the manifest
//! plus layer blobs are pulled, gzipped layers are decompressed,
//! and a docker-save-format tarball is written to a tempdir before
//! being routed through the existing
//! `scan_fs::docker_image::extract` path.
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
//! Substrate (post-milestone-032):
//!   * `oci-spec = "0.9"` for OCI distribution-spec + image-spec
//!     types (manifest, descriptor, image config, manifest list).
//!     Pure-Rust, types-only.
//!   * Workspace `reqwest 0.12 + rustls-tls (ring)` for HTTPS
//!     transport. No new TLS / HTTP deps introduced.
//!   * `registry.rs` provides a thin custom HTTP client (manifest
//!     fetch + blob fetch + sha256 verification + bearer-token
//!     retry flow for Docker Hub).
//!   * `reference.rs` provides our own image-ref parser
//!     (registry / repository / tag / digest grammar).
//!
//! Milestone 031 (#63) originally shipped this feature on
//! `oci-client = "0.12"`, but that pin was version-locked to escape
//! aws-lc-sys (a C library) that newer oci-client versions
//! transitively pulled in via rustls 0.23+. Milestone 032 (#65)
//! swapped to the durable substrate above, removing the version-
//! pin trap. The `no_c_dependencies_in_oci_registry_feature_tree`
//! regression test in `mikebom-cli/tests/no_c_dependencies.rs`
//! locks the substrate decision in.

mod platform;
mod reference;
mod registry;
mod tarball;

use std::path::Path;

use anyhow::{bail, Context, Result};

use registry::{ManifestOrIndex, RegistryClient};
use tarball::PulledLayer;

/// Pull an OCI image reference and write a docker-save-format
/// tarball to a tempdir. Returns the TempDir handle so the
/// caller can keep it alive through the subsequent
/// `docker_image::extract` call. The tarball lives at
/// `<tempdir>/image.tar`.
///
/// Multi-arch image indexes resolve to `linux/<host-arch>`. mikebom
/// only scans Linux containers regardless of the host OS.
///
/// Anonymous pulls only in milestone 031. Auth handling lives in
/// the deferred 031.x follow-on (#66).
///
/// Async by design — mikebom's CLI is `#[tokio::main]`-bootstrapped,
/// so callers `.await` this directly without bridging.
pub async fn pull_to_tarball(image_ref: &str) -> Result<tempfile::TempDir> {
    let mut reference = reference::parse_reference(image_ref)
        .with_context(|| format!("parsing OCI image reference `{image_ref}`"))?;
    tracing::info!(
        registry = %reference.registry,
        repository = %reference.repository,
        tag = ?reference.tag,
        digest = ?reference.digest,
        "pulling OCI image"
    );

    let host_arch = host_oci_arch()
        .context("mapping host architecture to OCI platform name")?;
    let client = RegistryClient::new()?;

    // Step 1: fetch the manifest. If it's an image index
    // (manifest list), resolve the platform-specific manifest and
    // re-fetch with the digest. Single-platform manifests are
    // returned directly.
    let manifest = match client.fetch_manifest(&reference).await? {
        ManifestOrIndex::Manifest(m) => m,
        ManifestOrIndex::Index(idx) => {
            // oci-spec's Descriptor exposes platform / digest /
            // architecture / os via getset accessors. `Arch` and
            // `Os` are enums; convert via `Display` to OCI string
            // form (`amd64`, `linux`, etc.) before handing to
            // platform.rs.
            let mapped: Vec<platform::ManifestListEntry> = idx
                .manifests()
                .iter()
                .filter_map(|d| {
                    let plat = d.platform().as_ref()?;
                    Some(platform::ManifestListEntry {
                        digest: d.digest().to_string(),
                        architecture: plat.architecture().to_string(),
                        os: plat.os().to_string(),
                    })
                })
                .collect();
            let chosen_digest = platform::resolve_manifest_list_to_linux(mapped, host_arch)?;
            // Re-fetch with the platform-specific digest.
            reference.digest = Some(chosen_digest);
            reference.tag = None;
            match client.fetch_manifest(&reference).await? {
                ManifestOrIndex::Manifest(m) => m,
                ManifestOrIndex::Index(_) => {
                    bail!("expected a single-platform manifest after resolving image index, got nested index")
                }
            }
        }
    };

    // Step 2: fetch the config blob (sha256 verified by registry::fetch_blob).
    let config_digest = manifest.config().digest().to_string();
    let config_bytes = client
        .fetch_blob(&reference, &config_digest)
        .await
        .with_context(|| format!("fetching config blob {config_digest}"))?;

    // Step 3: fetch each layer blob. Preserve order — layer
    // index in the manifest is meaningful (layer 0 is base, layer N
    // is top of stack).
    let mut layers: Vec<PulledLayer> = Vec::with_capacity(manifest.layers().len());
    for (idx, layer_desc) in manifest.layers().iter().enumerate() {
        let digest = layer_desc.digest().to_string();
        tracing::debug!(layer = idx, %digest, "fetching layer blob");
        let bytes = client
            .fetch_blob(&reference, &digest)
            .await
            .with_context(|| format!("fetching layer {idx} blob {digest}"))?;
        layers.push(PulledLayer {
            media_type: layer_desc.media_type().to_string(),
            bytes,
        });
    }

    tarball::assert_layers_supported(&layers)?;

    // Step 4: assemble the docker-save-format tarball.
    let tempdir = tempfile::Builder::new()
        .prefix("mikebom-oci-pull-")
        .tempdir()
        .context("creating tempdir for OCI pull tarball")?;
    let tarball_path = tempdir.path().join("image.tar");
    tarball::assemble_docker_save_tarball(&config_bytes, &layers, image_ref, &tarball_path)
        .context("assembling docker-save-format tarball from pulled image")?;
    Ok(tempdir)
}

/// Distinguish a `--image` argument as either a path on disk
/// (existing tarball-extract path) or an OCI image reference
/// (the registry-pull path).
///
/// Detection rules (priority order):
///  1. If a file exists at the given path → treat as tarball.
///  2. Else if the string parses via the new
///     [`reference::parse_reference`] grammar → treat as ref.
///  3. Else → return `Invalid`.
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
    match reference::parse_reference(s) {
        Ok(_) => ImageArgKind::OciRef,
        Err(_) => ImageArgKind::Invalid,
    }
}

/// Map `std::env::consts::ARCH` to an OCI platform-arch name.
///
/// The OCI image-spec uses Go's GOARCH naming (`amd64`, `arm64`,
/// `arm`, `riscv64`, etc.) which differs from Rust's `ARCH`
/// constant (`x86_64`, `aarch64`, etc.).
///
/// Returns an error for unmapped host architectures so the
/// caller can surface a clear "host arch X not supported, please
/// use --image-platform <linux/...> when 031.y ships" message.
pub fn host_oci_arch() -> Result<&'static str> {
    Ok(match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        "riscv64" => "riscv64",
        "powerpc64" => "ppc64le", // typical OCI naming
        "s390x" => "s390x",
        other => {
            anyhow::bail!(
                "host architecture `{other}` not mapped to an OCI platform name; \
                 milestone 031 supports x86_64/aarch64/arm/riscv64/powerpc64/s390x. \
                 Cross-arch image pulls (`--image-platform linux/<arch>`) deferred to milestone 031.y."
            );
        }
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn host_oci_arch_returns_a_known_value_for_typical_hosts() {
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
        assert_eq!(detect_image_arg_kind(tmp.path()), ImageArgKind::Path);
    }

    #[test]
    fn detect_image_arg_kind_recognizes_typical_image_refs() {
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
}
