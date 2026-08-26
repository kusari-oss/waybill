//! Milestone 665 US1 T012 — integration test for the
//! `--no-binary-scan=go` registration-skip contract.
//!
//! **What this test covers**
//!
//! - SC-005 (spec.md): setting `--no-binary-scan=go` eliminates every
//!   `pkg:golang/*` component derived from `go_binary::finalize`'s
//!   BuildInfo probe.
//! - Contract C1 (contracts/cli-flag.md): the registration gate at
//!   `run_shared_walker_pilot` prevents the reader from being
//!   registered when the mode is `Go`, so `finalize()` runs on an
//!   empty candidate list and emits zero entries.
//!
//! **Fixture**
//!
//! Reuses the existing m003 `go/binaries/` fixture (a Linux-x86_64
//! ELF binary with `runtime/debug.BuildInfo`) checked into the
//! sibling `kusari-oss/waybill-test-fixtures` repo. No new fixture
//! is required. This is the same binary
//! `scan_go_binary_emits_buildinfo_modules` in `scan_go.rs` asserts
//! against — reusing it guarantees the assertion in *this* file
//! flips iff the m665 gate is doing its job.
//!
//! Rationale: research.md R5 proposed adding
//! `no_binary_scan/gobin_with_buildinfo` to the sibling repo. The
//! existing `go/binaries/hello-linux-amd64` fixture is functionally
//! identical (BuildInfo-bearing Linux ELF) and already pinned via
//! `WAYBILL_FIXTURES_DIR`. T016 stays open only as an optional
//! nice-to-have for cross-Go-version coverage.

use std::process::Command;

// Milestone 665 T017 (contract T2): resolve fixture paths via the
// m090 `common::fixture_path()` helper, not a hardcoded path. Ensures
// cross-host determinism — the helper reads `WAYBILL_FIXTURES_DIR`
// set by build.rs (which points at the checked-out
// `kusari-oss/waybill-test-fixtures` clone).
mod common;

fn scan_with(path: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let mut cmd = Command::new(bin);
    // Mirrors `scan_go.rs::scan_path_args` so the delta between the
    // pre-m665 golden test and this m665 test is exactly the added
    // `--no-binary-scan=go` flag.
    cmd.env("WAYBILL_NO_GO_MOD_WHY", "1");
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn golang_purls(sbom: &serde_json::Value) -> Vec<String> {
    sbom["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str())
                .filter(|p| p.starts_with("pkg:golang/"))
                .map(|p| p.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// SC-005 anchor: scanning a Go-binary tree WITHOUT the flag emits
/// the BuildInfo modules; scanning WITH `--no-binary-scan=go`
/// eliminates them. Both runs use the same fixture and same CLI
/// surface so the delta is provably attributable to the flag.
#[test]
fn no_binary_scan_go_suppresses_buildinfo_derived_components() {
    let path = common::fixture_path("go/binaries");

    // Baseline: without the flag, the m003 fixture yields ≥3
    // pkg:golang components (main + cobra + logrus), same as
    // `scan_go_binary_emits_buildinfo_modules` in scan_go.rs.
    let baseline = scan_with(&path, &[]);
    let baseline_purls = golang_purls(&baseline);
    assert!(
        baseline_purls.len() >= 3,
        "baseline sanity check: expected ≥3 pkg:golang/* components \
         from the m003 binary fixture without the flag, got {}: {:?}",
        baseline_purls.len(),
        baseline_purls,
    );
    assert!(
        baseline_purls
            .iter()
            .any(|p| p.contains("github.com/spf13/cobra")),
        "baseline: expected cobra in {:?}",
        baseline_purls,
    );

    // With `--no-binary-scan=go`, every pkg:golang/* derived from
    // BuildInfo probing must be absent. The fixture has no go.mod,
    // so the go-source reader can't contribute any pkg:golang/*
    // components — the entire set drops to zero.
    let suppressed = scan_with(&path, &["--no-binary-scan=go"]);
    let suppressed_purls = golang_purls(&suppressed);
    assert_eq!(
        suppressed_purls.len(),
        0,
        "--no-binary-scan=go should suppress every pkg:golang/* \
         component derived from binary probing, but got {}: {:?}",
        suppressed_purls.len(),
        suppressed_purls,
    );
}
