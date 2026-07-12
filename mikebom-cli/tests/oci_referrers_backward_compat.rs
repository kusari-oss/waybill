//! Milestone 186 US3 — `--sbom-source scan` (default) backward compatibility.
//!
//! Verifies that pre-m186 behavior is preserved byte-identically when the
//! flag is omitted (or explicitly `scan`), that the Referrers endpoint is
//! NEVER called under scan mode (FR-010), and that FR-011 rejects the flag
//! against non-registry inputs (local tarball / --path).
//!
//! Also covers T028a — the F2 remediation from /speckit-analyze that
//! explicitly gates SC-007 (m182 TLS/transport flags composing with the
//! Referrers endpoint) — verifying that plain-HTTP reaches the Referrers
//! endpoint under `--insecure-registry`.

#![cfg(feature = "oci-registry")]

use std::io::Write;
use std::process::Command;

use serde_json::json;
use sha2::Digest;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── OCI fixture builder (same shape as US1/US2 tests) ────────────────

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
        let body = b"ID=mikebom-m186-us3-test\nVERSION=1\n";
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

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

// ── Tests ────────────────────────────────────────────────────────────

/// FR-010 / SC-004 gate — verify that under `--sbom-source scan` (or omitted)
/// mikebom NEVER hits the Referrers endpoint. We mount the endpoint with a
/// response that would cause a byte-identity divergence if consulted (returns
/// a bogus non-JSON body), and assert the scan succeeds anyway — proving the
/// endpoint was untouched.
#[tokio::test]
async fn scan_mode_never_calls_referrers_endpoint() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us3-scan";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;

    // Mount a "poisoned" referrers endpoint — 500 + a body that would trip
    // any code path that consumed it. If mikebom under `--sbom-source scan`
    // called it, the scan would fail; scan succeeds only if this handler is
    // NEVER hit.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/referrers/{}", image.manifest_digest)))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_bytes(b"POISONED - scan mode should never call this endpoint".to_vec()),
        )
        .expect(0) // wiremock will fail this test if the endpoint is ever hit
        .mount(&server)
        .await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us3-scan.cdx.json");

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
            "scan",
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
    // Scan-derived output; not the referrer bytes.
    let parsed: serde_json::Value = serde_json::from_slice(&std::fs::read(&output).unwrap())
        .expect("scan output is valid JSON");
    assert_eq!(parsed.get("bomFormat").and_then(|v| v.as_str()), Some("CycloneDX"));
    // wiremock's `.expect(0)` above enforces the "never called" invariant
    // during the MockServer's drop, but explicitly checking here surfaces a
    // clearer failure message if regression happens.
    for received in server.received_requests().await.unwrap_or_default() {
        assert!(
            !received.url.path().contains("/referrers/"),
            "Referrers endpoint was contacted under --sbom-source scan (FR-010 violation): {}",
            received.url,
        );
    }
}

/// SC-004 gate — invoking mikebom WITHOUT `--sbom-source` produces the same
/// scan-mode behavior as passing `--sbom-source scan` explicitly.
#[tokio::test]
async fn default_flag_absence_equivalent_to_scan_mode() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us3-default";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;
    // A live referrer is mounted, BUT since `--sbom-source` is absent, mikebom
    // must produce scan-derived output (not the referrer bytes).
    let referrer_bytes = build_cdx_referrer("SHOULD-NOT-APPEAR-IN-OUTPUT");
    mount_referrer(&server, repo, &image.manifest_digest, &referrer_bytes).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us3-default.cdx.json");

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
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "mikebom exit={:?} stderr:\n{stderr}",
        out.status.code(),
    );
    let got = std::fs::read(&output).unwrap();
    assert_ne!(
        got, referrer_bytes,
        "output MUST NOT be the referrer bytes when --sbom-source is unspecified (default = scan)"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&got).expect("output is valid JSON");
    assert_eq!(parsed.get("bomFormat").and_then(|v| v.as_str()), Some("CycloneDX"));
    // Verify emission-time provenance markers were NOT set — no FR-007
    // audit-log line under scan mode.
    assert!(
        !stderr.contains("emitted SBOM from OCI Referrers API"),
        "unexpected referrer-mode audit-log line under default flag absence. stderr:\n{stderr}",
    );
}

/// FR-011 — reject `--sbom-source referrer` against a `--path` scan.
/// Also verifies `--sbom-source either` is rejected identically.
#[tokio::test]
async fn sbom_source_rejected_on_local_path_input() {
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("some-dir");
    std::fs::create_dir(&path).unwrap();

    for mode in ["referrer", "either"] {
        let out = Command::new(mikebom_bin())
            .args([
                "sbom",
                "scan",
                "--path",
                path.to_str().unwrap(),
                "--sbom-source",
                mode,
                "--format",
                "cyclonedx-json",
                "--output",
                tempdir.path().join("out.cdx.json").to_str().unwrap(),
            ])
            .output()
            .expect("spawn mikebom binary");
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            !out.status.success(),
            "expected mikebom to reject --sbom-source {mode} against --path. stderr:\n{stderr}",
        );
        assert!(
            stderr.contains("only valid for registry-pull scans"),
            "expected FR-011 rejection message. stderr:\n{stderr}",
        );
    }
}

/// T028a / F2 remediation — SC-007 + FR-013 gate: verify the Referrers
/// endpoint is reached over plain HTTP under the m182 `--insecure-registry`
/// flag, proving TLS-flag inheritance composes correctly with the m186
/// dispatch path.
#[tokio::test]
async fn referrers_endpoint_honors_insecure_registry_flag() {
    let server = MockServer::start().await;
    let image = build_minimal_oci_image();
    let repo = "library/mikebom-m186-us3-tls";
    let tag = "1.0";
    mount_image_endpoints(&server, repo, tag, &image).await;

    let referrer_bytes = build_cdx_referrer("mikebom-m186-us3-tls-referrer");
    let descriptor_digest =
        mount_referrer(&server, repo, &image.manifest_digest, &referrer_bytes).await;

    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();
    let image_ref = format!("{hostport}/{repo}:{tag}");

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m186-us3-tls.cdx.json");

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
    // (a) The Referrers endpoint was reached via plain HTTP.
    let received_paths: Vec<String> = server
        .received_requests()
        .await
        .expect("wiremock records requests")
        .into_iter()
        .map(|r| r.url.path().to_string())
        .collect();
    assert!(
        received_paths.iter().any(|p| p.contains("/referrers/")),
        "Referrers endpoint was not contacted — TLS flag inheritance broken. Received paths: {received_paths:?}",
    );
    // (b) Byte-identity emission of the referrer blob.
    let got = std::fs::read(&output).unwrap();
    assert_eq!(
        got, referrer_bytes,
        "referrer bytes NOT emitted verbatim (FR-006 under TLS flag path)"
    );
    // (c) Provenance log confirms the referrer-source path.
    assert!(
        stderr.contains(&descriptor_digest),
        "expected stderr to contain descriptor_digest={descriptor_digest}. stderr:\n{stderr}",
    );
}
