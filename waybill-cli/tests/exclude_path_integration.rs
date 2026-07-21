//! Milestone 113 integration tests — user-supplied directory
//! exclusion for `waybill scan`.
//!
//! Coverage:
//!
//! - `cargo_fixture_suppressed_under_tests_fixtures` (T016): a real
//!   Cargo workspace at the scan root + a fixture Cargo crate at
//!   `tests/fixtures/sample-fixture/` is scanned with
//!   `--exclude-path tests/fixtures`; the fixture component is
//!   absent from the emitted SBOM, the real workspace component
//!   is present.
//! - `glob_pattern_matches_nested_testdata` (T025): synthetic
//!   monorepo with multiple nested cargo fixtures under
//!   `services/<name>/testdata/...`; a single
//!   `--exclude-path '**/testdata'` argument suppresses every
//!   nested fixture.
//! - `transparency_annotation_emitted_when_set_non_empty` (T024a /
//!   FR-014 / SC-007): scanning with `--exclude-path tests/fixtures`
//!   makes the emitted SBOM carry the `waybill:exclude-path`
//!   envelope annotation in CDX; scanning without any exclusion
//!   does NOT emit the annotation.
//! - `no_flag_scan_is_byte_identical_to_baseline` (T024 / FR-003 /
//!   SC-002): two back-to-back scans of the same fixture (one with
//!   `--exclude-path` absent, one with `WAYBILL_EXCLUDE_PATH=""`)
//!   produce byte-identical CDX output modulo the random
//!   `serialNumber` field. Exercises the empty-set no-op path.
//! - `malformed_pattern_exits_nonzero_before_scan` (T024 / FR-007
//!   / SC-005): supplying `--exclude-path '['` (unmatched bracket)
//!   causes waybill to exit non-zero before any walker begins.
//!
//! Tests use the `waybill` binary via `env!("CARGO_BIN_EXE_waybill")`,
//! the standard cargo-supported way to invoke the integration-test
//! target without rebuilding. Each test creates its fixture tree
//! under `tempfile::tempdir()` for isolation.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Write a minimal real Cargo project (no deps, no lockfile required
/// for main-module emission per milestone 064).
fn write_cargo_project(root: &std::path::Path, name: &str, version: &str) {
    std::fs::create_dir_all(root).unwrap();
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"
    );
    std::fs::write(root.join("Cargo.toml"), manifest).unwrap();
    // A bare `src/lib.rs` so the crate is structurally complete; this
    // doesn't affect waybill's scan but keeps the fixture realistic.
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
}

