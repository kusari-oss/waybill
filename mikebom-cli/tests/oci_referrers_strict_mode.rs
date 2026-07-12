//! Milestone 186 US2 — `--sbom-source referrer` strict-mode integration tests.
//!
//! Fail-closed guarantee: mikebom exits non-zero with an actionable error
//! when no matching referrer exists (FR-009 / SC-003). Verified via a
//! `wiremock` plain-HTTP registry (dev-dep since m055).

#![cfg(feature = "oci-registry")]

use std::io::Write;
use std::process::Command;

use serde_json::json;
use sha2::Digest;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── OCI fixture builder (same shape as US1 tests) ────────────────────

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

struct OciImage {
    manifest_bytes: Vec<u8>,
    manifest_digest: String,
    config_bytes: Vec<u8>,
    layer_bytes: Vec<u8>,
    config_digest: String,
    layer_digest: String,
}

fn build_minimal_oci_image() -> OciImage {
    let uncompressed_tar = {
        let mut builder = tar::Builder::new(Vec::<u8>::new());
        let body = b"ID=mikebom-m186-us2-test\nVERSION=1\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("etc/os-release").unwrap();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, body.as_ref()).unwrap();
        builder.into_inner().unwrap()
    };
    let uncompressed_diff_id = sha256_hex(&uncompressed_tar);

    let layer_bytes = {
        let mut encoder = flate2::write::GzEncoder::new(
            Vec::<u8>::new(),
            flate2::Compression::default(),
        );
        encoder.write_all(&uncompressed_tar).unwrap();
        encoder.finish().unwrap()
    };
    let layer_digest = format!("sha256:{}", sha256_hex(&layer_bytes));

    let config = json!({
        "created": "2026-07-11T00:00:00Z",
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
    let manifest_digest = format!("sha256:{}", sha256_hex(&manifest_bytes));
    OciImage {
        manifest_bytes,
        manifest_digest,
        config_bytes,
        layer_bytes,
        config_digest,
        layer_digest,
    }
}

async fn mount_image_endpoints(server: &MockServer, repo: &str, tag: &str, image: &OciImage) {
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

fn build_cdx_referrer(marker: &str) -> Vec<u8> {
    let body = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": "urn:uuid:00000000-0000-0000-0000-000000000186",
        "version": 1,
        "metadata": {
            "component": {
                "type": "container",
                "name": marker,
                "version": "referrer-fixture-v1",
            },
        },
        "components": [],
    });
    serde_json::to_vec(&body).unwrap()
}

async fn mount_referrer(
    server: &MockServer,
    repo: &str,
    manifest_digest: &str,
    referrer_bytes: &[u8],
) -> String {
    let descriptor_digest = format!("sha256:{}", sha256_hex(referrer_bytes));
    let index = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [
            {
                "mediaType": "application/vnd.cyclonedx+json",
                "digest": descriptor_digest,
                "size": referrer_bytes.len(),
            }
        ],
    });
    let index_bytes = serde_json::to_vec(&index).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/referrers/{manifest_digest}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(index_bytes)
                .insert_header(
                    "content-type",
                    "application/vnd.oci.image.index.v1+json",
                ),
        )
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/blobs/{descriptor_digest}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(referrer_bytes.to_vec())
                .insert_header("content-type", "application/vnd.cyclonedx+json"),
        )
        .mount(server)
        .await;

    descriptor_digest
}

async fn mount_oversize_referrer(server: &MockServer, repo: &str, manifest_digest: &str) {
    let dummy_body = b"{}";
    let descriptor_digest = format!("sha256:{}", sha256_hex(dummy_body));
    let index = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [
            {
                "mediaType": "application/vnd.cyclonedx+json",
                "digest": descriptor_digest,
                "size": 200u64 * 1024 * 1024,
            }
        ],
    });
    let index_bytes = serde_json::to_vec(&index).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/referrers/{manifest_digest}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(index_bytes)
                .insert_header(
                    "content-type",
                    "application/vnd.oci.image.index.v1+json",
                ),
        )
        .mount(server)
        .await;
}

async fn mount_empty_referrers(server: &MockServer, repo: &str, manifest_digest: &str) {
    let index = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [],
    });
    let index_bytes = serde_json::to_vec(&index).unwrap();
    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/referrers/{manifest_digest}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(index_bytes)
                .insert_header(
                    "content-type",
                    "application/vnd.oci.image.index.v1+json",
                ),
        )
        .mount(server)
        .await;
}

async fn mount_referrers_404(server: &MockServer, repo: &str, manifest_digest: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/referrers/{manifest_digest}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(server)
        .await;
}

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn referrer_mode_emits_matching_referrer() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us2";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;

    let referrer_bytes = build_cdx_referrer("mikebom-m186-us2-referrer");
    let descriptor_digest =
        mount_referrer(&server, repo, &image.manifest_digest, &referrer_bytes).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us2.cdx.json");

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
            "--sbom-source",
            "referrer",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "mikebom exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );
    let got = std::fs::read(&output).unwrap();
    assert_eq!(
        got, referrer_bytes,
        "referrer bytes NOT emitted verbatim (FR-006)"
    );
    // F3 remediation — SC-005 provenance log strings.
    assert!(
        stderr.contains("emitted SBOM from OCI Referrers API")
            && stderr.contains("sbom_source")
            && stderr.contains("referrer")
            && stderr.contains(&descriptor_digest),
        "expected SC-005 provenance log with descriptor_digest={descriptor_digest}. stderr:\n{stderr}",
    );
}

#[tokio::test]
async fn referrer_mode_errors_on_no_match() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us2-empty";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    mount_empty_referrers(&server, repo, &image.manifest_digest).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us2-empty.cdx.json");

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
            "--sbom-source",
            "referrer",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected mikebom to fail under --sbom-source referrer with no match. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("no matching SBOM referrer found"),
        "expected actionable stderr per contracts/cli-flag.md §Error message templates. stderr:\n{stderr}",
    );
    // Output file must not exist on error path.
    assert!(!output.exists(), "output file should not be written on --sbom-source referrer failure");
}

#[tokio::test]
async fn referrer_mode_errors_on_404_registry() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us2-404";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    mount_referrers_404(&server, repo, &image.manifest_digest).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us2-404.cdx.json");

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
            "--sbom-source",
            "referrer",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected mikebom to fail under --sbom-source referrer with HTTP 404. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("--sbom-source scan")
            || stderr.contains("--sbom-source either")
            || stderr.contains("does not support"),
        "expected stderr to point at the fix flags per contracts/cli-flag.md. stderr:\n{stderr}",
    );
}

#[tokio::test]
async fn referrer_mode_errors_on_size_cap() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us2-oversize";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    mount_oversize_referrer(&server, repo, &image.manifest_digest).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us2-oversize.cdx.json");

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
            "--sbom-source",
            "referrer",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected mikebom to fail under --sbom-source referrer with all candidates over size cap. stderr:\n{stderr}",
    );
    // Size-cap WARN is emitted during the pick step; the "no matching" error
    // message is what surfaces to the operator since the picker returned None.
    assert!(
        stderr.contains("no matching SBOM referrer found")
            || stderr.contains("MIKEBOM_REFERRER_MAX_BYTES"),
        "expected size-cap-related error message. stderr:\n{stderr}",
    );
}
