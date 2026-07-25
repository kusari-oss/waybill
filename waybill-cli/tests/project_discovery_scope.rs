//! Milestone 220 — integration tests for `--project-discovery=<mode>`.
//!
//! Covers US1 (root-only drops nested independent projects; C140 doc-
//! scope annotation; FR-012 INFO log), US2 (workspace-member preservation
//! under RootOnly; SC-003/SC-004), US3 (Strict drops workspace members;
//! SC-006), Polish (SC-007 invalid-mode error; SC-011 --split composition).
//!
//! Fixtures under `tests/fixtures/project_discovery/`:
//! - `polyglot_nested_independent/` — root Cargo.toml + nested npm +
//!   nested go.mod. Not workspace members.
//! - `cargo_workspace_with_independent_neighbor/` — root [workspace] +
//!   crates/{api,worker} declared members + bench/Gemfile independent.
//!
//! Follows the m219 split_modes.rs pattern verbatim: HOME isolation,
//! NO_COLOR=1 for tracing-subscriber log substring assertions,
//! combined stdout+stderr capture.

use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn fixture_polyglot_nested() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project_discovery/polyglot_nested_independent")
}

fn fixture_cargo_ws() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project_discovery/cargo_workspace_with_independent_neighbor")
}

fn fixture_gemfile_only() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gemfile_application")
}

fn waybill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

/// Invoke waybill against `fixture` with an optional
/// `--project-discovery=<mode>` value and return (output CDX bytes,
/// captured combined stderr+stdout for log assertions).
fn run_scan(fixture: &PathBuf, mode: Option<&str>) -> (Vec<u8>, String) {
    let out_dir = tempdir().expect("output tempdir");
    let home = tempdir().expect("home tempdir");
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(waybill_bin());
    cmd.env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env("WAYBILL_FIXTURES_DIR", env!("WAYBILL_FIXTURES_DIR"))
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    if let Some(m) = mode {
        cmd.arg(format!("--project-discovery={m}"));
    }
    let output = cmd.output().expect("waybill invokes");
    assert!(
        output.status.success(),
        "waybill failed (mode={:?}): stderr={}",
        mode,
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).expect("read emitted CDX");
    let mut combined = String::from_utf8_lossy(&output.stderr).to_string();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    (bytes, combined)
}

fn parse_cdx(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("emitted CDX parses as JSON")
}