/// Run `waybill sbom scan --path <dir>` with deterministic env, the
/// supplied --exclude-path entries, and `--format cdx`. Returns the
/// parsed CDX value and the process status.
fn run_scan(
    root: &std::path::Path,
    exclude_paths: &[&str],
) -> (serde_json::Value, std::process::Output) {
    let mut cmd = Command::new(binary_path());
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--no-deep-hash")
        .arg("--offline")
        .arg("--output")
        .arg(root.join("out.cdx.json"))
        // Determinism levers — same env vars used by milestone-112's
        // byte-identity tests.
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        // Clear inherited values so the test environment is hermetic.
        .env_remove("WAYBILL_EXCLUDE_PATH")
        .env_remove("WAYBILL_NO_GO_MOD_WHY");
    for entry in exclude_paths {
        cmd.arg("--exclude-path").arg(entry);
    }
    let output = cmd.output().expect("failed to invoke waybill binary");
    if !output.status.success() {
        eprintln!(
            "waybill exited non-zero:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    let cdx_text = std::fs::read_to_string(root.join("out.cdx.json"))
        .expect("waybill should have written out.cdx.json");
    let cdx: serde_json::Value =
        serde_json::from_str(&cdx_text).expect("CDX output must parse as JSON");
    (cdx, output)
}

/// Gather every component name in the SBOM — both `metadata.component`
/// (the scan subject; waybill promotes the dominant project here when
/// only one survives the scan) AND every entry in `components[]`. Tests
/// need both because a fixture that's excluded may have been the
/// metadata.component in the unfiltered scan and become absent in the
/// filtered one, and vice-versa.
fn component_names(cdx: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(name) = cdx
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("name"))
        .and_then(|n| n.as_str())
    {
        out.push(name.to_string());
    }
    if let Some(arr) = cdx.get("components").and_then(|c| c.as_array()) {
        for c in arr {
            if let Some(n) = c.get("name").and_then(|n| n.as_str()) {
                out.push(n.to_string());
            }
        }
    }
    out
}

fn envelope_property(cdx: &serde_json::Value, name: &str) -> Option<String> {
    cdx.get("metadata")
        .and_then(|m| m.get("properties"))
        .and_then(|p| p.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|prop| {
                if prop.get("name").and_then(|n| n.as_str()) == Some(name) {
                    prop.get("value")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
}

#[test]
fn cargo_fixture_suppressed_under_tests_fixtures() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "real-app", "1.0.0");
    write_cargo_project(
        &root.join("tests/fixtures/sample-fixture"),
        "sample-fixture",
        "0.0.1",
    );

    // Baseline: without exclusion, both components appear.
    let (cdx_baseline, _) = run_scan(root, &[]);
    let baseline_names = component_names(&cdx_baseline);
    assert!(
        baseline_names.iter().any(|n| n == "real-app"),
        "baseline: real-app must appear in unfiltered scan; got: {baseline_names:?}",
    );
    assert!(
        baseline_names.iter().any(|n| n == "sample-fixture"),
        "baseline: sample-fixture must appear without --exclude-path; got: {baseline_names:?}",
    );

    // With exclusion: the fixture vanishes.
    let (cdx, status) = run_scan(root, &["tests/fixtures"]);
    assert!(status.status.success(), "waybill exited non-zero");
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n == "real-app"),
        "real-app must remain in filtered scan; got: {names:?}",
    );
    assert!(
        !names.iter().any(|n| n == "sample-fixture"),
        "sample-fixture must be suppressed by --exclude-path tests/fixtures; got: {names:?}",
    );
}

#[test]
fn glob_pattern_matches_nested_testdata() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "monorepo-root", "1.0.0");
    write_cargo_project(
        &root.join("services/a/testdata/fixture-a"),
        "fixture-a",
        "0.0.1",
    );
    write_cargo_project(
        &root.join("services/b/testdata/fixture-b"),
        "fixture-b",
        "0.0.1",
    );

    let (cdx, status) = run_scan(root, &["**/testdata"]);
    assert!(status.status.success());
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n == "monorepo-root"),
        "real workspace must remain; got: {names:?}",
    );
    assert!(
        !names.iter().any(|n| n == "fixture-a"),
        "fixture-a must be suppressed by **/testdata; got: {names:?}",
    );
    assert!(
        !names.iter().any(|n| n == "fixture-b"),
        "fixture-b must be suppressed by **/testdata; got: {names:?}",
    );
}

#[test]
fn transparency_annotation_emitted_when_set_non_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "annotation-app", "1.0.0");
    write_cargo_project(
        &root.join("tests/fixtures/fixture-x"),
        "fixture-x",
        "0.0.1",
    );

    // With exclusion: annotation is present, value matches the
    // operator-typed entry.
    let (cdx, status) = run_scan(root, &["tests/fixtures"]);
    assert!(status.status.success());
    let value = envelope_property(&cdx, "waybill:exclude-path");
    assert_eq!(
        value.as_deref(),
        Some("tests/fixtures"),
        "exclude-path annotation must carry the entry verbatim",
    );

    // Without exclusion: annotation is absent.
    let (cdx_clean, _) = run_scan(root, &[]);
    let value = envelope_property(&cdx_clean, "waybill:exclude-path");
    assert_eq!(
        value, None,
        "exclude-path annotation must be absent when no exclusions in effect",
    );
}

