//! Milestone 186 US1 — `--sbom-source either` mode integration tests.
//!
//! Prefer a referrer if one is published on the registry; fall through to
//! scan silently on 404, empty index, or size-cap exceeded. Verified via a
//! `wiremock` plain-HTTP registry (dev-dep since m055; reused verbatim from
//! the m182 `oci_pull_plain_http.rs` scaffold — the only additions here are
//! the Referrers endpoint + referrer descriptor blob).
//!
//! FR-007 / SC-005: `us1_either_prefers_referrer_when_available` also asserts
//! stderr contains the audit-log provenance strings so operators consuming
//! waybill logs can identify referrer-sourced emissions from log content
//! alone (F3 remediation from /speckit-analyze).

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
/// Same shape as m182's `build_minimal_oci_image` so both test suites
/// exercise the identical fixture semantics.
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
        let body = b"ID=waybill-m186-test\nVERSION=1\n";
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
    // Manifest endpoint (accessed by both `sbom-source scan` and the m186
    // `try_fetch_referrer_sbom` manifest-digest derivation).
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

    // Config blob (image scan pipeline only).
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

    // Layer blob (image scan pipeline only).
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

/// Build a tiny CycloneDX 1.6 SBOM body suitable as a referrer blob.
/// `marker` is a caller-supplied string embedded in the `metadata.component.name`
/// field so tests can prove byte-identity round-tripping through waybill.
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

/// Mount the Referrers endpoint for a given manifest digest with an
/// `ImageIndex` payload advertising a single CDX-JSON SBOM descriptor at
/// `descriptor_digest`. Also mounts the blob endpoint that returns
/// `referrer_bytes`.
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

/// Mount an oversize-declared referrer that would trip
/// `WAYBILL_REFERRER_MAX_BYTES` — the `size` field claims 200 MiB even
/// though the blob body would be a few hundred bytes if fetched.
async fn mount_oversize_referrer(
    server: &MockServer,
    repo: &str,
    manifest_digest: &str,
) -> String {
    let dummy_body = b"{\"bomFormat\":\"CycloneDX\",\"specVersion\":\"1.6\"}";
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

    descriptor_digest
}

async fn mount_empty_referrers(
    server: &MockServer,
    repo: &str,
    manifest_digest: &str,
) {
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
async fn either_prefers_referrer_when_available() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m186-us1";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;

    let referrer_bytes = build_cdx_referrer("waybill-m186-us1-referrer");
    let descriptor_digest =
        mount_referrer(&server, repo, &image.manifest_digest, &referrer_bytes).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us1.cdx.json");

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
            "either",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "waybill exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );

    // Byte-identity check: the emitted output MUST be byte-identical to the
    // referrer blob body (FR-006).
    let got = std::fs::read(&output).unwrap();
    assert_eq!(
        got, referrer_bytes,
        "referrer bytes NOT emitted verbatim; byte-identity broken (FR-006)"
    );

    // SC-005 audit-log — stderr must carry provenance strings. Tracing may
    // emit ANSI color escapes between the key and value, so we check that
    // BOTH the field name AND the field value are present rather than
    // asserting a specific `key=value` byte sequence.
    assert!(
        stderr.contains("emitted SBOM from OCI Referrers API"),
        "expected stderr to contain the FR-007 audit-log preamble. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("sbom_source") && stderr.contains("referrer"),
        "expected stderr to contain `sbom_source` and `referrer` provenance markers. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains(&descriptor_digest),
        "expected stderr to contain descriptor_digest={descriptor_digest}. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("application/vnd.cyclonedx+json"),
        "expected stderr to contain media_type=application/vnd.cyclonedx+json. stderr:\n{stderr}",
    );
}

#[tokio::test]
async fn either_falls_through_to_scan_when_no_referrer() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m186-us1-empty";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    mount_empty_referrers(&server, repo, &image.manifest_digest).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us1-fallthrough.cdx.json");

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
            "either",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "waybill exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );
    // Fall-through log per F3 remediation.
    assert!(
        stderr.contains("falling through to scan"),
        "expected stderr to contain the fall-through INFO log. stderr:\n{stderr}",
    );
    // Scan-derived output: should be a full CDX 1.6 document with our
    // fixture's ecosystem shape (empty components on the waybill-m186-test
    // synthetic os-release, but a valid CDX metadata block).
    let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(&output).unwrap())
        .expect("scan output is valid JSON");
    assert_eq!(parsed.get("bomFormat").and_then(|v| v.as_str()), Some("CycloneDX"));
}

#[tokio::test]
async fn either_falls_through_silently_on_404() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m186-us1-404";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    mount_referrers_404(&server, repo, &image.manifest_digest).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us1-404.cdx.json");

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
            "either",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "waybill exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );
    // 404 is a spec-blessed signal, not an error — log at INFO not WARN.
    assert!(
        stderr.contains("HTTP 404")
            || stderr.contains("does not support OCI Distribution Spec v1.1"),
        "expected stderr to log the HTTP 404 INFO diagnostic. stderr:\n{stderr}",
    );
    // Scan pipeline should have produced a valid CDX document.
    let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(&output).unwrap())
        .expect("scan output is valid JSON");
    assert_eq!(parsed.get("bomFormat").and_then(|v| v.as_str()), Some("CycloneDX"));
}

#[tokio::test]
async fn either_falls_through_on_size_cap_exceeded() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/waybill-m186-us1-oversize";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    let _descriptor_digest =
        mount_oversize_referrer(&server, repo, &image.manifest_digest).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us1-oversize.cdx.json");

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
            "either",
            "--format",
            "cyclonedx-json",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "waybill exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );
    // Size-cap WARN log per research.md Decision 4.
    assert!(
        stderr.contains("oversize referrer descriptor")
            || stderr.contains("WAYBILL_REFERRER_MAX_BYTES"),
        "expected stderr to log the size-cap WARN. stderr:\n{stderr}",
    );
    // Fell through to scan.
    let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(&output).unwrap())
        .expect("scan output is valid JSON");
    assert_eq!(parsed.get("bomFormat").and_then(|v| v.as_str()), Some("CycloneDX"));
}
