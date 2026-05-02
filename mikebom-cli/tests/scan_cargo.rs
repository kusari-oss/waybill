//! Integration tests for the Cargo ecosystem (milestone 003 US4).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tests/fixtures/cargo")
        .join(sub)
}

fn run_scan(path: &Path) -> Output {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(tmp.path())
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run")
}

fn run_scan_with_output(path: &Path) -> (Output, tempfile::TempDir, PathBuf) {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run");
    (output, tmp, out_path)
}

fn cargo_purls(sbom_path: &Path) -> Vec<String> {
    let raw = std::fs::read_to_string(sbom_path).expect("read sbom");
    let sbom: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    sbom["components"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|c| {
            let p = c["purl"].as_str()?;
            if p.starts_with("pkg:cargo/") {
                Some(p.to_string())
            } else {
                None
            }
        })
        .collect()
}

// --- T069: v3 + v4 conformant SBOMs -----------------------------------

#[test]
fn scan_cargo_v3_fixture_emits_conformant_sbom() {
    let (output, _tmp, sbom_path) = run_scan_with_output(&fixture("lockfile-v3"));
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let purls = cargo_purls(&sbom_path);
    assert!(
        purls.len() >= 6,
        "expected ≥6 cargo components from v3 fixture, got {}: {purls:?}",
        purls.len(),
    );
    // Registry crate must be present.
    assert!(purls.iter().any(|p| p.starts_with("pkg:cargo/serde@")));
    // Git-sourced crate must be present.
    assert!(purls.iter().any(|p| p.starts_with("pkg:cargo/my-fork@")));
}

#[test]
fn scan_cargo_v4_fixture_emits_conformant_sbom() {
    let (output, _tmp, sbom_path) = run_scan_with_output(&fixture("lockfile-v4"));
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let purls = cargo_purls(&sbom_path);
    assert!(
        purls.iter().any(|p| p.starts_with("pkg:cargo/anyhow@")),
        "anyhow missing from v4 scan: {purls:?}",
    );
}

#[test]
fn scan_cargo_v3_git_source_carries_source_type_property() {
    let (_output, _tmp, sbom_path) = run_scan_with_output(&fixture("lockfile-v3"));
    let raw = std::fs::read_to_string(&sbom_path).expect("read sbom");
    let sbom: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let my_fork = sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:cargo/my-fork@"))
        })
        .expect("my-fork component present");
    let props = my_fork["properties"]
        .as_array()
        .expect("properties array");
    let source_type = props
        .iter()
        .find(|p| p["name"].as_str() == Some("mikebom:source-type"))
        .expect("mikebom:source-type property present")
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(source_type, "git");
}

// --- T070: v1 / v2 refusal --------------------------------------------

#[test]
fn scan_cargo_v1_lockfile_refuses_with_actionable_error() {
    let output = run_scan(&fixture("lockfile-v1-refused"));
    assert!(
        !output.status.success(),
        "v1 lockfile scan should exit non-zero, got status {}",
        output.status,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cargo.lock v1/v2 not supported"),
        "stderr missing refusal message: {stderr}",
    );
    assert!(
        stderr.contains("cargo ≥1.53"),
        "stderr missing remediation hint: {stderr}",
    );
}

#[test]
fn scan_cargo_v2_lockfile_refuses_with_actionable_error() {
    let output = run_scan(&fixture("lockfile-v2-refused"));
    assert!(
        !output.status.success(),
        "v2 lockfile scan should exit non-zero, got status {}",
        output.status,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cargo.lock v1/v2 not supported"),
        "stderr missing refusal message: {stderr}",
    );
}

// ---------------------------------------------------------------------------
// Milestone 051 — cargo dev/build dep tagging
// ---------------------------------------------------------------------------

/// Run mikebom against `path` with optional extra args (e.g.
/// `--include-dev`). Returns the parsed SBOM JSON.
fn run_scan_args(path: &Path, extra_args: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let output = cmd.output().expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn cargo_component_named<'a>(sbom: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
}

fn has_dev_property(component: &serde_json::Value) -> bool {
    // Milestone 052/part-2: native CDX `scope: "excluded"` is the
    // primary signal; the new `mikebom:lifecycle-scope` property
    // carries the finer dev/build/test distinction. Either signal
    // proves the component is non-Runtime-scoped.
    if component["scope"].as_str() == Some("excluded") {
        return true;
    }
    component["properties"]
        .as_array()
        .map(|props| {
            props.iter().any(|p| {
                p["name"].as_str() == Some("mikebom:lifecycle-scope")
            })
        })
        .unwrap_or(false)
}

