//! Milestone 771 integration tests — `go mod why` subprocess-spawn
//! scaling. Exercises US1 / US2 / US3 acceptance scenarios against
//! the synthetic 4-workspace fixture at
//! `waybill-cli/tests/fixtures/golang/mod_why_scaling/`.
//!
//! Design intent: these tests validate ORCHESTRATION shape (subprocess
//! count, log-line correlation, scope partitioning) without invoking
//! the real Go toolchain against the fixture. Use
//! `WAYBILL_GO_MOD_WHY_BUDGET_MS=1` to short-circuit real subprocess
//! work while the concurrent-worker code paths still execute — the
//! resulting `budget-exhausted` skip is expected in this test context
//! and does not invalidate the orchestration assertions.
//!
//! Empirical wall-time validation on Kubernetes (SC-001) lives in the
//! m669 benchmark harness, not here.

use std::path::PathBuf;

mod common;

/// Absolute path to the m771 synthetic fixture. Uses `CARGO_MANIFEST_DIR`
/// (waybill-cli crate root) — the fixture is crate-local under
/// `waybill-cli/tests/fixtures/golang/mod_why_scaling/`, matching the
/// convention used by `optional_dep_*` / `fingerprints_v2_schema` /
/// `cargo_workspace_root_lifecycle_m200` sibling tests.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golang/mod_why_scaling")
}

#[test]
fn fixture_is_readable() {
    // T004 — sanity guard: catches fixture-path breakage before other
    // tests attempt to scan it. If this fails, T003 didn't land or the
    // stay-set fixture dir was accidentally moved to the sibling repo.
    let root = fixture_path();
    assert!(
        root.is_dir(),
        "m771 fixture directory missing: {}",
        root.display(),
    );
    assert!(
        root.join("go.work").is_file(),
        "m771 fixture is missing go.work at {}",
        root.display(),
    );
    for member in ["mod-a", "mod-b", "mod-c", "loose"] {
        let gomod = root.join(member).join("go.mod");
        assert!(
            gomod.is_file(),
            "m771 fixture member {} is missing go.mod at {}",
            member,
            gomod.display(),
        );
    }
}
