//! Milestone 182 US4 — multi-flag composition regression guard.
//!
//! Verifies that the three m182 flags can coexist in a single scan
//! invocation without contamination (FR-009, FR-010).
//!
//! **Scope**: CLI-parse-layer composition test — proves the flags
//! coexist without triggering clap conflicts, and that plain-HTTP
//! wins over skip-verify on the same host (FR-009). Full multi-
//! registry end-to-end validation deferred to T035 manual verification.

#![cfg(feature = "oci-registry")]

use std::io::Write;
use std::process::Command;

use serde_json::json;
use sha2::Digest;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Mount a plain-HTTP OCI-conformant registry with a single
/// minimal image (mirrors `oci_pull_plain_http.rs`'s fixture).
async fn mount_minimal_image(server: &MockServer, repo: &str, tag: &str) {
    let uncompressed_tar = {
        let mut builder = tar::Builder::new(Vec::<u8>::new());
        let body = b"ID=waybill-m182-us4\nVERSION=1\n";
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

    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/manifests/{tag}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(manifest_bytes)
                .insert_header("content-type", "application/vnd.oci.image.manifest.v1+json"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/blobs/{config_digest}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(config_bytes)
                .insert_header("content-type", "application/vnd.oci.image.config.v1+json"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/{repo}/blobs/{layer_digest}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(layer_bytes)
                .insert_header("content-type", "application/vnd.oci.image.layer.v1.tar+gzip"),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn us4_all_three_flags_coexist_without_clap_conflict() {
    // Regression pin: passing all three m182 flags in one invocation
    // must not trigger a clap conflict or hidden interaction. The
    // invocation may fail at network time (host unreachable), but
    // MUST NOT fail at CLI-parse time.
    let mut tf = tempfile::NamedTempFile::new().unwrap();
    let ca = rcgen::generate_simple_self_signed(vec!["us4.test.invalid".to_string()]).unwrap();
    tf.write_all(ca.cert.pem().as_bytes()).unwrap();
    tf.flush().unwrap();

    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "unreachable.test.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--insecure-registry",
            "different-host.test.invalid:5000",
            "--registry-ca-cert",
            tf.path().to_str().unwrap(),
            "--insecure-tls-skip-verify",
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // Fails at transport-time (unreachable host), NOT at parse-time.
    assert!(
        !out.status.success(),
        "expected waybill to fail against unreachable host. stderr:\n{stderr}",
    );
    // Parse-error signatures — none should appear.
    assert!(
        !stderr.contains("cannot be used with"),
        "unexpected clap conflict message. stderr:\n{stderr}",
    );
    assert!(
        !stderr.contains("the argument"),
        "unexpected clap-arg conflict. stderr:\n{stderr}",
    );
    // WARN log fires because skip-verify is set (FR-007) — regardless
    // of whether the host is reachable.
    assert!(
        stderr.contains("TLS verification DISABLED"),
        "expected WARN log when --insecure-tls-skip-verify is set. stderr:\n{stderr}",
    );
}

#[tokio::test]
async fn us4_insecure_registry_wins_over_skip_verify_on_same_host() {
    // FR-009: when --insecure-registry matches a host, waybill uses
    // http://... — skip-verify is moot because there's no TLS
    // handshake. This is a plain-HTTP wiremock target where BOTH
    // flags are set for the same host; the pull must succeed via the
    // plain-HTTP path.
    let server = MockServer::start().await;
    let repo = "library/waybill-m182-us4";
    let tag = "1.0";
    mount_minimal_image(&server, repo, tag).await;
    let uri = server.uri();
    let hostport = uri.strip_prefix("http://").unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let output = tempdir.path().join("m182-us4.cdx.json");
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
            "--insecure-tls-skip-verify",
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
        "expected plain-HTTP pull to succeed (FR-009: insecure-registry \
         wins over skip-verify). stderr:\n{stderr}",
    );
    // Both flags set → the WARN log still fires (skip-verify is
    // scan-global — it affects HTTPS pulls in *other* invocations
    // within the same scan). Documented behavior per FR-007.
    assert!(
        stderr.contains("TLS verification DISABLED"),
        "expected WARN log when --insecure-tls-skip-verify is set, \
         even for a plain-HTTP-matched registry. stderr:\n{stderr}",
    );
}
