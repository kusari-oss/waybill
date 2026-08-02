//! Milestone 224: integration tests for the Pants coursier JVM
//! lockfile reader. Each test invokes waybill as a subprocess against
//! a synthetic fixture and asserts the emitted SBOM contains the
//! expected components + annotations + dependency edges + log lines.
//!
//! Fixtures live at `waybill-cli/tests/fixtures/pants_coursier_jvm/`
//! per T008-T010. Every fixture uses synthetic
//! `dev.waybill.fixture:*` Maven coordinates per memory
//! `feedback_fixture_synthetic_package_names`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;

/// Crate-local pants_coursier_jvm fixture path resolver.
fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_coursier_jvm")
        .join(rel)
}

/// Run `waybill sbom scan` against a fixture. Emits ONE format by
/// default; callers pass `extra_args` for extra `--format` +
/// `--output <fmt>=<path>` pairs to emit multiple formats in a
/// single invocation (per SC-001).
fn run_scan(
    fixture_path: &Path,
    output: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(output)
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.output().expect("waybill invocation")
}

/// Parse the emitted CDX JSON.
fn read_cdx(path: &Path) -> serde_json::Value {
    let raw = std::fs::read(path).expect("read cdx");
    serde_json::from_slice(&raw).expect("parse cdx")
}

/// Extract a CDX component's property value by name.
fn get_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
}

/// Find the pants-coursier-jvm-sourced components in the CDX output.
/// (The scan may also emit non-JVM components from the fixture root;
/// this filters to just our reader's output.)
fn pants_jvm_components(cdx: &serde_json::Value) -> Vec<&serde_json::Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:maven/dev.waybill.fixture/"))
        })
        .collect()
}

/// Strip ANSI escape codes from tracing pretty-format output.
fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").expect("valid regex");
    re.replace_all(s, "").to_string()
}

// ---------------------------------------------------------------------
// US1 T012 — minimal Pants JVM lockfile emits 3 pkg:maven components
// with sha256 hashes, dep edges, and waybill:pants-resolve=default,
// across BOTH CDX and SPDX 2.3 in one scan invocation per SC-001.
// ---------------------------------------------------------------------

#[test]
fn us1_minimal_jvm_lockfile_emits_3_maven_components() {
    let fixture_dir = fixture("minimal_jvm");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let spdx_path = tmp.path().join("out.spdx.json");

    // Multi-format emission per SC-001. Use "--output <fmt>=<path>".
    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture_dir)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx_path.display()))
        .arg("--no-deep-hash")
        .env("RUST_LOG", "info")
        .output()
        .expect("waybill invocation");
    assert!(
        out.status.success(),
        "waybill exited nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    // ---- CDX assertions ----
    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert_eq!(
        jvm.len(),
        3,
        "expected 3 pants-jvm components in CDX, got {} — components:\n{}",
        jvm.len(),
        serde_json::to_string_pretty(&jvm).unwrap_or_default(),
    );
    let expected_purls: std::collections::HashSet<String> = [
        "pkg:maven/dev.waybill.fixture/core@1.0.0",
        "pkg:maven/dev.waybill.fixture/util@1.0.0",
        "pkg:maven/dev.waybill.fixture/api@1.0.0",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let actual_purls: std::collections::HashSet<String> = jvm
        .iter()
        .filter_map(|c| c.get("purl").and_then(|v| v.as_str()).map(String::from))
        .collect();
    assert_eq!(actual_purls, expected_purls, "PURL set mismatch");

    // Every component: 1 sha256 hash + waybill:pants-resolve=default.
    for c in &jvm {
        let hashes = c
            .get("hashes")
            .and_then(|v| v.as_array())
            .expect("component has hashes[]");
        assert!(
            hashes.iter().any(|h| {
                h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256")
            }),
            "component missing SHA-256 hash: {:?}",
            c.get("purl"),
        );
        assert_eq!(
            get_property(c, "waybill:pants-resolve"),
            Some("default"),
            "component missing waybill:pants-resolve=default: {:?}",
            c.get("purl"),
        );
    }

    // ---- Dependency edge assertion: api → core ----
    // The CDX dependencies[] array uses BOM-refs; find the api-1.0.0
    // component's bom-ref, then verify its dependency list contains a
    // ref that resolves to the core-1.0.0 component.
    let api_ref = jvm
        .iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:maven/dev.waybill.fixture/api@1.0.0")
        })
        .and_then(|c| c.get("bom-ref").and_then(|v| v.as_str()))
        .expect("api component has bom-ref");
    let core_ref = jvm
        .iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:maven/dev.waybill.fixture/core@1.0.0")
        })
        .and_then(|c| c.get("bom-ref").and_then(|v| v.as_str()))
        .expect("core component has bom-ref");
    let deps = cdx
        .get("dependencies")
        .and_then(|v| v.as_array())
        .expect("cdx.dependencies[]");
    let api_dep_entry = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(api_ref))
        .expect("api has dependencies[] entry");
    let api_dependson: Vec<&str> = api_dep_entry
        .get("dependsOn")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        api_dependson.contains(&core_ref),
        "api → core edge missing. api.dependsOn = {api_dependson:?}, core_ref = {core_ref}",
    );

    // ---- SPDX 2.3 assertions ----
    let spdx = read_cdx(&spdx_path); // JSON parser works on SPDX-JSON too
    let packages = spdx
        .get("packages")
        .and_then(|v| v.as_array())
        .expect("spdx has packages[]");
    let jvm_pkgs: Vec<&serde_json::Value> = packages
        .iter()
        .filter(|p| {
            p.get("externalRefs")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .any(|r| {
                    r.get("referenceLocator")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.starts_with("pkg:maven/dev.waybill.fixture/"))
                })
        })
        .collect();
    assert_eq!(
        jvm_pkgs.len(),
        3,
        "expected 3 pants-jvm packages in SPDX, got {}",
        jvm_pkgs.len(),
    );
    for p in &jvm_pkgs {
        let checksums = p
            .get("checksums")
            .and_then(|v| v.as_array())
            .expect("spdx package has checksums[]");
        assert!(
            checksums.iter().any(|c| {
                c.get("algorithm").and_then(|v| v.as_str()) == Some("SHA256")
            }),
            "SPDX package missing SHA256 checksum",
        );
    }
}

