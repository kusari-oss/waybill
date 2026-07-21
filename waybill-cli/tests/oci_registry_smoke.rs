//! Network-gated end-to-end smoke tests for the OCI registry-pull
//! pipeline (milestones 031 anonymous + 034 authenticated).
//!
//! These tests pull real OCI images from real registries, run them
//! through the full waybill pipeline (oci_pull →
//! docker_image::extract → scan → SBOM emission), and verify the
//! output is well-formed. They run ONLY when:
//!
//!   1. The crate is built with `--features oci-registry`, AND
//!   2. The `WAYBILL_OCI_NETWORK_TESTS=1` env var is set.
//!
//! The default Linux + ebpf + macOS CI lanes do NOT set the env
//! var, so these tests are silently skipped on every standard PR.
//! A follow-on milestone may add a dedicated
//! `lint-and-test-oci-network` job that flips it on; for now they
//! ship as opt-in only.
//!
//! The authenticated smoke test additionally requires
//! `WAYBILL_OCI_AUTH_TESTS=1` and the env var
//! `WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF` pointing at a private image
//! you already have credentials for in `~/.docker/config.json`.
//! Documented in PR descriptions for manual verification.
//!
//! To run locally:
//!
//! ```sh
//! WAYBILL_OCI_NETWORK_TESTS=1 cargo +stable test \
//!     -p waybill --features oci-registry --test oci_registry_smoke
//!
//! WAYBILL_OCI_NETWORK_TESTS=1 \
//! WAYBILL_OCI_AUTH_TESTS=1 \
//! WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF=ghcr.io/<you>/<priv>:tag \
//!     cargo +stable test \
//!     -p waybill --features oci-registry --test oci_registry_smoke
//! ```

#![cfg(feature = "oci-registry")]

use std::process::Command;

fn network_tests_enabled() -> bool {
    std::env::var("WAYBILL_OCI_NETWORK_TESTS").ok().as_deref() == Some("1")
}

fn auth_tests_enabled() -> bool {
    std::env::var("WAYBILL_OCI_AUTH_TESTS").ok().as_deref() == Some("1")
}

#[test]
fn pulls_alpine_3_19_and_emits_apk_components() {
    if !network_tests_enabled() {
        eprintln!(
            "skipping: set WAYBILL_OCI_NETWORK_TESTS=1 to run network-gated smoke tests"
        );
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("alpine.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mikebom"))
        .arg("--offline") // VEX/CD enrichment off; pure registry pull
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg("alpine:3.19")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "waybill failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read alpine.cdx.json");
    let sbom: serde_json::Value = serde_json::from_slice(&bytes).expect("valid CDX JSON");
    let components = sbom["components"]
        .as_array()
        .expect("CDX components array");
    // Alpine 3.19 base image has ~15-20 apk packages; we
    // intentionally don't pin the exact count to avoid breaking
    // when alpine bumps a minor version. Just assert non-empty
    // and at least one well-formed apk PURL.
    assert!(
        !components.is_empty(),
        "alpine:3.19 should yield ≥1 component; got 0"
    );
    let has_apk_purl = components.iter().any(|c| {
        c["purl"]
            .as_str()
            .is_some_and(|p| p.starts_with("pkg:apk/alpine/"))
    });
    assert!(
        has_apk_purl,
        "alpine:3.19 should yield at least one pkg:apk/alpine/* PURL; got components: {}",
        serde_json::to_string_pretty(&components).unwrap_or_default()
    );

    // Milestone 039: per-file evidence MUST be populated for apk
    // components (mirrors the milestone-038 deb assertion). Pre-039
    // this was always 0 because file_hashes.rs was dpkg-only.
    //
    // Milestone 040 US2: each populated occurrence's
    // `additionalContext` MUST also carry `sha1` from the apk
    // package's `Z:` line. Pre-040 this was always None.
    let mut total_occurrences = 0usize;
    let mut sha256_seen = false;
    let mut sha1_seen = false;
    for c in components {
        let occs = c["evidence"]["occurrences"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        total_occurrences += occs.len();
        for o in occs {
            let Some(ctx_str) = o["additionalContext"].as_str() else {
                continue;
            };
            let Ok(ctx) = serde_json::from_str::<serde_json::Value>(ctx_str) else {
                continue;
            };
            if let Some(s) = ctx["sha256"].as_str() {
                if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                    sha256_seen = true;
                }
            }
            if let Some(s) = ctx["sha1"].as_str() {
                if s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                    sha1_seen = true;
                }
            }
        }
    }
    assert!(
        total_occurrences > 0,
        "milestone 039 (apk per-file evidence) should populate \
         evidence.occurrences[]; got 0 across {} components — has \
         hash_apk_package_files regressed?",
        components.len()
    );
    assert!(
        sha256_seen,
        "at least one apk occurrence should carry a 64-hex SHA-256 in additionalContext; got none"
    );
    assert!(
        sha1_seen,
        "milestone 040 US2: at least one apk occurrence should carry a 40-hex \
         SHA-1 (apk-provided Z:-line cross-ref) in additionalContext; got none"
    );
}

