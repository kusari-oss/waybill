//! Docker-save-format tarball assembly + layer decompression
//! (milestone 031, refactored into submodule by milestone 032).
//!
//! The OCI registry pull pipeline produces:
//!   - per-layer compressed bytes from the registry (typically gzipped tar)
//!   - the image config JSON blob
//!
//! This module assembles those into a tarball that
//! `scan_fs::docker_image::extract` can consume verbatim:
//!
//! - `manifest.json`: top-level array with one entry referencing
//!   `Config`, `RepoTags`, `Layers`.
//! - `<config-digest>.json`: the image config JSON blob.
//! - `<layer-digest>/layer.tar`: per-layer plain-tar bytes
//!   (decompressed). `<layer-digest>` here is the SHA-256 of the
//!   UNCOMPRESSED tar bytes — that's what docker save uses.
//!
//! Behavior preserved from milestone 031.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};

// Layer media-type constants (no longer pulled from oci-client; we
// own these values directly since they're stable wire-format strings
// per the OCI distribution-spec).
const OCI_LAYER_MEDIA_TYPE: &str = "application/vnd.oci.image.layer.v1.tar";
const OCI_LAYER_GZIP_MEDIA_TYPE: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
const DOCKER_LAYER_TAR_MEDIA_TYPE: &str = "application/vnd.docker.image.rootfs.diff.tar";
const DOCKER_LAYER_GZIP_MEDIA_TYPE: &str = "application/vnd.docker.image.rootfs.diff.tar.gzip";

/// One layer's bytes-as-fetched-from-the-registry plus its
/// declared media type. Used to feed
/// [`assemble_docker_save_tarball`] without coupling to any
/// specific OCI client crate's types.
pub(super) struct PulledLayer {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// Reject layers we can't decompress. zstd is the main risk in
/// modern OCI images.
pub(super) fn assert_layers_supported(layers: &[PulledLayer]) -> Result<()> {
    for (idx, layer) in layers.iter().enumerate() {
        match layer.media_type.as_str() {
            // Plain tar — no decompression needed.
            OCI_LAYER_MEDIA_TYPE | DOCKER_LAYER_TAR_MEDIA_TYPE => {}
            // Gzipped tar — handled by `flate2::read::GzDecoder`.
            OCI_LAYER_GZIP_MEDIA_TYPE | DOCKER_LAYER_GZIP_MEDIA_TYPE => {}
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
/// pulled image data. Layer files are named by the SHA-256 of the
/// UNCOMPRESSED tar bytes (matching `docker save`'s convention).
pub(super) fn assemble_docker_save_tarball(
    config_bytes: &[u8],
    layers: &[PulledLayer],
    image_ref: &str,
    out_path: &Path,
) -> Result<()> {
    let out = std::fs::File::create(out_path)
        .with_context(|| format!("creating tarball at {}", out_path.display()))?;
    let mut builder = tar::Builder::new(std::io::BufWriter::new(out));

    let mut layer_paths_for_manifest: Vec<String> = Vec::new();
    let mut staged_layers: Vec<(String, Vec<u8>)> = Vec::new();
    for layer in layers {
        let decompressed = decompress_layer(layer)?;
        let digest = sha256_hex(&decompressed);
        let layer_path_in_tarball = format!("{digest}/layer.tar");
        layer_paths_for_manifest.push(layer_path_in_tarball.clone());
        staged_layers.push((layer_path_in_tarball, decompressed));
    }

    let config_digest = sha256_hex(config_bytes);
    let config_filename = format!("{config_digest}.json");

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

    append_tarball_entry(&mut builder, &config_filename, config_bytes)?;

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

fn decompress_layer(layer: &PulledLayer) -> Result<Vec<u8>> {
    match layer.media_type.as_str() {
        OCI_LAYER_MEDIA_TYPE | DOCKER_LAYER_TAR_MEDIA_TYPE => Ok(layer.bytes.clone()),
        OCI_LAYER_GZIP_MEDIA_TYPE | DOCKER_LAYER_GZIP_MEDIA_TYPE => {
            let mut decoder = flate2::read::GzDecoder::new(layer.bytes.as_slice());
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Hand-build fake layers + config, run the assembly, verify
    /// `docker_image::extract` round-trips and the rootfs has the
    /// expected file. Preserved from milestone-031 oci_pull.rs;
    /// updated to use the new primitive `PulledLayer` shape.
    #[test]
    fn assemble_docker_save_tarball_round_trips_via_extract() {
        use crate::scan_fs::docker_image;

        let layer_uncompressed = {
            let mut builder = tar::Builder::new(Vec::<u8>::new());
            let body = b"ID=waybill-test\nVERSION=0\n";
            let mut header = tar::Header::new_gnu();
            header.set_path("etc/os-release").unwrap();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, body.as_ref()).unwrap();
            builder.into_inner().unwrap()
        };

        let layer_compressed = {
            let mut encoder = flate2::write::GzEncoder::new(
                Vec::<u8>::new(),
                flate2::Compression::default(),
            );
            encoder.write_all(&layer_uncompressed).unwrap();
            encoder.finish().unwrap()
        };

        let config_bytes = b"{\"architecture\":\"amd64\",\"os\":\"linux\"}".to_vec();

        let layers = vec![PulledLayer {
            bytes: layer_compressed,
            media_type: OCI_LAYER_GZIP_MEDIA_TYPE.to_string(),
        }];

        let tempdir = tempfile::tempdir().unwrap();
        let tarball = tempdir.path().join("image.tar");
        assemble_docker_save_tarball(&config_bytes, &layers, "test/sample:latest", &tarball)
            .expect("tarball assembly should succeed");
        assert!(tarball.exists(), "tarball file was not created");

        let extracted = docker_image::extract(&tarball)
            .expect("extract should accept the assembled tarball");

        let os_release = extracted.rootfs.join("etc/os-release");
        assert!(
            os_release.exists(),
            "etc/os-release missing from extracted rootfs"
        );
        let body = std::fs::read_to_string(&os_release).unwrap();
        assert!(
            body.contains("ID=waybill-test"),
            "os-release body unexpected: {body:?}"
        );

        assert_eq!(extracted.repo_tag.as_deref(), Some("test/sample:latest"));
    }

    #[test]
    fn assert_layers_supported_rejects_zstd() {
        let layers = vec![PulledLayer {
            bytes: vec![0u8; 16],
            media_type: "application/vnd.oci.image.layer.v1.tar+zstd".to_string(),
        }];
        let err = assert_layers_supported(&layers).unwrap_err();
        assert!(
            err.to_string().contains("zstd")
                || err.to_string().contains("unsupported media type"),
            "expected zstd-related error; got: {err}"
        );
    }
}
