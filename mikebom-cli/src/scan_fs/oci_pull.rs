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

use anyhow::{anyhow, Result};

/// Pull an OCI image reference and write a docker-save-format
/// tarball to a tempdir. Returns the TempDir handle so the
/// caller can keep it alive through the subsequent
/// `docker_image::extract` call. The tarball lives at
/// `<tempdir>/image.tar`.
///
/// `target_arch` is an OCI platform-arch name (e.g. `amd64`,
/// `arm64`). Use [`host_oci_arch`] to get the appropriate value
/// for the current host.
///
/// Anonymous pulls only in milestone 031. Auth handling lives in
/// the deferred 031.x follow-on.
#[allow(dead_code)] // wired in commit 2 (031/cli-dispatch-and-pull)
pub fn pull_to_tarball(
    _image_ref: &str,
    _target_arch: &str,
) -> Result<tempfile::TempDir> {
    // Filled in commit 2 (031/cli-dispatch-and-pull). This stub
    // exists in commit 1 so the workspace build matrix passes
    // for both the feature-on and feature-off cases — and so
    // dep-audit failures surface independently of any behavior
    // bugs in the implementation.
    Err(anyhow!(
        "oci_pull::pull_to_tarball is not yet implemented (filled in milestone 031 commit 2)"
    ))
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
}