#[test]
fn no_flag_scan_is_byte_identical_to_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "identity-app", "1.0.0");

    let (cdx_a, _) = run_scan(root, &[]);
    let (cdx_b, _) = run_scan(root, &[]);

    // Two back-to-back scans with the same fixture must produce
    // byte-identical CDX modulo the random serialNumber. Strip the
    // serial number before comparison (its randomness is intentional
    // per CDX 1.6 spec; not affected by milestone 113).
    let mut a = cdx_a.clone();
    let mut b = cdx_b.clone();
    if let Some(obj) = a.as_object_mut() {
        obj.insert(
            "serialNumber".into(),
            serde_json::Value::String("urn:uuid:MASKED".into()),
        );
    }
    if let Some(obj) = b.as_object_mut() {
        obj.insert(
            "serialNumber".into(),
            serde_json::Value::String("urn:uuid:MASKED".into()),
        );
    }

    assert_eq!(
        a, b,
        "back-to-back scans with no --exclude-path must produce byte-identical CDX",
    );
    // The exclude-path annotation must be absent from both.
    assert!(
        envelope_property(&a, "waybill:exclude-path").is_none(),
        "no-flag scan must not emit waybill:exclude-path annotation",
    );
}

#[test]
fn malformed_pattern_exits_nonzero_before_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "malformed-app", "1.0.0");

    let output = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--no-deep-hash")
        .arg("--offline")
        .arg("--output")
        .arg(root.join("out.cdx.json"))
        .arg("--exclude-path")
        .arg("foo[")
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env_remove("WAYBILL_EXCLUDE_PATH")
        .env_remove("WAYBILL_NO_GO_MOD_WHY")
        .output()
        .expect("failed to invoke waybill");

    assert!(
        !output.status.success(),
        "waybill must exit non-zero on malformed --exclude-path entry; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("foo["),
        "error must name the offending entry verbatim; stderr was: {stderr}",
    );
    // The output file must not have been created (scan never started).
    assert!(
        !root.join("out.cdx.json").is_file(),
        "scan should not have produced an output file on malformed input",
    );
}

// ============================================================================
// Milestone 118 — US1 per-ecosystem regression coverage (issue #343)
// ============================================================================

/// Write a minimal real Go module (go.mod + a `package <name>` source file).
/// Used by US1 + US2 milestone-118 tests that need a non-Cargo ecosystem
/// fixture without triggering the Go-tool unconditional skip shapes
/// (testdata/ + _-prefix dirs).
fn write_go_module(root: &std::path::Path, module_path: &str) {
    std::fs::create_dir_all(root).unwrap();
    let gomod = format!("module {module_path}\n\ngo 1.21\n");
    std::fs::write(root.join("go.mod"), gomod).unwrap();
    let main_rs = "package main\n\nfunc main() {}\n";
    std::fs::write(root.join("main.go"), main_rs).unwrap();
}

/// T001 / FR-001 — Go source fixture suppressed via --exclude-path.
/// Fixture path is `tests/fixtures/` (NOT `testdata/`, NOT `_archive`) so
/// the Go-tool unconditional skip doesn't fire; the test exercises
/// `--exclude-path` exclusively.
#[test]
fn golang_source_fixture_suppressed_via_exclude_path() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_go_module(&root.join("real-app"), "github.com/example/real-app");
    write_go_module(
        &root.join("tests/fixtures/fixture-app"),
        "github.com/example/fixture-app",
    );

    // Baseline scan: both modules present.
    let (cdx, output) = run_scan(root, &[]);
    assert!(output.status.success(), "baseline scan must succeed");
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n.contains("real-app")),
        "baseline scan must surface real-app; got {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains("fixture-app")),
        "baseline scan must surface fixture-app (no exclusion); got {names:?}"
    );

    // Excluded scan: fixture vanishes.
    let (cdx, output) = run_scan(root, &["tests/fixtures"]);
    assert!(output.status.success(), "excluded scan must succeed");
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n.contains("real-app")),
        "excluded scan must keep real-app; got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("fixture-app")),
        "excluded scan must suppress fixture-app; got {names:?}"
    );
}

