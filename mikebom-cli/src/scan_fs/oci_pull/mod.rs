//! OCI registry image pull (milestone 031, restructured into a
//! submodule directory by milestone 032).
//!
//! This module is gated behind the `oci-registry` Cargo feature
//! (on by default as of milestone 033; users who want a
//! minimal-deps build can opt out via `--no-default-features`).
//! When enabled, the `--image <ref>` CLI argument accepts an OCI
//! image reference (e.g. `alpine:3.19`,
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

mod auth;
// `cache` is wired into `registry` in milestone 036 commit 2; this
// commit only adds the module + inline tests so it can land
// independently.
#[allow(dead_code)]
mod cache;
mod platform;
mod reference;
mod registry;
// Milestone 186 (#442) — OCI Distribution Spec v1.1 Referrers API SBOM
// discovery. Media-type filter + priority-ordering picker for
// `try_fetch_referrer_sbom`.
mod referrers;
mod tarball;
// Milestone 182 — TLS/transport configuration surfaced by the three
// `--insecure-registry`, `--registry-ca-cert`, `--insecure-tls-skip-verify`
// flags. Threaded through `pull_to_tarball` into `RegistryClient::new`.
mod tls_config;

pub(crate) use tls_config::RegistryTlsConfig;

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
/// Multi-arch image indexes resolve to `linux/<host-arch>` by
/// default. Pass `Some("linux/<arch>[/<variant>]")` via
/// `image_platform` to override (milestone 035 / #67). mikebom only
/// scans Linux containers regardless of the host OS, so non-linux
/// platform requests are rejected upfront.
///
/// Authenticated pulls (milestone 034 / #66): credentials are
/// resolved from the Docker keychain inside `RegistryClient::new`.
///
/// Layer caching (milestone 036 / #68): when
/// `cache_size_cap` is `Some(bytes)`, blobs are cached on disk
/// keyed on their SHA-256 digest. `None` disables caching for
/// this pull (every blob is fetched from the network).
///
/// Async by design — mikebom's CLI is `#[tokio::main]`-bootstrapped,
/// so callers `.await` this directly without bridging.
pub async fn pull_to_tarball(
    image_ref: &str,
    image_platform: Option<&str>,
    cache_size_cap: Option<u64>,
    creds_dir: Option<&Path>,
    // Milestone 182 — TLS/transport configuration (insecure-registry
    // matcher, additional CA bundle, skip-verify boolean). Passed
    // unmodified into `RegistryClient::new`. Zero-cost when default.
    tls_config: &RegistryTlsConfig,
) -> Result<tempfile::TempDir> {
    let mut reference = reference::parse_reference(image_ref)
        .with_context(|| format!("parsing OCI image reference `{image_ref}`"))?;
    // Resolve the target platform: explicit `--image-platform`
    // overrides the host default. `host_oci_arch` errors on
    // unmapped host arches; the explicit-platform path bypasses
    // that mapping entirely (a user on a host arch we don't
    // recognize can still scan a recognized cross-arch image).
    let (target_arch, target_variant): (String, Option<String>) = match image_platform {
        Some(s) => {
            let parsed = platform::parse_platform_string(s)
                .with_context(|| format!("parsing --image-platform `{s}`"))?;
            (parsed.architecture, parsed.variant)
        }
        None => (
            host_oci_arch()
                .context("mapping host architecture to OCI platform name")?
                .to_string(),
            None,
        ),
    };
    tracing::info!(
        registry = %reference.registry,
        repository = %reference.repository,
        tag = ?reference.tag,
        digest = ?reference.digest,
        target_arch = %target_arch,
        target_variant = ?target_variant,
        "pulling OCI image"
    );

    // Open the layer cache when a cap is configured. Cache::open
    // is best-effort: any IO failure (read-only fs, missing $HOME,
    // etc.) returns None and we fall through to no-cache mode. The
    // user's scan completes either way.
    let cache_handle = cache_size_cap.and_then(cache::Cache::open);
    let client = RegistryClient::new(&reference, cache_handle, creds_dir, tls_config)?;

    // Step 1: fetch the manifest. If it's an image index
    // (manifest list), resolve the platform-specific manifest and
    // re-fetch with the digest. Single-platform manifests are
    // returned directly.
    let manifest = match client.fetch_manifest(&reference).await? {
        ManifestOrIndex::Manifest(m) => m,
        ManifestOrIndex::Index(idx) => {
            // oci-spec's Descriptor exposes platform / digest /
            // architecture / os / variant via getset accessors.
            // `Arch` and `Os` are enums; convert via `Display` to
            // OCI string form (`amd64`, `linux`, etc.) before
            // handing to platform.rs. Variant is already an
            // `Option<String>`.
            let mapped: Vec<platform::ManifestListEntry> = idx
                .manifests()
                .iter()
                .filter_map(|d| {
                    let plat = d.platform().as_ref()?;
                    Some(platform::ManifestListEntry {
                        digest: d.digest().to_string(),
                        architecture: plat.architecture().to_string(),
                        os: plat.os().to_string(),
                        variant: plat.variant().clone(),
                    })
                })
                .collect();
            let chosen_digest = platform::resolve_manifest_list_to_linux(
                mapped,
                &target_arch,
                target_variant.as_deref(),
            )?;
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

// -----------------------------------------------------------------
// Milestone 186 (#442) — OCI Referrers API SBOM discovery.
// -----------------------------------------------------------------

/// Milestone 186 — the fetched SBOM referrer + provenance markers, ready for
/// the caller to (a) write `bytes` verbatim to `--output` and (b) emit the
/// FR-007 INFO audit-log line naming the source descriptor.
///
/// `bytes` is the byte-identical blob body — no re-parse, no re-encode.
/// `descriptor_digest` is the SHA-256 digest mikebom already verified against
/// the descriptor's declared digest. `media_type` is the descriptor's
/// declared media type (used in the audit log + FR-004 format-mismatch WARN).
pub struct ReferrerSbom {
    pub bytes: Vec<u8>,
    pub descriptor_digest: String,
    pub media_type: String,
}

/// Default per-referrer content size cap (100 MiB) — matches spec.md FR-014.
/// Override via the `MIKEBOM_REFERRER_MAX_BYTES` env var (research Decision 4).
pub const DEFAULT_REFERRER_MAX_BYTES: u64 = 100 * 1024 * 1024;

/// Read `MIKEBOM_REFERRER_MAX_BYTES` (default 100 MiB) — the descriptor-level
/// cap consulted by [`referrers::pick_sbom_descriptor`] BEFORE any blob fetch,
/// preventing a malicious/misconfigured registry from DoSing mikebom via an
/// oversize declared size (research Decision 4).
pub fn resolve_referrer_max_bytes() -> u64 {
    std::env::var("MIKEBOM_REFERRER_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_REFERRER_MAX_BYTES)
}

/// Milestone 186 (#442) — attempt to fetch an SBOM from the OCI Distribution
/// Spec v1.1 Referrers API for the given image reference.
///
/// Pipeline (see `specs/186-oci-referrers-sbom/contracts/referrers-pipeline.md`):
///   1. Parse the image reference + resolve platform (reuses the same code
///      path as [`pull_to_tarball`]).
///   2. Fetch the manifest → capture the resolved single-platform manifest
///      digest (used as the `<digest>` in `/v2/<repo>/referrers/<digest>`).
///   3. Query the Referrers endpoint → parse into `ImageIndex`.
///   4. Filter + pick the best SBOM descriptor via
///      [`referrers::pick_sbom_descriptor`].
///   5. Fetch + SHA-256-verify the descriptor's blob body.
///
/// Returns:
///   * `Ok(Some(sbom))` — a matching referrer was fetched + verified.
///     Caller writes `sbom.bytes` verbatim to `--output`.
///   * `Ok(None)` — the endpoint returned HTTP 404 (no v1.1 support), OR the
///     ImageIndex contained zero SBOM-shaped descriptors, OR every candidate
///     exceeded `max_bytes`. Caller falls through to scan under `either`
///     mode; caller errors out under `referrer` mode.
///   * `Err(_)` — HTTP or SHA-256-verify failure. Caller decides how to
///     surface based on `SbomSourceMode`.
///
/// `requested_formats` is passed through to `pick_sbom_descriptor` for the
/// format-match preference (research Decision 2 tier 1). Callers pass their
/// `--format` values in operator-specified order.
#[allow(dead_code)] // Wired into scan_cmd.rs dispatch in T018+T024+T029.
pub async fn try_fetch_referrer_sbom(
    image_ref: &str,
    image_platform: Option<&str>,
    creds_dir: Option<&Path>,
    tls_config: &RegistryTlsConfig,
    requested_formats: &[&str],
    max_bytes: u64,
) -> Result<Option<ReferrerSbom>> {
    // Step 1: parse reference + resolve platform.
    let mut reference = reference::parse_reference(image_ref)
        .with_context(|| format!("parsing OCI image reference `{image_ref}`"))?;
    let (target_arch, target_variant): (String, Option<String>) = match image_platform {
        Some(s) => {
            let parsed = platform::parse_platform_string(s)
                .with_context(|| format!("parsing --image-platform `{s}`"))?;
            (parsed.architecture, parsed.variant)
        }
        None => (
            host_oci_arch()
                .context("mapping host architecture to OCI platform name")?
                .to_string(),
            None,
        ),
    };

    let client = RegistryClient::new(&reference, None, creds_dir, tls_config)?;

    // Step 2: fetch manifest → resolve single-platform digest.
    match client.fetch_manifest(&reference).await? {
        ManifestOrIndex::Manifest(_) => {
            // Reference was already a single-platform manifest; its digest is
            // whatever the operator supplied (or a tag we need to re-resolve).
            // Fetch by tag returns no explicit digest via this path; we
            // re-fetch with the digest header captured for the referrer query.
        }
        ManifestOrIndex::Index(idx) => {
            let mapped: Vec<platform::ManifestListEntry> = idx
                .manifests()
                .iter()
                .filter_map(|d| {
                    let plat = d.platform().as_ref()?;
                    Some(platform::ManifestListEntry {
                        digest: d.digest().to_string(),
                        architecture: plat.architecture().to_string(),
                        os: plat.os().to_string(),
                        variant: plat.variant().clone(),
                    })
                })
                .collect();
            let chosen_digest = platform::resolve_manifest_list_to_linux(
                mapped,
                &target_arch,
                target_variant.as_deref(),
            )?;
            reference.digest = Some(chosen_digest);
            reference.tag = None;
        }
    };

    // For a tag-only reference resolved to a single-platform manifest, we
    // need the manifest's own digest for the Referrers query. If the caller
    // supplied a `@sha256:...` digest, use it as-is; otherwise fetch the
    // manifest by tag and hash the response body to derive the digest.
    let manifest_digest = match reference.digest.clone() {
        Some(d) => d,
        None => {
            let body = client.fetch_manifest_body(&reference).await?;
            format!("sha256:{}", sha2_hex(&body))
        }
    };

    // Step 3: query the Referrers endpoint.
    let index = match client.fetch_referrers(&reference, &manifest_digest).await? {
        Some(idx) => idx,
        None => return Ok(None),
    };

    // Step 4: filter + pick the best SBOM descriptor.
    let descriptor = match referrers::pick_sbom_descriptor(&index, requested_formats, max_bytes) {
        Some(d) => d,
        None => return Ok(None),
    };
    let descriptor_digest = descriptor.digest().to_string();
    let media_type = descriptor.media_type().as_ref().to_string();

    // Step 5: fetch + SHA-256-verify the referrer blob.
    let bytes = client
        .fetch_blob(&reference, &descriptor_digest)
        .await
        .with_context(|| {
            format!("fetching SBOM referrer blob {descriptor_digest} for {image_ref}")
        })?;

    Ok(Some(ReferrerSbom {
        bytes,
        descriptor_digest,
        media_type,
    }))
}

/// SHA-256 hex digest of `bytes` for the manifest-digest derivation path.
/// Kept local to the m186 code path so we don't perturb `verify_sha256`'s
/// return-shape.
fn sha2_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
                "host architecture `{other}` not mapped to an OCI platform name. \
                 mikebom recognizes x86_64/aarch64/arm/riscv64/powerpc64/s390x \
                 for the host-default selection. To scan a different architecture, \
                 pass `--image-platform <linux/arch>` explicitly \
                 (e.g. `--image-platform linux/amd64`)."
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
