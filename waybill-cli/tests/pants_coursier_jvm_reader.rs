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

/// Issue #911 — `waybill:pants-resolve` carries a lexically sorted JSON array,
/// carried as JSON-in-string in CycloneDX (properties are spec'd as strings).
/// Decode it so assertions can keep naming a resolve directly.
///
/// Returns the sole resolve. Fixtures here are single-resolve; a plural value
/// means the fixture changed and the caller should know rather than silently
/// see the first element.
fn resolve_name(component: &serde_json::Value) -> Option<String> {
    let raw = get_property(component, "waybill:pants-resolve")?;
    let names: Vec<String> = serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| {
            Some(
                v.as_array()?
                    .iter()
                    .filter_map(|x| x.as_str())
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| vec![raw.to_string()]);
    match names.len() {
        1 => names.into_iter().next(),
        _ => panic!("expected exactly one resolve, got {names:?}"),
    }
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
            resolve_name(c).as_deref(),
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
        let resolve_owned = resolve_name(c);
        let resolve = resolve_owned.as_deref();
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
            resolve_name(c).as_deref(),
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

/// Two resolves lock different versions of one artifact chain. Each app's
/// edge must land on its own resolve's lib. Edges are keyed `group:artifact`,
/// which the resolve-scoped index did not carry for maven, so lookup fell
/// back to the flat index, where one version overwrites the other: one app
/// always pointed at the wrong lib, and which one depended on read order.
/// Found by the #925 corpus target's two-run reproducibility check.
#[test]
fn each_resolve_keeps_its_own_version_of_a_shared_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.cdx.json");
    let o = run_scan(&fixture("two_versions_two_resolves"), &out, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cdx = read_cdx(&out);
    let edges: std::collections::BTreeSet<(String, String)> = cdx["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| {
            let from = d["ref"].as_str().unwrap_or_default().to_string();
            d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str())
                .map(move |t| (from.clone(), t.to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|(f, _)| f.contains("dev.waybill.fixture/app@"))
        .collect();
    let want: std::collections::BTreeSet<(String, String)> = [
        ("pkg:maven/dev.waybill.fixture/app@1.0.0", "pkg:maven/dev.waybill.fixture/lib@1.0.0"),
        ("pkg:maven/dev.waybill.fixture/app@2.0.0", "pkg:maven/dev.waybill.fixture/lib@2.0.0"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(edges, want);
}


// ---------------------------------------------------------------
// Milestone 1064 (#924) — resolve ownership across namespaces.
// ---------------------------------------------------------------

/// The document-scope `waybill:resolve-ownership` value (C161), parsed.
fn ownership(cdx: &serde_json::Value) -> Option<serde_json::Value> {
    cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:resolve-ownership"))
        .and_then(|p| p["value"].as_str())
        .and_then(|v| serde_json::from_str(v).ok())
}

fn scan_fixture(name: &str) -> serde_json::Value {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.cdx.json");
    let o = run_scan(&fixture(name), &out, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    read_cdx(&out)
}

/// US3 — with no `[jvm.resolves]`, Pants's built-in default applies.
#[test]
fn unconfigured_jvm_default_is_named_jvm_default_and_declared() {
    let cdx = scan_fixture("implicit_default");
    for c in pants_jvm_components(&cdx) {
        assert_eq!(resolve_name(c).as_deref(), Some("jvm-default"), "{}", c["purl"]);
    }
    let o = ownership(&cdx).expect("a JVM Pants repository carries a statement");
    assert_eq!(o["declared"], serde_json::json!(["jvm:jvm-default"]));
    assert_eq!(o["discovered"], serde_json::json!([]));
}

/// US3 — a configured `[jvm.resolves]` turns the built-in default off, so a
/// stray `default.lock` is found by convention only.
#[test]
fn explicit_resolves_table_disables_the_builtin_default() {
    let cdx = scan_fixture("configured_plus_default");
    let o = ownership(&cdx).expect("statement present");
    assert_eq!(o["declared"], serde_json::json!(["jvm:main"]));
    assert_eq!(o["discovered"], serde_json::json!(["jvm:default"]));
    assert_eq!(o["unanchored_lockfiles"], serde_json::json!(1));
}


// --- US1: JVM owning components ---------------------------------------

/// Every component the scan marks as owning a resolve, as `purl`.
fn anchors(cdx: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = cdx["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| get_property(c, "waybill:component-kind") == Some("lockfile-resolve"))
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

/// The `dependsOn` targets of the component whose PURL is `purl`.
fn depends_of(cdx: &serde_json::Value, purl: &str) -> Vec<String> {
    let bom_ref = cdx["components"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["purl"].as_str() == Some(purl))
        .and_then(|c| c["bom-ref"].as_str())
        .unwrap_or_default()
        .to_string();
    let mut out: Vec<String> = cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"].as_str() == Some(bom_ref.as_str()))
        .flat_map(|d| d["dependsOn"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}

#[test]
fn jvm_declared_resolve_gets_an_anchor_wired_to_its_top_levels() {
    let cdx = scan_fixture("two_versions_two_resolves");
    assert_eq!(
        anchors(&cdx),
        vec![
            "pkg:generic/java17?pants-namespace=jvm".to_string(),
            "pkg:generic/java21?pants-namespace=jvm".to_string(),
        ]
    );
    for (resolve, app) in [("java17", "1.0.0"), ("java21", "2.0.0")] {
        assert_eq!(
            depends_of(&cdx, &format!("pkg:generic/{resolve}?pants-namespace=jvm")),
            vec![format!("pkg:maven/dev.waybill.fixture/app@{app}")],
            "{resolve} owns exactly its own declared top-level"
        );
    }
}

#[test]
fn jvm_only_repository_carries_an_ownership_statement() {
    let cdx = scan_fixture("two_versions_two_resolves");
    assert_eq!(
        ownership(&cdx),
        Some(serde_json::json!({
            "declared": ["jvm:java17", "jvm:java21"],
            "discovered": [],
            "unanchored_lockfiles": 0,
            "weak_classification": 2,
        }))
    );
}

#[test]
fn jvm_root_edges_agree_across_formats() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, a, g) = (
        tmp.path().join("o.cdx.json"),
        tmp.path().join("o.spdx.json"),
        tmp.path().join("o.spdx3.json"),
    );
    let out = Command::new(bin())
        .args(["--offline", "sbom", "scan", "--no-deep-hash", "--path"])
        .arg(fixture("two_versions_two_resolves"))
        .args(["--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", a.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", g.display()))
        .output()
        .expect("waybill invocation");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let counts = (
        cdx_root_out_edges(&read_cdx(&c)),
        spdx23_root_out_edges(&read_cdx(&a)),
        spdx3_root_out_edges(&read_cdx(&g)),
    );
    assert_eq!(counts.0, counts.1, "CycloneDX vs SPDX 2.3: {counts:?}");
    assert_eq!(counts.0, counts.2, "CycloneDX vs SPDX 3: {counts:?}");
}

#[test]
fn an_empty_top_level_list_still_gets_an_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("3rdparty/jvm")).unwrap();
    std::fs::write(root.join("pants.toml"), "[jvm.resolves]\nmain = \"3rdparty/jvm/main.lock\"\n").unwrap();
    let lock = std::fs::read_to_string(fixture("tool_lockfile/3rdparty/jvm/main.lock")).unwrap();
    let emptied = lock.replace(
        "#     \"dev.waybill.fixture:app:1.0.0,url=not_provided,jar=not_provided\"\n",
        "",
    );
    assert_ne!(lock, emptied, "the fixture's requirement line was removed");
    std::fs::write(root.join("3rdparty/jvm/main.lock"), emptied).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("o.cdx.json");
    let o = run_scan(root, &out, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cdx = read_cdx(&out);
    assert_eq!(anchors(&cdx), vec!["pkg:generic/main?pants-namespace=jvm".to_string()]);
    assert!(depends_of(&cdx, "pkg:generic/main?pants-namespace=jvm").is_empty());
    assert_eq!(ownership(&cdx).unwrap()["declared"], serde_json::json!(["jvm:main"]));
}

#[test]
fn top_level_missing_from_its_lockfile_is_dropped_not_rewired() {
    let cdx = scan_fixture("missing_top_level");
    let anchor = "pkg:generic/main?pants-namespace=jvm";
    assert_eq!(
        depends_of(&cdx, anchor),
        vec!["pkg:maven/dev.waybill.fixture/present@1.0.0".to_string()]
    );
    let unresolved = cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["purl"].as_str() == Some(anchor))
        .and_then(|c| get_property(c, "waybill:unresolved-declared-dep"))
        .unwrap_or_default()
        .to_string();
    assert!(
        unresolved.contains("dev.waybill.fixture:absent"),
        "the dropped name is reported, got {unresolved:?}"
    );
}

#[test]
fn implicit_default_jvm_resolve_is_anchored() {
    let cdx = scan_fixture("implicit_default");
    let anchor = "pkg:generic/jvm-default?pants-namespace=jvm";
    assert_eq!(anchors(&cdx), vec![anchor.to_string()]);
    assert_eq!(
        depends_of(&cdx, anchor),
        vec!["pkg:maven/dev.waybill.fixture/app@1.0.0".to_string()]
    );
}

// Root out-edge counters, as in corpus_harness_195::layer1_assertions.
fn cdx_root_out_edges(cdx: &serde_json::Value) -> usize {
    let root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap_or_default();
    cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["ref"].as_str() == Some(root))
        .map(|e| e["dependsOn"].as_array().map_or(0, |t| t.len()))
        .sum()
}

fn spdx23_root_out_edges(spdx: &serde_json::Value) -> usize {
    let roots: std::collections::HashSet<&str> = spdx["documentDescribes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect();
    const REVERSE: &[&str] = &[
        "DEV_DEPENDENCY_OF", "TEST_DEPENDENCY_OF", "BUILD_DEPENDENCY_OF",
        "OPTIONAL_DEPENDENCY_OF", "PROVIDED_DEPENDENCY_OF", "RUNTIME_DEPENDENCY_OF",
    ];
    spdx["relationships"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| match r["relationshipType"].as_str() {
            Some("DEPENDS_ON") => r["spdxElementId"].as_str().is_some_and(|f| roots.contains(f)),
            Some(t) if REVERSE.contains(&t) => {
                r["relatedSpdxElement"].as_str().is_some_and(|t| roots.contains(t))
            }
            _ => false,
        })
        .count()
}

fn spdx3_root_out_edges(spdx3: &serde_json::Value) -> usize {
    let graph = spdx3["@graph"].as_array().cloned().unwrap_or_default();
    let roots: std::collections::HashSet<String> = graph
        .iter()
        .filter(|n| n["type"].as_str() == Some("SpdxDocument"))
        .flat_map(|n| {
            ["rootElement", "software_rootElement"]
                .iter()
                .flat_map(|k| n[*k].as_array().cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    graph
        .iter()
        .filter(|n| {
            matches!(n["type"].as_str(), Some("Relationship") | Some("LifecycleScopedRelationship"))
                && n["relationshipType"].as_str() == Some("dependsOn")
                && n["from"].as_str().is_some_and(|f| roots.contains(f))
        })
        .map(|n| match &n["to"] {
            serde_json::Value::Array(a) => a.len(),
            serde_json::Value::String(_) => 1,
            _ => 0,
        })
        .sum()
}

// m1064 US4 — JVM tool lockfiles (`[<scope>].lockfile`, research R5)

/// `(pants-resolve, lifecycle-scope)` of every Maven package in `cdx`.
fn maven_membership_and_scope(cdx: &serde_json::Value) -> Vec<(Option<String>, Option<String>)> {
    cdx["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["purl"].as_str().is_some_and(|p| p.starts_with("pkg:maven/")))
        .map(|c| (resolve_name(c), get_property(c, "waybill:lifecycle-scope").map(str::to_string)))
        .collect()
}

fn property_of(cdx: &serde_json::Value, purl: &str, name: &str) -> Option<String> {
    cdx["components"]
        .as_array()?
        .iter()
        .find(|c| c["purl"].as_str() == Some(purl))
        .and_then(|c| get_property(c, name))
        .map(str::to_string)
}

#[test]
fn tool_lockfile_is_declared_development_scope() {
    let cdx = scan_fixture("tool_lockfile");
    assert_eq!(
        anchors(&cdx),
        vec![
            "pkg:generic/junit?pants-namespace=jvm".to_string(),
            "pkg:generic/main?pants-namespace=jvm".to_string(),
        ]
    );
    let junit = "pkg:generic/junit?pants-namespace=jvm";
    assert_eq!(
        property_of(&cdx, junit, "waybill:resolve-classification-source").as_deref(),
        Some("declared")
    );
    let testing: Vec<_> = maven_membership_and_scope(&cdx)
        .into_iter()
        .filter(|(r, _)| r.as_deref() == Some("junit"))
        .collect();
    assert!(!testing.is_empty(), "testing.lock's packages carry membership [\"junit\"]");
    for (_, scope) in &testing {
        assert_eq!(scope.as_deref(), Some("development"));
    }
    assert_eq!(
        ownership(&cdx),
        Some(serde_json::json!({
            "declared": ["jvm:junit", "jvm:main"],
            "discovered": [],
            "unanchored_lockfiles": 0,
            "weak_classification": 1,
        }))
    );
}

#[test]
fn configured_resolve_also_declared_by_a_tool_keeps_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("3rdparty/jvm")).unwrap();
    std::fs::write(
        root.join("pants.toml"),
        "[jvm.resolves]\ntests = \"3rdparty/jvm/tests.lock\"\n\n[junit]\nlockfile = \"3rdparty/jvm/tests.lock\"\n",
    )
    .unwrap();
    std::fs::copy(
        fixture("tool_lockfile/3rdparty/jvm/testing.lock"),
        root.join("3rdparty/jvm/tests.lock"),
    )
    .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("o.cdx.json");
    let o = run_scan(root, &out, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cdx = read_cdx(&out);
    let anchor = "pkg:generic/tests?pants-namespace=jvm";
    assert_eq!(anchors(&cdx), vec![anchor.to_string()]);
    assert_eq!(
        property_of(&cdx, anchor, "waybill:resolve-classification-source").as_deref(),
        Some("declared")
    );
    let packages = maven_membership_and_scope(&cdx);
    assert!(!packages.is_empty());
    for (resolve, scope) in packages {
        assert_eq!(resolve.as_deref(), Some("tests"));
        assert_eq!(scope.as_deref(), Some("development"));
    }
}

/// #1108 / #1022 — an owning component's root edge must not switch off the
/// root fallback in one format only. `waybill-fixture-unpinned` is a
/// design-tier requirement nothing else reaches, the shape of
/// `pants-example-django`'s requirements and `pants-example-jvm`'s file-tier
/// `get-pants.sh`. SPDX 3 used to count root -> `jvm-default` as "the root
/// already has edges" and drop it; CycloneDX and SPDX 2.3 never did (m894).
#[test]
fn an_unowned_component_reaches_the_root_in_every_format() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("3rdparty/jvm")).unwrap();
    for f in ["pants.toml", "3rdparty/jvm/default.lock"] {
        std::fs::copy(fixture(&format!("implicit_default/{f}")), root.join(f)).unwrap();
    }
    std::fs::write(root.join("requirements.txt"), "waybill-fixture-unpinned\n").unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let (c, a, g) = (
        tmp.path().join("o.cdx.json"),
        tmp.path().join("o.spdx.json"),
        tmp.path().join("o.spdx3.json"),
    );
    let out = Command::new(bin())
        .args(["--offline", "sbom", "scan", "--no-deep-hash", "--path"])
        .arg(root)
        .args(["--format", "cyclonedx-json", "--format", "spdx-2.3-json", "--format", "spdx-3-json"])
        .arg("--output")
        .arg(format!("cyclonedx-json={}", c.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", a.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", g.display()))
        .output()
        .expect("waybill invocation");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let cdx = read_cdx(&c);
    let root_ref = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string();
    assert_eq!(
        depends_of_ref(&cdx, &root_ref),
        vec![
            "pkg:generic/jvm-default?pants-namespace=jvm".to_string(),
            "pkg:pypi/waybill-fixture-unpinned".to_string(),
        ],
        "precondition: CycloneDX reaches the owning component and the unowned requirement"
    );
    let counts = (
        cdx_root_out_edges(&cdx),
        spdx23_root_out_edges(&read_cdx(&a)),
        spdx3_root_out_edges(&read_cdx(&g)),
    );
    assert_eq!(counts, (2, 2, 2), "root out-edges CycloneDX / SPDX 2.3 / SPDX 3");
}

fn depends_of_ref(cdx: &serde_json::Value, bom_ref: &str) -> Vec<String> {
    let mut out: Vec<String> = cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"].as_str() == Some(bom_ref))
        .flat_map(|d| d["dependsOn"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    out.sort();
    out
}
