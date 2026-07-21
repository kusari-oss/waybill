//! End-to-end integration test for the docker-daemon-first image
//! source path (milestone 044 commit 1). Skips itself when:
//!
//! - the `docker` CLI is not on `$PATH`, OR
//! - `docker info` fails (daemon not running, unreachable
//!   `DOCKER_HOST`, etc.), OR
//! - the env var `WAYBILL_SKIP_DOCKER_INTEGRATION` is non-empty.
//!
//! When all gates pass, the test pulls a tiny well-known image
//! (`alpine:3.19`) into the local docker daemon, then runs
//! `waybill sbom scan --image alpine:3.19 --image-src docker` and
//! asserts:
//!
//! - the CLI exits 0,
//! - tracing output mentions the docker-daemon source ("found image
//!   in local docker daemon"),
//! - a CycloneDX SBOM is produced with at least 5 components
//!   (alpine:3.19's apk db ships ~14–15 base packages).
//!
//! This is the regression gate for the user-reported flow:
//! "the image is here locally and should look at that". Without
//! this test, an accidental revert to "always pull from registry"
//! would silently break the docker-daemon path while the unit-test-
//! level shell-out tests would still pass.

use std::path::PathBuf;
use std::process::{Command, Stdio};

mod common;

/// Compose the runtime gates: returns `true` when this test should
/// skip (docker missing or daemon unreachable). Centralized here so
/// the same logic applies to every assertion path.
fn should_skip() -> bool {
    if std::env::var_os("WAYBILL_SKIP_DOCKER_INTEGRATION").is_some() {
        eprintln!("skipping: WAYBILL_SKIP_DOCKER_INTEGRATION set");
        return true;
    }
    let version = Command::new("docker")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match version {
        Ok(s) if s.success() => {}
        _ => {
            eprintln!("skipping: `docker --version` failed; CLI not installed");
            return true;
        }
    }
    let info = Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match info {
        Ok(s) if s.success() => false,
        _ => {
            eprintln!(
                "skipping: `docker info` failed; daemon not running or DOCKER_HOST unreachable"
            );
            true
        }
    }
}

/// `alpine:3.19` is small (~7 MB), stable, and reliably available on
/// Docker Hub. Hard-pinning the tag means the test gives the same
/// component count regardless of when it runs — alpine point-releases
/// don't change the base apk-db shape.
const TEST_IMAGE: &str = "alpine:3.19";

#[test]
fn docker_daemon_source_scans_locally_cached_image_end_to_end() {
    if should_skip() {
        return;
    }

    // Make sure the image is cached locally. `docker pull` is
    // idempotent — already-cached digests are a no-op.
    let pull = Command::new("docker")
        .args(["pull", TEST_IMAGE])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn `docker pull`");
    assert!(
        pull.status.success(),
        "`docker pull {TEST_IMAGE}` failed: {}",
        String::from_utf8_lossy(&pull.stderr)
    );

    // Run waybill in a fresh tempdir so we don't leak `waybill.cdx.json`
    // into the repo or the user's CWD.
    let tmp = tempfile::tempdir().expect("create tempdir");
    let output = Command::new(common::bin())
        .args([
            "sbom",
            "scan",
            "--image",
            TEST_IMAGE,
            "--image-src",
            "docker",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("spawn waybill");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "waybill exited {}; stderr:\n{stderr}\nstdout:\n{stdout}",
        output.status
    );
    assert!(
        stderr.contains("found image in local docker daemon"),
        "expected docker-daemon source to be reported in tracing output; got stderr:\n{stderr}"
    );

    // SBOM should land at `waybill.cdx.json` in the tempdir (default
    // filename for the cyclonedx-json format).
    let sbom_path: PathBuf = tmp.path().join("waybill.cdx.json");
    assert!(
        sbom_path.exists(),
        "expected SBOM at {}; tempdir contents: {:?}",
        sbom_path.display(),
        std::fs::read_dir(tmp.path())
            .ok()
            .map(|rd| rd
                .filter_map(|e| e.ok().map(|e| e.file_name()))
                .collect::<Vec<_>>())
    );
    let sbom_bytes = std::fs::read(&sbom_path).expect("read SBOM");
    let sbom: serde_json::Value =
        serde_json::from_slice(&sbom_bytes).expect("SBOM is valid JSON");
    let components = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .expect("CycloneDX SBOM must carry a `components` array");
    assert!(
        components.len() >= 5,
        "alpine:3.19 should yield ≥5 components from its apk db; got {}",
        components.len()
    );
}

#[test]
fn docker_only_source_errors_helpfully_when_image_not_cached() {
    if should_skip() {
        return;
    }

    // Reference that cannot exist in any cache — random suffix is
    // not in any registry waybill would query. With `--image-src
    // docker` we never reach the registry; the docker-inspect probe
    // returns Absent and we expect the "not found in any of the
    // configured sources" error.
    let bogus = "registry.invalid.waybill-test.example/no-such-image:nope-d9f4b2";

    let tmp = tempfile::tempdir().expect("create tempdir");
    let output = Command::new(common::bin())
        .args(["sbom", "scan", "--image", bogus, "--image-src", "docker"])
        .current_dir(tmp.path())
        .output()
        .expect("spawn waybill");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "waybill should have errored on a docker-only miss"
    );
    assert!(
        stderr.contains("not found in any of the configured `--image-src` sources"),
        "expected helpful 'not found in any source' error; got stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("docker"),
        "expected error to enumerate the [docker] source it tried; got stderr:\n{stderr}"
    );
}
