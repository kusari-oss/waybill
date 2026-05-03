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
/// `--exclude-scope dev,build,test`). Returns the parsed SBOM JSON.
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
    // Milestone 052 FR-001 / FR-002: a crate declared in
    // [dev-dependencies] emits in default mode tagged with native
    // CDX scope: "excluded" + mikebom:lifecycle-scope: "development",
    // and is dropped under --exclude-scope dev,build,test.
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

    // Milestone 052/part-3: default scan now emits ALL scopes
    // (criterion present + tagged with native CDX scope: "excluded"
    // and mikebom:lifecycle-scope: "development"). Pre-052 default
    // dropped criterion silently.
    let sbom = run_scan_args(dir.path(), &[]);
    let criterion =
        cargo_component_named(&sbom, "criterion").expect("criterion present in default scan");
    assert!(
        has_dev_property(criterion),
        "criterion must carry native CDX scope: \"excluded\" + mikebom:lifecycle-scope: {criterion:?}",
    );
    assert!(
        cargo_component_named(&sbom, "serde").is_some(),
        "serde (prod-dep) must be retained",
    );

    // --exclude-scope dev,build,test reproduces the alpha.9-equivalent
    // strict deployed-runtime view: criterion dropped.
    let sbom_strict = run_scan_args(dir.path(), &["--exclude-scope", "dev,build,test"]);
    assert!(
        cargo_component_named(&sbom_strict, "criterion").is_none(),
        "criterion (dev-dep) must be dropped under --exclude-scope dev,build,test",
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

    // Milestone 052/part-3: default scan emits cc with native
    // CDX scope: "excluded" + mikebom:lifecycle-scope: "build".
    let sbom = run_scan_args(dir.path(), &[]);
    let cc = cargo_component_named(&sbom, "cc").expect("cc present in default scan");
    assert!(has_dev_property(cc), "cc must be tagged non-runtime: {cc:?}");
    // --exclude-scope build drops cc; --exclude-scope dev does not.
    let sbom_no_build = run_scan_args(dir.path(), &["--exclude-scope", "build"]);
    assert!(
        cargo_component_named(&sbom_no_build, "cc").is_none(),
        "cc (build-dep) must be dropped under --exclude-scope build",
    );
    let sbom_no_dev = run_scan_args(dir.path(), &["--exclude-scope", "dev"]);
    assert!(
        cargo_component_named(&sbom_no_dev, "cc").is_some(),
        "cc (build-dep) survives --exclude-scope dev — Build is distinct from Development",
    );
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

    // Milestone 052/part-3: default scan emits the workspace
    // member's dev-dep with native scope: "excluded" + lifecycle-
    // scope property; --exclude-scope dev,build,test drops it.
    let sbom = run_scan_args(dir.path(), &[]);
    let proptest =
        cargo_component_named(&sbom, "proptest").expect("proptest present in default scan");
    assert!(has_dev_property(proptest), "proptest must be tagged non-runtime");
    let sbom_strict = run_scan_args(dir.path(), &["--exclude-scope", "dev,build,test"]);
    assert!(
        cargo_component_named(&sbom_strict, "proptest").is_none(),
        "workspace member's dev-dep dropped under --exclude-scope dev,build,test",
    );
}

// --- Milestone 064: cargo main-module emission ------------------------

/// US1 AS#1 + SC-001: a single-crate cargo project with `[package]`
/// emits exactly one main-module component with the manifest-derived
/// PURL placed in CDX `metadata.component`.
#[test]
fn scan_cargo_single_crate_emits_main_module_in_metadata_component() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "demo-app"
version = "1.2.3"
edition = "2021"
"#,
    )
    .unwrap();
    let (output, _tmp, sbom_path) = run_scan_with_output(dir.path());
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let raw = std::fs::read_to_string(&sbom_path).expect("read sbom");
    let sbom: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    // metadata.component MUST be the main-module per FR-001a.
    let meta = &sbom["metadata"]["component"];
    assert_eq!(meta["type"].as_str(), Some("application"));
    assert_eq!(meta["purl"].as_str(), Some("pkg:cargo/demo-app@1.2.3"));
    assert_eq!(meta["name"].as_str(), Some("demo-app"));
    assert_eq!(meta["version"].as_str(), Some("1.2.3"));
    // C40 supplementary tag must be on metadata.component.properties.
    let props = meta["properties"]
        .as_array()
        .expect("metadata.component.properties array");
    let role = props
        .iter()
        .find(|p| p["name"].as_str() == Some("mikebom:component-role"));
    assert!(
        role.is_some(),
        "C40 mikebom:component-role property missing from metadata.component"
    );
    assert_eq!(
        role.unwrap()["value"].as_str(),
        Some("main-module"),
        "C40 value must be \"main-module\""
    );
    // Same PURL must NOT also appear in components[] (FR-001a).
    let components = sbom["components"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    assert!(
        !components
            .iter()
            .any(|c| c["purl"].as_str() == Some("pkg:cargo/demo-app@1.2.3")),
        "main-module's PURL must not double-emit in components[]"
    );
}