fn component_purls(cdx: &serde_json::Value) -> Vec<String> {
    cdx["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn find_metadata_property<'a>(
    cdx: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    cdx["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
}

// ---------------- US1: shallow scan (SC-001, SC-002, SC-008, SC-009) ----------------

#[test]
fn us1_root_only_drops_nested_independent_projects() {
    let (bytes, _log) = run_scan(&fixture_polyglot_nested(), Some("root-only"));
    let cdx = parse_cdx(&bytes);
    let purls = component_purls(&cdx);
    assert!(
        !purls.iter().any(|p| p.starts_with("pkg:npm/")),
        "SC-001: no pkg:npm/* allowed under --project-discovery=root-only; got purls={purls:?}"
    );
    assert!(
        !purls.iter().any(|p| p.starts_with("pkg:golang/")),
        "SC-001: no pkg:golang/* allowed under --project-discovery=root-only; got purls={purls:?}"
    );
    // At least one cargo component must be present (root + serde).
    assert!(
        purls.iter().any(|p| p.starts_with("pkg:cargo/")),
        "expected pkg:cargo/* components to remain in-scope; got purls={purls:?}"
    );
}

#[test]
fn us1_all_mode_default_includes_all_ecosystems() {
    let (bytes, _log) = run_scan(&fixture_polyglot_nested(), None);
    let cdx = parse_cdx(&bytes);
    let purls = component_purls(&cdx);
    // SC-002 sanity: default mode covers all 3 ecosystems' components.
    let has_cargo = purls.iter().any(|p| p.starts_with("pkg:cargo/"));
    let has_npm = purls.iter().any(|p| p.starts_with("pkg:npm/"));
    let has_go = purls.iter().any(|p| p.starts_with("pkg:golang/"));
    assert!(
        has_cargo && has_npm && has_go,
        "default (--project-discovery=all) MUST include all 3 ecosystems' components; \
         cargo={has_cargo} npm={has_npm} go={has_go} purls={purls:?}"
    );
}

#[test]
fn us1_root_only_emits_c140_doc_scope_annotation() {
    let (bytes, _log) = run_scan(&fixture_polyglot_nested(), Some("root-only"));
    let cdx = parse_cdx(&bytes);
    let prop = find_metadata_property(&cdx, "waybill:project-discovery-mode")
        .expect("SC-008: C140 annotation MUST be present under --project-discovery=root-only");
    assert_eq!(
        prop["value"].as_str(),
        Some("root-only"),
        "SC-008: C140 value MUST be 'root-only'; got {prop:?}"
    );

    // And confirm the annotation is ABSENT under default (SC-005).
    let (bytes_default, _) = run_scan(&fixture_polyglot_nested(), None);
    let cdx_default = parse_cdx(&bytes_default);
    assert!(
        find_metadata_property(&cdx_default, "waybill:project-discovery-mode").is_none(),
        "SC-005: C140 annotation MUST be absent under default (`all`) mode"
    );
}

#[test]
fn us1_info_log_carries_mode_and_counts() {
    let (_bytes, log) = run_scan(&fixture_polyglot_nested(), Some("root-only"));
    assert!(
        log.contains("mode=root-only"),
        "FR-012: INFO log MUST contain `mode=root-only`. Log: {log}"
    );
    assert!(
        log.contains("project-discovery mode complete"),
        "FR-012: INFO log MUST contain `project-discovery mode complete`. Log: {log}"
    );
    // Fixture has 3 main-modules under All (cargo root + npm nested + go
    // nested). Under root-only: 1 root + 2 nested-ignored.
    assert!(
        log.contains("nested_projects_ignored=2"),
        "FR-012: INFO log MUST contain `nested_projects_ignored=2`. Log: {log}"
    );
}

// ---------------- US2: workspace-member preservation (SC-003, SC-004) ----------------

#[test]
fn us2_root_only_preserves_workspace_members() {
    let (bytes, _log) = run_scan(&fixture_cargo_ws(), Some("root-only"));
    let cdx = parse_cdx(&bytes);
    let purls = component_purls(&cdx);
    // Workspace root + both workspace members must be present.
    assert!(
        purls.iter().any(|p| p.contains("p220-api")),
        "SC-003: workspace member p220-api MUST be present; purls={purls:?}"
    );
    assert!(
        purls.iter().any(|p| p.contains("p220-worker")),
        "SC-003: workspace member p220-worker MUST be present; purls={purls:?}"
    );
    // SC-004: NO pkg:gem/* — the bench/Gemfile is an independent
    // nested project, not a workspace member.
    assert!(
        !purls.iter().any(|p| p.starts_with("pkg:gem/")),
        "SC-004: bench/Gemfile is not a workspace member — no pkg:gem/* allowed; purls={purls:?}"
    );
}

#[test]
fn us2_all_mode_includes_independent_gemfile() {
    let (bytes, _log) = run_scan(&fixture_cargo_ws(), None);
    let cdx = parse_cdx(&bytes);
    let purls = component_purls(&cdx);
    // SC-002 sanity for US2's fixture: default mode picks up both the
    // cargo workspace AND the independent Gemfile.
    let has_cargo = purls.iter().any(|p| p.starts_with("pkg:cargo/") || p.contains("p220-"));
    let has_gem = purls.iter().any(|p| p.starts_with("pkg:gem/"));
    assert!(
        has_cargo && has_gem,
        "default (--project-discovery=all) MUST include the workspace AND the Gemfile; \
         cargo={has_cargo} gem={has_gem} purls={purls:?}"
    );
}

// ---------------- US3: Strict mode (SC-006) ----------------

#[test]
fn us3_strict_drops_workspace_members() {
    let (bytes, _log) = run_scan(&fixture_cargo_ws(), Some("strict"));
    let cdx = parse_cdx(&bytes);
    let purls = component_purls(&cdx);
    // Workspace-member crates MUST be absent under Strict.
    assert!(
        !purls.iter().any(|p| p.contains("p220-api")),
        "SC-006: workspace member p220-api MUST be absent under --project-discovery=strict; purls={purls:?}"
    );
    assert!(
        !purls.iter().any(|p| p.contains("p220-worker")),
        "SC-006: workspace member p220-worker MUST be absent under --project-discovery=strict; purls={purls:?}"
    );
    // And still no gem components.
    assert!(
        !purls.iter().any(|p| p.starts_with("pkg:gem/")),
        "SC-006: independent Gemfile MUST also be absent under strict; purls={purls:?}"
    );
}

#[test]
fn us3_strict_c140_annotation_value_is_strict() {
    let (bytes, _log) = run_scan(&fixture_cargo_ws(), Some("strict"));
    let cdx = parse_cdx(&bytes);
    let prop = find_metadata_property(&cdx, "waybill:project-discovery-mode")
        .expect("C140 annotation MUST be present under --project-discovery=strict");
    assert_eq!(
        prop["value"].as_str(),
        Some("strict"),
        "C140 value MUST render as `strict` (Display kebab-case); got {prop:?}"
    );
}

#[test]
fn us3_gemfile_only_ruby_app_root_only_strict_produce_same_components() {
    // Ruby has no workspace concept — the difference between
    // `root-only` and `strict` collapses to nil on a Gemfile-only
    // scan. Component sets MUST be identical (modulo the mode value
    // in the C140 annotation).
    let (bytes_ro, _) = run_scan(&fixture_gemfile_only(), Some("root-only"));
    let (bytes_strict, _) = run_scan(&fixture_gemfile_only(), Some("strict"));
    let cdx_ro = parse_cdx(&bytes_ro);
    let cdx_strict = parse_cdx(&bytes_strict);
    let mut purls_ro = component_purls(&cdx_ro);
    let mut purls_strict = component_purls(&cdx_strict);
    purls_ro.sort();
    purls_strict.sort();
    assert_eq!(
        purls_ro, purls_strict,
        "Ruby-only scan: root-only and strict MUST produce identical component sets"
    );
}

// ---------------- Polish: SC-007 invalid mode + SC-011 --split composition ----------------

#[test]
fn invalid_mode_value_fails_cli_parse() {
    let home = tempdir().expect("home tempdir");
    let out_dir = tempdir().expect("out tempdir");
    let out_path = out_dir.path().join("out.cdx.json");
    let output = Command::new(waybill_bin())
        .env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env("WAYBILL_FIXTURES_DIR", env!("WAYBILL_FIXTURES_DIR"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_polyglot_nested())
        .arg("--project-discovery=nonexistent-mode")
        .arg("--output")
        .arg(&out_path)
        .output()
        .expect("waybill invokes");
    assert!(
        !output.status.success(),
        "SC-007: invalid --project-discovery value MUST exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in ["all", "root-only", "strict"] {
        assert!(
            stderr.contains(expected),
            "SC-007: stderr MUST list accepted value `{expected}`; got: {stderr}"
        );
    }
    assert!(
        !out_path.exists(),
        "SC-007: no output file MUST be created on CLI parse failure"
    );
}

#[test]
fn compose_with_split_directory_yields_single_sbom() {
    // On the polyglot fixture, root-only + split=directory yields
    // exactly 1 sub-SBOM (root's directory group; nested projects
    // dropped by the scope filter before split-directory sees them).
    let out_dir = tempdir().expect("out tempdir");
    let home = tempdir().expect("home tempdir");
    let status = Command::new(waybill_bin())
        .env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env("WAYBILL_FIXTURES_DIR", env!("WAYBILL_FIXTURES_DIR"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_polyglot_nested())
        .arg("--project-discovery=root-only")
        .arg("--split=directory")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output-dir")
        .arg(out_dir.path())
        .arg("--no-deep-hash")
        .status()
        .expect("waybill invokes");
    assert!(status.success(), "SC-011: waybill MUST succeed under --project-discovery=root-only + --split=directory");
    let count = std::fs::read_dir(out_dir.path())
        .expect("read out dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".cdx.json"))
        .count();
    assert_eq!(
        count, 1,
        "SC-011: root-only + split=directory on polyglot-nested fixture MUST yield 1 sub-SBOM; got {count}"
    );
}