// ---------------------------------------------------------------------
// US1 T013 — multi-resolve tags scope per JVM dev-tool allowlist
// ---------------------------------------------------------------------

#[test]
fn us1_multi_resolve_tags_scope_per_allowlist() {
    let fixture_dir = fixture("multi_resolve");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert_eq!(jvm.len(), 6, "expected 6 total pants-jvm components");

    for c in &jvm {
        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let scope = get_property(c, "waybill:lifecycle-scope");
        let resolve = get_property(c, "waybill:pants-resolve");
        if name.starts_with("runtime-") {
            // default resolve → Runtime (may be absent OR explicitly "runtime")
            assert!(
                scope.is_none() || scope == Some("runtime"),
                "runtime component has non-runtime scope: name={name} scope={scope:?}",
            );
            assert_eq!(
                resolve,
                Some("default"),
                "runtime component pants-resolve mismatch: name={name}",
            );
        } else if name.starts_with("testing-junit-") {
            assert_eq!(
                scope,
                Some("development"),
                "junit-resolve component not tagged development: name={name}",
            );
            assert_eq!(resolve, Some("junit"));
        } else if name.starts_with("testing-scala-") {
            assert_eq!(
                scope,
                Some("development"),
                "scalatest-resolve component not tagged development: name={name}",
            );
            assert_eq!(resolve, Some("scalatest"));
        } else {
            panic!("unexpected component name: {name}");
        }
    }
}

// ---------------------------------------------------------------------
// US1 T014 — classifier + packaging qualifiers emit correctly;
// waybill:source-url annotation emits iff coord.url present (C1 gate)
// ---------------------------------------------------------------------