#[test]
fn pulls_distroless_static_and_emits_dpkg_status_d_components() {
    if !network_tests_enabled() {
        eprintln!(
            "skipping: set WAYBILL_OCI_NETWORK_TESTS=1 to run network-gated smoke tests"
        );
        return;
    }

    // distroless static-debian12 ships per-package metadata at
    // `/var/lib/dpkg/status.d/<pkg>` files (no monolithic
    // `/var/lib/dpkg/status` daemon-managed file). Milestone 037
    // / #64 closed the previous "0 components" gap; waybill now
    // surfaces the 4 documented packages: base-files, media-types,
    // netbase, tzdata.
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("distroless.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mikebom"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg("gcr.io/distroless/static-debian12:latest")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "waybill failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read distroless.cdx.json");
    let sbom: serde_json::Value = serde_json::from_slice(&bytes).expect("valid CDX JSON");
    // SBOM is well-formed — the fields we care about are present.
    assert!(sbom["bomFormat"].as_str().is_some(), "missing bomFormat");
    assert!(
        sbom["specVersion"].as_str().is_some(),
        "missing specVersion"
    );
    assert!(
        sbom["serialNumber"].as_str().is_some(),
        "missing serialNumber"
    );
    let components = sbom["components"].as_array().expect("components array");
    assert!(
        components.len() >= 4,
        "distroless static image should yield at least 4 components (base-files, \
         media-types, netbase, tzdata); got {} — is the dpkg status.d/ reader \
         (#64) regressing?",
        components.len()
    );
    let names: std::collections::BTreeSet<&str> = components
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    for expected in ["base-files", "media-types", "netbase", "tzdata"] {
        assert!(
            names.contains(expected),
            "distroless image should contain `{expected}`; got {names:?}"
        );
    }

    // Milestone 038: per-file evidence MUST be populated for
    // status.d/-layout deb components. Pre-038, this count was 0;
    // post-038 the .md5sums-derived path-list synthesis populates
    // evidence.occurrences[] for each component.
    //
    // CDX shape per waybill-cli/src/generate/cyclonedx/evidence.rs:
    //   evidence.occurrences[] = [{ location, additionalContext }]
    // where additionalContext is a JSON-string-encoded map carrying
    // sha256 + optionally md5. We parse it back to verify the
    // SHA-256 surfaced.
    let mut total_occurrences = 0usize;
    let mut sha256_seen = false;
    for c in components {
        let occs = c["evidence"]["occurrences"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        total_occurrences += occs.len();
        for o in occs {
            let Some(ctx_str) = o["additionalContext"].as_str() else {
                continue;
            };
            let Ok(ctx) = serde_json::from_str::<serde_json::Value>(ctx_str) else {
                continue;
            };
            if let Some(s) = ctx["sha256"].as_str() {
                if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                    sha256_seen = true;
                }
            }
        }
    }
    assert!(
        total_occurrences > 0,
        "milestone 038 (per-file evidence for status.d/ images) should populate \
         evidence.occurrences[]; got 0 occurrences across {} components — has \
         the .md5sums-derived path-list synthesis regressed?",
        components.len()
    );
    assert!(
        sha256_seen,
        "at least one occurrence should carry a 64-hex SHA-256 in \
         additionalContext; got none"
    );
}

