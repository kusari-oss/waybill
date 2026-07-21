//! Milestone 207 (#596) — `--no-deps-dev` aggregate-disable integration tests.
//!
//! All tests are network-free (use `--offline` + existing non-image
//! fixtures). Behavioral coverage of the semantic change lives in
//! `scan_cmd::tests` unit tests (`resolve_enrich_*_m207`). These
//! integration tests pin FR-006 migration-log presence/absence.
//!
//! SC-001 content verification (reporter's exact invocation → zero
//! deps.dev-provenance components in emitted SBOM) is a network-
//! required check documented in the PR body as a manual reproducer
//! per plan.md quickstart.md Reproducer 1.

use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn npm_express_fixture() -> String {
    format!(
        "{}/tests/fixtures/public_corpus/npm-express",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn scan(extra_args: &[&str]) -> (bool, String) {
    let tempdir = tempfile::tempdir().unwrap();
    let out = tempdir.path().join("out.cdx.json");
    let mut cmd = Command::new(mikebom_bin());
    cmd.args([
        "sbom",
        "scan",
        "--offline",
        "--path",
        &npm_express_fixture(),
        "--format",
        "cyclonedx-json",
        "--output",
        out.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("spawn mikebom");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stderr)
}

// ── T010 US1 — aggregate flag scan succeeds + FR-006 log fires ──

#[test]
fn us1_no_deps_dev_scan_succeeds_and_fires_migration_log_m207() {
    let (ok, stderr) = scan(&["--no-deps-dev"]);
    assert!(ok, "FR-007 no new failure modes: scan should exit 0. stderr:\n{stderr}");
    assert!(
        stderr.contains("m207 aggregate semantic"),
        "FR-006: --no-deps-dev alone MUST fire the migration INFO log. stderr:\n{stderr}"
    );
}

// ── T011 US1 — log fires-alone (positive isolation) ──

#[test]
fn fr006_migration_info_log_fires_when_aggregate_flag_used_alone_m207() {
    let (ok, stderr) = scan(&["--no-deps-dev"]);
    assert!(ok);
    assert!(
        stderr.contains("m207 aggregate semantic"),
        "FR-006: migration log MUST fire on --no-deps-dev alone. stderr:\n{stderr}"
    );
}

// ── T012 US1 — log suppressed with fine-grained escape hatch ──

#[test]
fn fr006_migration_info_log_suppressed_when_fine_grained_flag_also_set_m207() {
    let (ok, stderr) = scan(&["--no-deps-dev", "--no-deps-dev-license"]);
    assert!(ok);
    assert!(
        !stderr.contains("m207 aggregate semantic"),
        "FR-006: migration log MUST NOT fire when a fine-grained flag is \
         also set (operator is already aware of the fine-grained semantic). stderr:\n{stderr}"
    );
}

// ── T013 US2 — no-deps-dev-license alone does not fire aggregate log ──

#[test]
fn us2_no_deps_dev_license_alone_does_not_fire_aggregate_migration_log_m207() {
    let (ok, stderr) = scan(&["--no-deps-dev-license"]);
    assert!(ok);
    assert!(
        !stderr.contains("m207 aggregate semantic"),
        "FR-006: --no-deps-dev-license alone MUST NOT fire the aggregate migration log \
         (only fires for --no-deps-dev without escape hatch). stderr:\n{stderr}"
    );
}