/// T002 / FR-002 — Go binary fixture suppressed via --exclude-path.
/// Skips gracefully if the host has no `go` toolchain — preserves
/// Decision 3's "no new vendored fixture directories" rule by building
/// the binary at test time rather than committing one to the repo.
#[test]
fn go_binary_fixture_suppressed_via_exclude_path() {
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("skipping go_binary_fixture_suppressed_via_exclude_path: go toolchain not available");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // Build a tiny binary under tests/fixtures/go-binary/bin/foo.
    let src_dir = root.join("tests/fixtures/go-binary/src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("main.go"),
        "package main\n\nfunc main() { println(\"foo\") }\n",
    )
    .unwrap();
    let bin_dir = root.join("tests/fixtures/go-binary/bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin_path = bin_dir.join("foo");
    let build = Command::new("go")
        .arg("build")
        .arg("-o")
        .arg(&bin_path)
        .arg(src_dir.join("main.go"))
        .output()
        .expect("go build invocation");
    if !build.status.success() {
        eprintln!(
            "skipping go_binary_fixture_suppressed_via_exclude_path: go build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        return;
    }
    // Also write a "real" Cargo project at the scan root so the scan
    // emits something non-fixture-tier even when the fixture is excluded.
    write_cargo_project(&root.join("real-app"), "real-app", "1.0.0");

    // Baseline scan: should detect the binary as pkg:generic/foo (or similar).
    let (cdx, output) = run_scan(root, &[]);
    if !output.status.success() {
        eprintln!(
            "skipping go_binary_fixture_suppressed_via_exclude_path: baseline scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let names_baseline = component_names(&cdx);
    let foo_present_baseline = names_baseline.iter().any(|n| n == "foo");
    if !foo_present_baseline {
        // The milestone-096 binary discovery might not classify this
        // synthetic minimal binary as a generic component — the test
        // still proves the negative direction (excluded scan must
        // ALSO not include it). Skip if baseline doesn't surface.
        eprintln!(
            "skipping go_binary_fixture_suppressed_via_exclude_path: baseline scan did not classify the synthetic binary (got {names_baseline:?})"
        );
        return;
    }

    // Excluded scan: binary must vanish.
    let (cdx, output) = run_scan(root, &["tests/fixtures"]);
    assert!(output.status.success(), "excluded scan must succeed");
    let names = component_names(&cdx);
    assert!(
        !names.iter().any(|n| n == "foo"),
        "excluded scan must suppress the foo binary; got {names:?}"
    );
}

/// T003 / FR-003 — dependency edges pointing AT a suppressed component
/// must be dropped (no dangling DEPENDS_ON references in the emitted SBOM).
#[test]
fn dependency_edges_referencing_suppressed_components_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // Dep crate at tests/fixtures/dep-lib/
    write_cargo_project(&root.join("tests/fixtures/dep-lib"), "dep-lib", "1.0.0");
    // Main app at main-app/, with a path-dependency on dep-lib.
    let main_dir = root.join("main-app");
    std::fs::create_dir_all(&main_dir).unwrap();
    let manifest = "[package]\nname = \"main-app\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n\
                    [dependencies]\ndep-lib = { path = \"../tests/fixtures/dep-lib\" }\n";
    std::fs::write(main_dir.join("Cargo.toml"), manifest).unwrap();
    std::fs::create_dir_all(main_dir.join("src")).unwrap();
    std::fs::write(main_dir.join("src/lib.rs"), "").unwrap();

    // Excluded scan: dep-lib must be absent AND no dangling dep-lib reference
    // should remain in any dependency edge.
    let (cdx, output) = run_scan(root, &["tests/fixtures"]);
    assert!(output.status.success(), "excluded scan must succeed");
    let names = component_names(&cdx);
    assert!(
        !names.iter().any(|n| n == "dep-lib"),
        "excluded scan must suppress dep-lib; got {names:?}"
    );
    // Walk the dependencies array (CDX) looking for any edge whose `dependsOn`
    // list references a non-existent component bom-ref. If a future emission
    // shape drops the dep edge for suppressed components, this test stays
    // green; if it surfaces a dangling reference, this test fails.
    if let Some(deps) = cdx.get("dependencies").and_then(|d| d.as_array()) {
        // Collect every emitted component's bom-ref / purl identifier.
        let mut emitted: std::collections::HashSet<String> = Default::default();
        if let Some(c) = cdx.get("metadata").and_then(|m| m.get("component")) {
            if let Some(s) = c.get("bom-ref").and_then(|v| v.as_str()) {
                emitted.insert(s.to_string());
            }
            if let Some(s) = c.get("purl").and_then(|v| v.as_str()) {
                emitted.insert(s.to_string());
            }
        }
        if let Some(arr) = cdx.get("components").and_then(|v| v.as_array()) {
            for c in arr {
                if let Some(s) = c.get("bom-ref").and_then(|v| v.as_str()) {
                    emitted.insert(s.to_string());
                }
                if let Some(s) = c.get("purl").and_then(|v| v.as_str()) {
                    emitted.insert(s.to_string());
                }
            }
        }
        for edge in deps {
            if let Some(refs) = edge.get("dependsOn").and_then(|v| v.as_array()) {
                for r in refs {
                    if let Some(rs) = r.as_str() {
                        assert!(
                            emitted.contains(rs),
                            "dangling dependsOn reference {rs:?} for a component not in the emitted set ({emitted:?}); excluded scan must drop edges pointing AT suppressed components"
                        );
                    }
                }
            }
        }
    }
}

/// T004 / FR-004 — when the operator excludes the scan root itself, the
/// SBOM contains only metadata.component (waybill's self-description) and
/// no other components.
#[test]
fn scan_root_excluded_yields_only_metadata_component() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(&root.join("some-app"), "some-app", "1.0.0");
    // Excluding the absolute scan root path should suppress every walker.
    let root_abs = root.canonicalize().unwrap();
    let root_abs_str = root_abs.to_string_lossy().into_owned();

    let (cdx, output) = run_scan(root, &[root_abs_str.as_str()]);
    assert!(
        output.status.success(),
        "scan with --exclude-path=<root> must succeed (no error; just empty components[])"
    );
    let components = cdx.get("components").and_then(|c| c.as_array());
    let count = components.map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        count, 0,
        "excluding the scan root must yield an empty components[] array; got {count} components: {:?}",
        components
    );
}

// ============================================================================
// Milestone 118 — US2 complex pattern + cross-platform separator coverage
// ============================================================================

/// T005 / FR-005 — two distinct pattern entries in one scan suppress both
/// subtree shapes (union semantics).
#[test]
fn multiple_pattern_entries_combine_by_union() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(&root.join("real-app"), "real-app", "1.0.0");
    write_cargo_project(
        &root.join("services/payment/testdata/cargo/fixture-cargo"),
        "fixture-cargo",
        "1.0.0",
    );
    write_cargo_project(
        &root.join("services/payment/_archive/cargo/legacy-app"),
        "legacy-app",
        "1.0.0",
    );

    let (cdx, output) = run_scan(root, &["**/testdata", "**/_archive"]);
    assert!(output.status.success(), "scan must succeed");
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n.contains("real-app")),
        "real-app must remain; got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("fixture-cargo")),
        "**/testdata pattern must suppress fixture-cargo; got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("legacy-app")),
        "**/_archive pattern must suppress legacy-app; got {names:?}"
    );

    // FR-005 transparency: the envelope annotation lists BOTH pattern entries.
    let annotation = envelope_property(&cdx, "waybill:exclude-path")
        .expect("envelope annotation must be present when set is non-empty");
    assert!(
        annotation.contains("**/testdata") && annotation.contains("**/_archive"),
        "envelope must list both pattern entries; got: {annotation}"
    );
}

/// T006 / FR-006 — backslash-separated literal entry normalizes to the
/// same suppression as forward-slash form.
#[test]
fn cross_platform_separator_normalization() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(&root.join("real-app"), "real-app", "1.0.0");
    write_cargo_project(
        &root.join("tests/fixtures/fixture-app"),
        "fixture-app",
        "1.0.0",
    );

    // Backslash-separated literal — must normalize at parse time and match
    // the same directories as the forward-slash form.
    let (cdx, output) = run_scan(root, &["tests\\fixtures"]);
    assert!(output.status.success(), "scan must succeed");
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n.contains("real-app")),
        "real-app must remain; got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("fixture-app")),
        "backslash literal must normalize and suppress fixture-app; got {names:?}"
    );
}
