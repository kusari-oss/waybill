//! Milestone 770 T022 — end-to-end smoke test.
//!
//! Environment-gated: this test fetches a repository over the network and
//! needs a release `waybill` plus `sbomqs`. The default
//! `cargo test --workspace` gate stays hermetic (Constitution VII), so run
//! it deliberately:
//!
//!     WAYBILL_QUALITY_E2E=1 cargo test -p xtask --test quality_smoke
#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .canonicalize()
        .unwrap()
}

#[test]
fn measures_one_small_target_end_to_end() {
    if std::env::var("WAYBILL_QUALITY_E2E").is_err() {
        eprintln!("skipping: set WAYBILL_QUALITY_E2E=1 to run the networked smoke test");
        return;
    }
    let root = workspace_root();
    let waybill = root.join("target/release/waybill");
    assert!(
        waybill.exists(),
        "build it first: cargo build --release -p waybill --bin waybill"
    );

    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus.toml");
    std::fs::write(
        &corpus,
        r#"
sbomqs_version = "v2.0.6"
[[targets]]
name = "go-cobra"
url = "https://github.com/spf13/cobra"
sha = "a655097faf7d54f78933a815984b9919d51a05d2"
ecosystem = "go"
"#,
    )
    .unwrap();
    let out = tmp.path().join("report.json");

    let status = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["quality", "--corpus"])
        .arg(&corpus)
        .arg("--output")
        .arg(&out)
        .arg("--cache-dir")
        .arg(tmp.path().join("cache"))
        .arg("--waybill-bin")
        .arg(&waybill)
        .status()
        .unwrap();
    assert!(status.success(), "unranged corpus must exit 0");

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 1);
    let m = &report["measurements"][0];
    assert_eq!(m["name"], "go-cobra");
    assert_eq!(m["status"], "measured");
    assert!(m["pkgs"].as_u64().unwrap() > 0, "cobra must yield packages");
    assert!(m["wall_ms"].as_u64().is_some());
    assert!(m["sbomqs"]["cyclonedx"].as_f64().unwrap() > 0.0);
    // The independent measurement and waybill's self-report are BOTH
    // recorded, as separate fields (FR-013).
    assert!(m["flat"].as_bool().is_some());
    assert!(m["graph_completeness"].as_str().is_some());
    assert!(report["violations"].as_array().unwrap().is_empty());
}
