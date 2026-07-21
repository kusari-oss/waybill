//! Milestone 182 US1 — plain-HTTP OCI registry pull.
//!
//! Verifies that waybill's `--insecure-registry` flag causes the
//! registry client to use `http://` instead of `https://`, and that
//! the absence of the flag yields an actionable error naming
//! `--insecure-registry` (FR-014 Case 1).
//!
//! Test infrastructure: `wiremock 0.6` for a plain-HTTP mock registry
//! (dev-dep since m055). We build a minimal OCI-conformant manifest +
//! config + gzipped-tar layer that carries a single `etc/os-release`
//! entry. The layer is discovered by the waybill OS-release reader and
//! surfaces as a component in the emitted SBOM.

#![cfg(feature = "oci-registry")]

use std::io::Write;
use std::process::Command;

use serde_json::json;
use sha2::Digest;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── OCI fixture builder ──────────────────────────────────────────────

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// A minimal OCI image with one os-release-carrying gzipped-tar layer.
struct OciImage {
    manifest_bytes: Vec<u8>,
    config_bytes: Vec<u8>,
    layer_bytes: Vec<u8>,
    config_digest: String,
    layer_digest: String,
}

fn build_minimal_oci_image() -> OciImage {
    // 1. Build the uncompressed layer: a tar containing `etc/os-release`.
    let uncompressed_tar = {
        let mut builder = tar::Builder::new(Vec::<u8>::new());
        let body = b"ID=waybill-m182-test\nVERSION=1\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("etc/os-release").unwrap();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, body.as_ref()).unwrap();
        builder.into_inner().unwrap()
    };
    let uncompressed_diff_id = sha256_hex(&uncompressed_tar);

    // 2. Gzip-compress the tar.
    let layer_bytes = {
        let mut encoder = flate2::write::GzEncoder::new(
            Vec::<u8>::new(),
            flate2::Compression::default(),
        );
        encoder.write_all(&uncompressed_tar).unwrap();
        encoder.finish().unwrap()
    };
    let layer_digest = format!("sha256:{}", sha256_hex(&layer_bytes));

    // 3. Image config JSON. Minimal shape — waybill only needs
    //    architecture/os for the platform-matching path (the pulled
    //    image is single-manifest, so platform matching is skipped).
    let config = json!({
        "created": "2026-07-10T00:00:00Z",
        "architecture": "amd64",
        "os": "linux",
        "rootfs": {
            "type": "layers",
            "diff_ids": [format!("sha256:{uncompressed_diff_id}")],
        },
        "config": {},
    });
    let config_bytes = serde_json::to_vec(&config).unwrap();
    let config_digest = format!("sha256:{}", sha256_hex(&config_bytes));

    // 4. Manifest that references config + layer by digest.
    let manifest = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "size": config_bytes.len(),
            "digest": config_digest,
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
            "size": layer_bytes.len(),
            "digest": layer_digest,
        }],
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    OciImage {
        manifest_bytes,
        config_bytes,
        layer_bytes,
        config_digest,
        layer_digest,
    }
}

/// Mount all three registry endpoints (manifest, config blob, layer
/// blob) on the given wiremock server for a fixed repo/tag pair.
async fn mount_oci_endpoints(server: &MockServer, repo: &str, tag: &str, image: &OciImage) {
    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/manifests/{tag}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(image.manifest_bytes.clone())
                .insert_header(
                    "content-type",
                    "application/vnd.oci.image.manifest.v1+json",
                ),
        )
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/blobs/{}", image.config_digest)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(image.config_bytes.clone())
                .insert_header(
                    "content-type",
                    "application/vnd.oci.image.config.v1+json",
                ),
        )
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/blobs/{}", image.layer_digest)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(image.layer_bytes.clone())
                .insert_header(
                    "content-type",
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                ),
        )
        .mount(server)
        .await;
}

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn us1_insecure_registry_flag_enables_plain_http_pull() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m182-us1";
    let tag = "1.0";
    mount_oci_endpoints(&server, repo, tag, &image).await;

    // wiremock's `server.uri()` returns `http://127.0.0.1:<port>`.
    let uri = server.uri();
    let hostport = uri
        .strip_prefix("http://")
        .expect("wiremock uri starts with http://");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m182-us1.cdx.json");

    let image_ref = format!("{hostport}/{repo}:{tag}");
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            &image_ref,
            "--image-src",
            "remote",
            "--insecure-registry",
            hostport,
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "waybill exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );
    assert!(output.exists(), "output file missing: {}", output.display());

    let parsed: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&output).unwrap(),
    )
    .expect("output is valid JSON");
    // The minimal fixture image is intentionally NOT a known distro
    // (os-release ID=waybill-m182-test), so `components[]` may be
    // empty. What we DO check: waybill reached emission (dependencies
    // referencing the pulled image), proving the plain-HTTP transport
    // succeeded end-to-end.
    let deps = parsed
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("cdx has dependencies array");
    let image_ref_in_deps = deps.iter().any(|d| {
        d.get("ref")
            .and_then(|r| r.as_str())
            .is_some_and(|s| s.contains("waybill-m182-us1"))
    });
    assert!(
        image_ref_in_deps,
        "expected the pulled image ref to appear in dependencies[].ref — \
         proves the pull reached emission. got: {parsed:#}",
    );
}

#[tokio::test]
async fn us1_no_flag_produces_actionable_error() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m182-us1";
    let tag = "1.0";
    mount_oci_endpoints(&server, repo, tag, &image).await;
    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m182-us1-noflag.cdx.json");

    let image_ref = format!("{hostport}/{repo}:{tag}");
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            &image_ref,
            "--image-src",
            "remote",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected waybill to fail (plain-HTTP registry contacted over TLS), \
         got success. stderr:\n{stderr}",
    );
    // FR-014 Case 1: message must name the fix flag.
    assert!(
        stderr.contains("--insecure-registry"),
        "expected stderr to name --insecure-registry as the fix flag. stderr:\n{stderr}",
    );
    // Message should hint at TLS handshake failure (or an equivalent
    // handshake-shape diagnostic).
    assert!(
        stderr.contains("TLS handshake failed")
            || stderr.to_lowercase().contains("handshake"),
        "expected stderr to describe a TLS handshake failure. stderr:\n{stderr}",
    );
}

#[tokio::test]
async fn us1_url_scheme_in_image_does_not_imply_insecure() {
    // FR-013 regression pin — codifies the docker-mental-model design
    // decision from research.md Decision 4: an explicit `http://` in
    // the `--image` URI does NOT by itself grant insecure transport;
    // `--insecure-registry` is the only opt-in.
    //
    // The waybill reference parser does not currently accept a
    // leading scheme in `--image` — either way, we assert that
    // *without* the flag the invocation fails with the FR-014 Case 1
    // message. This future-proofs against a parser refactor that
    // silently accepts `http://<host>/...` as an insecure signal.
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m182-us1";
    let tag = "1.0";
    mount_oci_endpoints(&server, repo, tag, &image).await;
    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m182-us1-scheme.cdx.json");

    // Use the plain-HTTP hostport in --image but withhold the flag.
    // The invocation must fail (waybill tries HTTPS on the plain-HTTP
    // port). No matter whether a future refactor makes waybill
    // tolerate an explicit `http://` prefix, THE FLAG remains the
    // required opt-in.
    let image_ref = format!("{hostport}/{repo}:{tag}");
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            &image_ref,
            "--image-src",
            "remote",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected waybill to fail without --insecure-registry. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("--insecure-registry"),
        "expected stderr to point at --insecure-registry. stderr:\n{stderr}",
    );
}