#[test]
fn scan_cargo_dev_dependency_is_tagged_and_droppable() {
    // FR-001 / FR-002: a crate declared in [dev-dependencies] is
    // dropped from the default-mode SBOM and tagged with
    // mikebom:dev-dependency = true under --include-dev.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1.0.197"

[dev-dependencies]
criterion = "0.5.1"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Cargo.lock"),
        r#"
version = 3

[[package]]
name = "demo"
version = "0.1.0"
dependencies = ["serde"]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c753"

[[package]]
name = "criterion"
version = "0.5.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "f2b12d09"
"#,
    )
    .unwrap();

    // Default scan: criterion absent.
    let sbom = run_scan_args(dir.path(), &[]);
    assert!(
        cargo_component_named(&sbom, "criterion").is_none(),
        "criterion (dev-dep) must be dropped in default mode",
    );
    assert!(
        cargo_component_named(&sbom, "serde").is_some(),
        "serde (prod-dep) must be retained",
    );

    // --include-dev: criterion present + tagged.
    let sbom_dev = run_scan_args(dir.path(), &["--include-dev"]);
    let criterion =
        cargo_component_named(&sbom_dev, "criterion").expect("criterion present with --include-dev");
    assert!(
        has_dev_property(criterion),
        "criterion must carry mikebom:dev-dependency = true: {criterion:?}",
    );
}

#[test]
fn scan_cargo_build_dependency_is_treated_as_dev() {
    // FR-001 (build-deps): per the cargo book, build-deps don't
    // ship in the runtime artifact — same SBOM-filter semantic
    // as dev-deps.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1.0.197"

[build-dependencies]
cc = "1.0.83"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Cargo.lock"),
        r#"
version = 3

[[package]]
name = "demo"
version = "0.1.0"
dependencies = ["serde"]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c753"

[[package]]
name = "cc"
version = "1.0.83"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "deadbeef"
"#,
    )
    .unwrap();

    let sbom = run_scan_args(dir.path(), &[]);
    assert!(
        cargo_component_named(&sbom, "cc").is_none(),
        "cc (build-dep) must be dropped in default mode",
    );
    let sbom_dev = run_scan_args(dir.path(), &["--include-dev"]);
    let cc = cargo_component_named(&sbom_dev, "cc").expect("cc present with --include-dev");
    assert!(has_dev_property(cc), "cc must be tagged dev: {cc:?}");
}

#[test]
fn scan_cargo_production_wins_over_dev() {
    // FR-003: a crate listed in BOTH [dependencies] and
    // [dev-dependencies] is treated as production.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1.0.197"

[dev-dependencies]
serde = "1.0.197"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Cargo.lock"),
        r#"
version = 3

[[package]]
name = "demo"
version = "0.1.0"
dependencies = ["serde"]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c753"
"#,
    )
    .unwrap();

    let sbom = run_scan_args(dir.path(), &[]);
    let serde =
        cargo_component_named(&sbom, "serde").expect("serde must be retained (prod wins)");
    assert!(
        !has_dev_property(serde),
        "serde must NOT be tagged dev when also present in [dependencies]: {serde:?}",
    );
}

#[test]
fn scan_cargo_workspace_member_dev_dep_is_tagged() {
    // FR-001 + workspace traversal: a dev-dep declared by a
    // workspace member crate gets correctly classified.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["member-a"]
"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("member-a")).unwrap();
    std::fs::write(
        dir.path().join("member-a/Cargo.toml"),
        r#"
[package]
name = "member-a"
version = "0.1.0"

[dependencies]
serde = "1.0.197"

[dev-dependencies]
proptest = "1.4.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("Cargo.lock"),
        r#"
version = 3

[[package]]
name = "member-a"
version = "0.1.0"
dependencies = ["serde"]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c753"

[[package]]
name = "proptest"
version = "1.4.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abcd1234"
"#,
    )
    .unwrap();

    let sbom = run_scan_args(dir.path(), &[]);
    assert!(
        cargo_component_named(&sbom, "proptest").is_none(),
        "workspace member's dev-dep must be dropped",
    );
    let sbom_dev = run_scan_args(dir.path(), &["--include-dev"]);
    let proptest =
        cargo_component_named(&sbom_dev, "proptest").expect("proptest present with --include-dev");
    assert!(has_dev_property(proptest), "proptest must be tagged dev");
}