/// Warm-cache speedup smoke test (milestone 036 / 031.z).
///
/// Pulls alpine:3.19 twice into a fresh tempdir-backed cache; the
/// second pull reads every blob from disk and produces a
/// byte-identical SBOM. Skipped silently unless
/// `WAYBILL_OCI_NETWORK_TESTS=1`.
#[test]
fn repeat_pull_uses_cache_for_warm_layers() {
    if !network_tests_enabled() {
        eprintln!(
            "skipping: set WAYBILL_OCI_NETWORK_TESTS=1 to run network-gated smoke tests"
        );
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).expect("create cache_dir");
    let out_path_1 = tmp.path().join("alpine1.cdx.json");
    let out_path_2 = tmp.path().join("alpine2.cdx.json");

    let invoke = |out: &std::path::Path| -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_mikebom"))
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--image")
            .arg("alpine:3.19")
            .arg("--format")
            .arg("cyclonedx-json")
            .arg("--output")
            .arg(out)
            .env("WAYBILL_OCI_CACHE_DIR", &cache_dir)
            .output()
            .expect("waybill should run")
    };

    // First pull: cache is empty → network fetches, writes cache.
    let out1 = invoke(&out_path_1);
    assert!(
        out1.status.success(),
        "first waybill pull failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out1.stdout),
        String::from_utf8_lossy(&out1.stderr),
    );

    // After the first pull, the cache directory should contain
    // some `sha256/<hex>` files. Verify by counting them — at
    // least one (config + layers) must be present.
    let blob_dir = cache_dir.join("sha256");
    let blob_count = std::fs::read_dir(&blob_dir)
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);
    assert!(
        blob_count >= 2,
        "first pull should have populated the cache; got {blob_count} blobs in {}",
        blob_dir.display()
    );

    // Second pull: same image, same cache → blobs read from disk.
    let out2 = invoke(&out_path_2);
    assert!(
        out2.status.success(),
        "second waybill pull failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr),
    );

    // SBOMs from both runs should be byte-identical (same image,
    // same scan logic; cache hit/miss doesn't change the output).
    // Strip the timestamp + serialNumber + workspace path before
    // comparing so we're robust to per-run wall-clock and uuid
    // variation (the standard cross-host normalization the existing
    // 27-fixture goldens use).
    let canonical = |bytes: &[u8]| -> serde_json::Value {
        let mut v: serde_json::Value =
            serde_json::from_slice(bytes).expect("valid CDX JSON");
        if let Some(metadata) = v.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            metadata.remove("timestamp");
        }
        if let Some(o) = v.as_object_mut() {
            o.remove("serialNumber");
        }
        v
    };
    let v1 = canonical(&std::fs::read(&out_path_1).expect("read 1"));
    let v2 = canonical(&std::fs::read(&out_path_2).expect("read 2"));
    assert_eq!(
        v1, v2,
        "warm-cache pull should produce a canonical-identical SBOM to the cold-cache pull"
    );
}