/// US1 AS#3: `version.workspace = true` resolves against the
/// enclosing workspace root's `[workspace.package].version`.
#[test]
fn scan_cargo_workspace_inherited_version_resolves() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo-workspace");
    let (output, _tmp, sbom_path) = run_scan_with_output(&path);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let raw = std::fs::read_to_string(&sbom_path).expect("read sbom");
    let sbom: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let main_module_purls: Vec<String> = sbom["components"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|c| {
            c["properties"]
                .as_array()
                .map(|p| {
                    p.iter().any(|prop| {
                        prop["name"].as_str() == Some("mikebom:component-role")
                            && prop["value"].as_str() == Some("main-module")
                    })
                })
                .unwrap_or(false)
        })
        .filter_map(|c| c["purl"].as_str().map(|s| s.to_string()))
        .collect();
    // Both members carry the workspace-inherited version (0.5.0).
    assert!(
        main_module_purls.iter().any(|p| p == "pkg:cargo/a@0.5.0"),
        "expected pkg:cargo/a@0.5.0 in main-module set: {main_module_purls:?}"
    );
    assert!(
        main_module_purls.iter().any(|p| p == "pkg:cargo/b@0.5.0"),
        "expected pkg:cargo/b@0.5.0 in main-module set: {main_module_purls:?}"
    );
    // Neither uses the 0.0.0-unknown placeholder — workspace inheritance
    // is the whole point of FR-001's resolver step 2.
    assert!(
        !main_module_purls
            .iter()
            .any(|p| p.contains("0.0.0-unknown")),
        "no main-module should fall back to placeholder when workspace \
         inheritance is resolvable: {main_module_purls:?}"
    );
}

/// FR-007 + FR-011 + #126: cargo workspace-root entries' lockfile
/// `dependencies = [...]` declarations now emit as direct-dep edges
/// from the milestone-064 main-module. For the cargo-workspace
/// fixture, member `b`'s path-dep on `a` (via `[dependencies] a =
/// { path = "../a" }`) MUST emit a `DependsOn` edge from `b`'s
/// main-module to `a`'s main-module.
#[test]
fn scan_cargo_workspace_path_dep_emits_main_module_to_main_module_edge() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo-workspace");
    let (_output, _tmp, sbom_path) = run_scan_with_output(&path);
    let raw = std::fs::read_to_string(&sbom_path).expect("read sbom");
    let sbom: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let deps = sbom["dependencies"].as_array().expect("deps array");
    let b_to_a = deps.iter().find(|d| {
        d["ref"].as_str() == Some("pkg:cargo/b@0.5.0")
            && d["dependsOn"]
                .as_array()
                .map(|arr| {
                    arr.iter().any(|x| x.as_str() == Some("pkg:cargo/a@0.5.0"))
                })
                .unwrap_or(false)
    });
    assert!(
        b_to_a.is_some(),
        "expected b → a workspace-member path-dep edge in cargo workspace SBOM. \
         dependencies array: {deps:#?}"
    );
}

/// FR-002: workspace-only `Cargo.toml` (no `[package]`) MUST NOT emit
/// a main-module for the root. Only members emit.
#[test]
fn scan_cargo_workspace_only_root_emits_no_main_module_for_root() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo-workspace");
    let (_output, _tmp, sbom_path) = run_scan_with_output(&path);
    let raw = std::fs::read_to_string(&sbom_path).expect("read sbom");
    let sbom: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    // No main-module should carry the workspace-root's name (the dir
    // is named "cargo-workspace"; no [package] declared at that
    // level, so no `pkg:cargo/cargo-workspace@*` should emit).
    let main_modules: Vec<&serde_json::Value> = sbom["components"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|c| {
            c["properties"]
                .as_array()
                .map(|p| {
                    p.iter().any(|prop| {
                        prop["name"].as_str() == Some("mikebom:component-role")
                            && prop["value"].as_str() == Some("main-module")
                    })
                })
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        main_modules.len(),
        2,
        "expected exactly 2 main-modules (members a + b); workspace root \
         (no [package]) MUST be skipped per FR-002. Got: {main_modules:?}"
    );
}