#[test]
fn us1_classifier_and_packaging_qualifiers_emit_correctly() {
    let fixture_dir = fixture("with_classifier");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert_eq!(jvm.len(), 4, "expected 4 pants-jvm components");

    let by_name: std::collections::HashMap<&str, &serde_json::Value> = jvm
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(|n| (n, *c)))
        .collect();

    // Plain: no qualifiers, no source-url.
    let plain = by_name.get("plain").expect("plain component present");
    let plain_purl = plain.get("purl").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(plain_purl, "pkg:maven/dev.waybill.fixture/plain@1.0.0");
    assert_eq!(get_property(plain, "waybill:source-url"), None);

    // War: ?type=war only.
    let webapp = by_name.get("webapp").expect("webapp component present");
    let webapp_purl = webapp.get("purl").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        webapp_purl.contains("?type=war"),
        "webapp PURL missing ?type=war: {webapp_purl}",
    );
    assert!(
        !webapp_purl.contains("type=jar"),
        "webapp PURL should not have type=jar: {webapp_purl}",
    );

    // Classifier + so: PURL contains both classifier=linux-x86_64 AND
    // type=so, with correct qualifier separators (? then &).
    let native = by_name.get("native").expect("native component present");
    let native_purl = native.get("purl").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        native_purl.contains("classifier=linux-x86_64"),
        "native PURL missing classifier=linux-x86_64: {native_purl}",
    );
    assert!(
        native_purl.contains("type=so"),
        "native PURL missing type=so: {native_purl}",
    );

    // Internal-source: no qualifiers on PURL (url doesn't change shape),
    // but waybill:source-url property matches the fixture URL exactly.
    let internal = by_name
        .get("internal-source")
        .expect("internal-source component present");
    let internal_purl = internal.get("purl").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(
        internal_purl,
        "pkg:maven/dev.waybill.fixture/internal-source@1.0.0",
        "internal-source PURL should have no qualifiers",
    );
    assert_eq!(
        get_property(internal, "waybill:source-url"),
        Some("https://internal-mirror.example.test/dev/waybill/fixture/internal-source/1.0.0/internal-source-1.0.0.jar"),
        "internal-source component missing waybill:source-url",
    );
}

// ---------------------------------------------------------------------
// US1 T014a — FR-010 INFO log includes all 5 structured fields
// (with lockfiles_skipped_non_pants added vs m223)
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// US2 T020 — dedup vs pom.xml: same coord in coursier lockfile +
// pom.xml → exactly one component sourced from the lockfile
// (m191 reconciler handles it, this test is a regression guard)
// ---------------------------------------------------------------------

#[test]
fn us2_lockfile_dedups_against_pom_xml() {
    let fixture_dir = fixture("with_pom_xml");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let shared_components: Vec<&serde_json::Value> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|c| {
            c.get("purl").and_then(|v| v.as_str())
                == Some("pkg:maven/dev.waybill.fixture/shared@1.0.0")
        })
        .collect();
    assert_eq!(
        shared_components.len(),
        1,
        "expected exactly ONE pkg:maven/dev.waybill.fixture/shared@1.0.0 component after dedup; got {}",
        shared_components.len(),
    );
    let shared = shared_components[0];

    // The surviving component must carry the lockfile's sha256 hash.
    let hashes = shared
        .get("hashes")
        .and_then(|v| v.as_array())
        .expect("shared has hashes[]");
    assert!(
        hashes.iter().any(|h| {
            h.get("alg").and_then(|v| v.as_str()) == Some("SHA-256")
                && h.get("content").and_then(|v| v.as_str())
                    == Some("00000000000000000000000000000000000000000000000000000000000000ff")
        }),
        "shared component must carry the lockfile sha256 (proves lockfile-tier won dedup)",
    );

    // waybill:source-files must contain BOTH the lockfile path AND pom.xml.
    let source_files = get_property(shared, "waybill:source-files")
        .expect("shared has waybill:source-files property");
    assert!(
        source_files.contains("default.lock"),
        "waybill:source-files missing lockfile path: {source_files}",
    );
    assert!(
        source_files.contains("pom.xml"),
        "waybill:source-files missing pom.xml path: {source_files}",
    );
}

// ---------------------------------------------------------------------
// US3 T023 — pants.toml [jvm.resolves] custom path discovery
// ---------------------------------------------------------------------

