//! Milestone 172 (T018): SC-001..SC-005 integration test — Go
//! step-5-fallback count doc-scope annotation
//! (`mikebom:go-transitive-fallback-count`) end-to-end via the release
//! binary.
//!
//! Three FR-006 scenarios are covered:
//!
//! * **Healthy Go scan** (fallback = 0): A synthesized `go.mod`-only
//!   Go project with no `require` entries and no `go.sum`. Nothing to
//!   resolve → 0 modules → 0 step-5 fallbacks. Annotation present
//!   with value `"0"` per Q1 clarification.
//!
//! * **Degraded Go scan** (fallback > 0): The vendored
//!   `simple-module` fixture in `--offline` mode. Offline mode is
//!   the deterministic path that forces every go.sum module onto
//!   step 5 (proxy fetches unavailable → GoSumFallback). Annotation
//!   present with value > 0.
//!
//! * **Non-Go scan**: A synthesized `Cargo.toml`-only Rust project.
//!   No Go components → no Go resolver run → annotation absent per
//!   FR-002.
//!
//! Plus the SC-005 count-sum invariant: for the degraded scan,
//! the doc-scope count MUST equal the number of components tagged
//! `mikebom:go-transitive-source == "go-sum-fallback"`.

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn go_fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("MIKEBOM_FIXTURES_DIR")).join("go").join(sub)
}

/// Scan a path with a fake `$HOME` so `$GOMODCACHE` points at an
/// empty tempdir — this is what makes offline mode deterministically
/// produce step-5 fallbacks (steps 2 and 3 find nothing). Without HOME
/// isolation, a populated developer `~/go/pkg/mod` would let step 2
/// resolve modules and drive `gosum_fallback_count` back to 0, matching
/// the m160 `cdx_regression` harness invariant.
fn scan(path: &Path, offline: bool) -> serde_json::Value {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let out_path = tmp.path().to_path_buf();
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("MIKEBOM_NO_GO_MOD_WHY", "1");
    if offline {
        cmd.arg("--offline");
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn doc_property<'a>(sbom: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    sbom["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)?["value"]
        .as_str()
}

fn component_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)?["value"]
        .as_str()
}

/// Write a minimal `go.mod`-only Go project into `dir`. No `require`
/// entries, no `go.sum` → nothing to resolve → 0 step-5 fallbacks.
fn write_empty_go_project(dir: &Path) {
    std::fs::write(
        dir.join("go.mod"),
        "module example.com/empty\n\ngo 1.21\n",
    )
    .expect("write go.mod");
}

/// Write a minimal `Cargo.toml`-only Rust project into `dir`. No Go
/// files → no Go resolver run → C117 absent per FR-002.
fn write_empty_rust_project(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).expect("mkdir src");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"empty-rust\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .expect("write Cargo.toml");
    std::fs::write(dir.join("src/lib.rs"), "").expect("write lib.rs");
}

/// SC-001 + SC-003: healthy Go scan emits C117 with value `"0"`.
#[test]
fn t018_healthy_go_scan_emits_zero() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_empty_go_project(tmp.path());

    let sbom = scan(tmp.path(), /* offline */ false);
    let c117 = doc_property(&sbom, "mikebom:go-transitive-fallback-count");
    assert_eq!(
        c117,
        Some("0"),
        "SC-003: healthy Go scan MUST emit C117 with value \"0\" \
         (Q1 emit-0-explicit rule); got {c117:?}"
    );
}

/// SC-001 + SC-004: degraded Go scan emits C117 with value > 0.
#[test]
fn t018_degraded_go_scan_emits_positive() {
    let sbom = scan(&go_fixture("simple-module"), /* offline */ true);
    let c117 = doc_property(&sbom, "mikebom:go-transitive-fallback-count");
    let value = c117.expect("SC-001: degraded scan MUST emit C117");
    let count: usize = value.parse().expect("C117 value must parse as integer");
    assert!(
        count > 0,
        "SC-004: offline (degraded) scan MUST emit C117 with value > 0; got \"{value}\""
    );
}

/// SC-002: non-Go scan omits C117 entirely.
#[test]
fn t018_non_go_scan_omits_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_empty_rust_project(tmp.path());

    let sbom = scan(tmp.path(), /* offline */ false);
    let c117 = doc_property(&sbom, "mikebom:go-transitive-fallback-count");
    assert_eq!(
        c117, None,
        "SC-002: non-Go scan MUST NOT emit C117 (annotation absent per FR-002); \
         got {c117:?}"
    );
}

/// SC-005: the doc-scope C117 count MUST equal the number of components
/// tagged `mikebom:go-transitive-source == "go-sum-fallback"`.
#[test]
fn t018_sc005_count_sum_invariant() {
    let sbom = scan(&go_fixture("simple-module"), /* offline */ true);
    let doc_count: usize = doc_property(&sbom, "mikebom:go-transitive-fallback-count")
        .expect("C117 must be present on degraded scan")
        .parse()
        .expect("C117 value must parse");

    let per_component_count = sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| component_property(c, "mikebom:go-transitive-source") == Some("go-sum-fallback"))
        .count();

    assert_eq!(
        doc_count, per_component_count,
        "SC-005 invariant violated: doc-scope C117 = {doc_count} but per-component \
         `mikebom:go-transitive-source == go-sum-fallback` count = {per_component_count}"
    );
}