/// Cross-arch end-to-end smoke test (milestone 035 / 031.y).
///
/// Pulls alpine:3.19 with `--image-platform` set to a non-host arch
/// and asserts the SBOM's apk PURLs reflect the requested arch's
/// alpine `apk` arch name (e.g. linux/amd64 → x86_64,
/// linux/arm64 → aarch64). Skipped silently unless
/// `WAYBILL_OCI_NETWORK_TESTS=1`.
#[test]
fn pulls_alpine_with_image_platform_override() {
    if !network_tests_enabled() {
        eprintln!(
            "skipping: set WAYBILL_OCI_NETWORK_TESTS=1 to run network-gated smoke tests"
        );
        return;
    }

    // Pick a non-host arch so the test exercises the override path
    // even when the host is itself one of the common arches.
    let (target_platform, expected_apk_arch) = match std::env::consts::ARCH {
        "x86_64" => ("linux/arm64", "aarch64"),
        _ => ("linux/amd64", "x86_64"),
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("alpine-cross-arch.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mikebom"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg("alpine:3.19")
        .arg("--image-platform")
        .arg(target_platform)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "waybill failed for {target_platform}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read alpine-cross-arch.cdx.json");
    let sbom: serde_json::Value = serde_json::from_slice(&bytes).expect("valid CDX JSON");
    let components = sbom["components"]
        .as_array()
        .expect("CDX components array");
    assert!(!components.is_empty(), "alpine should yield ≥1 component");

    // apk PURLs encode the arch as `arch=<x86_64|aarch64|...>`. We
    // expect the user-requested arch, NOT the host's.
    let qualifier = format!("arch={expected_apk_arch}");
    let has_target_arch = components.iter().any(|c| {
        c["purl"]
            .as_str()
            .is_some_and(|p| p.starts_with("pkg:apk/alpine/") && p.contains(&qualifier))
    });
    assert!(
        has_target_arch,
        "expected at least one apk PURL with `{qualifier}` for {target_platform}; \
         got components: {}",
        serde_json::to_string_pretty(&components).unwrap_or_default()
    );
}

/// Authenticated end-to-end smoke test (milestone 034 / 031.x).
///
/// Pulls a private image from the registry using credentials
/// resolved from `~/.docker/config.json`. The image reference is
/// passed via `WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF` so that this
/// test doesn't bake in any specific user's private repo.
///
/// Skipped silently unless BOTH gate env vars are set:
///   - `WAYBILL_OCI_NETWORK_TESTS=1`
///   - `WAYBILL_OCI_AUTH_TESTS=1`
///
/// Verifies the scan succeeds AND that no credential bytes leak to
/// stdout / stderr. The `auth` field's base64 string is what we'd
/// most readily detect in a regression — if it ever shows up in
/// program output, fail loudly.
#[test]
fn pulls_private_image_via_docker_keychain() {
    if !network_tests_enabled() || !auth_tests_enabled() {
        eprintln!(
            "skipping: set WAYBILL_OCI_NETWORK_TESTS=1 and \
             WAYBILL_OCI_AUTH_TESTS=1 to run the authenticated smoke test"
        );
        return;
    }

    let image_ref = match std::env::var("WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF") {
        Ok(r) if !r.is_empty() => r,
        _ => {
            eprintln!(
                "skipping: WAYBILL_OCI_AUTH_PRIVATE_IMAGE_REF must point at a \
                 private image you have credentials for in ~/.docker/config.json"
            );
            return;
        }
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("private.cdx.json");
    let output = Command::new(env!("CARGO_BIN_EXE_mikebom"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--image")
        .arg(&image_ref)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .env("RUST_LOG", "debug")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "waybill failed for {image_ref}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Best-effort secret-leak guard. If the user's config.json puts a
    // credential value in `auths.<reg>.auth`, we can read it back here
    // and confirm it doesn't appear in waybill's output. This is a
    // sanity check, not a security guarantee — credential helpers
    // store the secret outside config.json so we can't guard them.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(home) = std::env::var_os("HOME") {
        let cfg_path = std::path::PathBuf::from(home).join(".docker/config.json");
        if let Ok(cfg_bytes) = std::fs::read(&cfg_path) {
            if let Ok(cfg_json) = serde_json::from_slice::<serde_json::Value>(&cfg_bytes) {
                if let Some(auths) = cfg_json.get("auths").and_then(|v| v.as_object()) {
                    for (_reg, entry) in auths {
                        for field in ["auth", "identitytoken"] {
                            if let Some(secret) = entry.get(field).and_then(|v| v.as_str()) {
                                if !secret.is_empty() {
                                    assert!(
                                        !stdout.contains(secret),
                                        "credential `{field}` value leaked to stdout"
                                    );
                                    assert!(
                                        !stderr.contains(secret),
                                        "credential `{field}` value leaked to stderr"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // SBOM is well-formed.
    let bytes = std::fs::read(&out_path).expect("read private.cdx.json");
    let sbom: serde_json::Value = serde_json::from_slice(&bytes).expect("valid CDX JSON");
    assert!(sbom["bomFormat"].as_str().is_some(), "missing bomFormat");
    assert!(sbom["specVersion"].as_str().is_some(), "missing specVersion");
}