#[test]
fn us3_pants_toml_custom_path_discovery() {
    let fixture_dir = fixture("pants_toml_custom_path");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert_eq!(
        jvm.len(),
        2,
        "expected 2 pants-jvm components from build-support/jvm/prod.lock; got {}",
        jvm.len(),
    );
    for c in &jvm {
        // Config-declared name "prod" wins over filename stem.
        assert_eq!(
            get_property(c, "waybill:pants-resolve"),
            Some("prod"),
            "component should carry waybill:pants-resolve=prod (config wins over stem): {:?}",
            c.get("purl"),
        );
    }

    // FR-010 log: lockfiles_discovered=1 from build-support/jvm/prod.lock.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("lockfiles_discovered=1"),
        "FR-010 log missing lockfiles_discovered=1. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// US3 T024 — missing pants.toml → default glob still works
// ---------------------------------------------------------------------

#[test]
fn us3_missing_pants_toml_falls_back_to_default_glob() {
    // Reuse US1's minimal_jvm fixture — has no pants.toml.
    let fixture_dir = fixture("minimal_jvm");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert_eq!(
        jvm.len(),
        3,
        "expected 3 pants-jvm components from default glob when no pants.toml present; got {}",
        jvm.len(),
    );
}

// ---------------------------------------------------------------------
// US3 T025 — malformed pants.toml falls back gracefully (FR-004)
// ---------------------------------------------------------------------

#[test]
fn us3_malformed_pants_toml_falls_back_gracefully() {
    let fixture_dir = fixture("malformed_pants_toml");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "FR-004: waybill must not abort on malformed pants.toml. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert_eq!(
        jvm.len(),
        1,
        "expected 1 pants-jvm component discovered via fallback default glob; got {}",
        jvm.len(),
    );

    // FR-004: WARN naming pants.toml.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("pants.toml"),
        "expected WARN mentioning pants.toml. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Phase 6 T028 — FR-011: non-Pants coursier lockfile skipped with INFO
// ---------------------------------------------------------------------

#[test]
fn fr011_non_pants_coursier_lockfile_skipped_with_info() {
    let fixture_dir = fixture("non_pants_coursier");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    // No pants_jvm-sourced components: the standalone coursier lockfile
    // must not have been ingested by our reader.
    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert!(
        jvm.is_empty(),
        "expected zero pants-jvm components from a non-Pants coursier lockfile; got {}: {:?}",
        jvm.len(),
        jvm.iter()
            .filter_map(|c| c.get("purl").and_then(|v| v.as_str()))
            .collect::<Vec<_>>(),
    );

    // FR-010 log carries lockfiles_skipped_non_pants=1.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("lockfiles_skipped_non_pants=1"),
        "expected lockfiles_skipped_non_pants=1 in FR-010 log. stderr:\n{stripped}",
    );
    assert!(
        stripped.contains("not a Pants-generated coursier lockfile"),
        "expected INFO log naming the skip reason. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Phase 6 T030 — SC-005: corrupt lockfile produces WARN + continues
// ---------------------------------------------------------------------

#[test]
fn corrupt_lockfile_produces_warn_and_continues() {
    let fixture_dir = fixture("corrupt_lockfile");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "SC-005: scan must not abort on corrupt lockfile. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert!(
        jvm.is_empty(),
        "expected zero pants-jvm components from a corrupt lockfile; got {}",
        jvm.len(),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        stripped.contains("lockfiles_skipped_corrupt=1"),
        "expected lockfiles_skipped_corrupt=1 in FR-010 log. stderr:\n{stripped}",
    );
    // WARN naming the corrupt file's path.
    assert!(
        stripped.contains("default.lock"),
        "expected WARN naming the corrupt file. stderr:\n{stripped}",
    );
}

// ---------------------------------------------------------------------
// Phase 6 T031 — FR-007 / SC-003: no lockfiles → no reader activity
// (byte-identity regression guard)
// ---------------------------------------------------------------------

#[test]
fn no_pants_jvm_no_lockfiles_produces_no_reader_activity() {
    // Reuse a totally non-JVM fixture from another test suite; the
    // pants_pex/minimal_python case has 3rdparty/python but no
    // 3rdparty/jvm/ tree, so pants_jvm::read returns early.
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pants_pex/minimal_python");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let cdx = read_cdx(&cdx_path);
    let jvm = pants_jvm_components(&cdx);
    assert!(
        jvm.is_empty(),
        "expected zero pants-jvm components on non-JVM fixture; got {}",
        jvm.len(),
    );

    // Reader must return early WITHOUT emitting the FR-010 summary
    // (byte-identity guarantee — nothing extra in the log stream).
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    assert!(
        !stripped.contains("pants-coursier-jvm reader complete"),
        "FR-007 / SC-003: reader must emit no log when no lockfiles present. stderr:\n{stripped}",
    );
}

#[test]
fn us1_fr010_info_log_emits_all_five_structured_fields() {
    let fixture_dir = fixture("minimal_jvm");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_path = tmp.path().join("out.cdx.json");
    let out = run_scan(&fixture_dir, &cdx_path, &[]);
    assert!(
        out.status.success(),
        "waybill nonzero. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stripped = strip_ansi(&stderr);
    for field in &[
        "lockfiles_discovered=",
        "lockfiles_parsed_ok=",
        "lockfiles_skipped_corrupt=",
        "lockfiles_skipped_non_pants=",
        "components_emitted=",
    ] {
        assert!(
            stripped.contains(field),
            "FR-010: stderr missing structured field {field}. stderr (ANSI-stripped):\n{stripped}",
        );
    }
}
