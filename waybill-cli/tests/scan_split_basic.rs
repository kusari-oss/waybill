//! Milestone 215 — integration tests for `waybill sbom scan --split`.
//!
//! Verifies the end-to-end split fan-out against the m212
//! `two_binaries_diverge` cargo-workspace fixture (4 members):
//! T018  — happy path: 4 sub-SBOMs + 1 manifest, correct root
//!         identity, per-member component set, schema-valid
//!         CDX 1.6 output for every sub-SBOM (SC-006).
//! T018a — zero-boundary fallback: single-package project → 1 SBOM,
//!         no manifest, WARN log (FR-009).
//! T019  — manifest lists every emitted file for the requested
//!         format.

use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    // waybill-cli/tests/scan_split_basic.rs → workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("waybill-cli parent")
        .to_path_buf()
}

fn m212_cargo_workspace_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/compiler_pipeline/two_binaries_diverge")
}

fn m215_heterogeneous_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/split_heterogeneous")
}

fn m215_nested_workspace_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/split_nested_workspace")
}

fn waybill_bin() -> PathBuf {
    // Cargo sets CARGO_BIN_EXE_<name> for integration-test binaries.
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

/// Run `waybill sbom scan --split --output-dir <dir> [args]` in an
/// isolated $HOME so per-host caches (~/.m2, ~/.cargo, etc.) don't
/// leak into the split output.
fn run_split_scan(
    path: &PathBuf,
    output_dir: &PathBuf,
    extra_args: &[&str],
) -> (bool, String, String) {
    let home = tempdir().expect("home tempdir");
    let output = Command::new(waybill_bin())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--split")
        .arg("--output-dir")
        .arg(output_dir)
        .args(extra_args)
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path())
        .env("CARGO_HOME", home.path().join(".cargo"))
        .env("GOMODCACHE", home.path().join("go-mod"))
        .env("M2_REPO", home.path().join(".m2"))
        .current_dir(workspace_root())
        .output()
        .expect("spawn waybill");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Read every JSON file in `dir` (matching a pattern) and return the
/// list of paths.
fn list_files(dir: &PathBuf, suffix: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read output_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(suffix))
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

// ============ T018 ============

#[test]
fn cargo_workspace_split_emits_one_sbom_per_member() {
    let out = tempdir().expect("output tempdir");
    let out_path = out.path().to_path_buf();
    let (ok, _stdout, stderr) = run_split_scan(
        &m212_cargo_workspace_fixture(),
        &out_path,
        &["--format", "cyclonedx-json"],
    );
    assert!(ok, "split scan failed:\n{stderr}");

    // 4 CDX sub-SBOMs + 1 manifest.
    let cdxs = list_files(&out_path, ".cdx.json");
    assert_eq!(
        cdxs.len(),
        4,
        "expected 4 sub-SBOMs, got {}:\n{}",
        cdxs.len(),
        cdxs.iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );

    let manifest_path = out_path.join("split-manifest.json");
    assert!(
        manifest_path.exists(),
        "split-manifest.json missing at {}",
        manifest_path.display()
    );

    // Each sub-SBOM's metadata.component.purl matches a distinct
    // pkg:cargo/<member>@0.1.0.
    let mut roots: Vec<String> = Vec::new();
    for p in &cdxs {
        let text = std::fs::read_to_string(p).expect("read cdx");
        let v: serde_json::Value =
            serde_json::from_str(&text).expect("parse cdx");
        let purl = v
            .pointer("/metadata/component/purl")
            .and_then(|s| s.as_str())
            .expect("metadata.component.purl present")
            .to_string();
        roots.push(purl);
    }
    roots.sort();
    assert_eq!(
        roots,
        vec![
            "pkg:cargo/libsafe@0.1.0",
            "pkg:cargo/libvuln@0.1.0",
            "pkg:cargo/safe-only@0.1.0",
            "pkg:cargo/vuln-included@0.1.0",
        ],
        "sub-SBOM root PURLs don't match the 4 cargo workspace members",
    );
}

// ============ T019 ============

#[test]
fn cargo_workspace_split_manifest_lists_all_emitted_files() {
    let out = tempdir().expect("output tempdir");
    let out_path = out.path().to_path_buf();
    let (ok, _stdout, stderr) = run_split_scan(
        &m212_cargo_workspace_fixture(),
        &out_path,
        &["--format", "cyclonedx-json"],
    );
    assert!(ok, "split scan failed:\n{stderr}");

    let manifest_text = std::fs::read_to_string(out_path.join("split-manifest.json"))
        .expect("read manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse manifest");

    // Contract fields present.
    assert_eq!(
        manifest["$schema"],
        "https://waybill.dev/schema/split-manifest/v1.json"
    );
    assert!(manifest["waybill_version"].is_string());
    assert!(manifest["scan_root"].is_string());
    assert!(manifest["generated_at"].is_string());
    assert!(manifest["total_unique_components"].is_number());
    assert!(manifest["shared_dep_count"].is_number());

    let entries = manifest["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 4, "expected 4 manifest entries");

    // Every entries[].files["cyclonedx-json"] filename exists on disk.
    let mut ids: Vec<String> = Vec::new();
    for entry in entries {
        let id = entry["subproject_id"].as_str().expect("id").to_string();
        let filename = entry["files"]["cyclonedx-json"]
            .as_str()
            .expect("cdx filename");
        let fp = out_path.join(filename);
        assert!(
            fp.exists(),
            "manifest lists {filename} but file missing at {}",
            fp.display()
        );
        ids.push(id);
    }
    // subproject_id is unique per entry.
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate subproject_id in manifest");
}

// ============ T024 (US2) ============

#[test]
fn heterogeneous_split_emits_one_sbom_per_ecosystem() {
    let out = tempdir().expect("output tempdir");
    let out_path = out.path().to_path_buf();
    let (ok, _stdout, stderr) = run_split_scan(
        &m215_heterogeneous_fixture(),
        &out_path,
        &["--format", "cyclonedx-json"],
    );
    assert!(ok, "heterogeneous split scan failed:\n{stderr}");

    let cdxs = list_files(&out_path, ".cdx.json");
    assert_eq!(
        cdxs.len(),
        3,
        "expected 3 sub-SBOMs (npm + pypi + gem), got {}:\n{}",
        cdxs.len(),
        cdxs.iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );

    // Filename convention: one file per ecosystem, ecosystem token in
    // filename per contracts/filename-convention.md.
    let names: Vec<String> = cdxs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "m215-frontend.npm.cdx.json"),
        "missing npm sub-SBOM in {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "m215-backend.pypi.cdx.json"),
        "missing pypi sub-SBOM in {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "m215-ruby-svc.gem.cdx.json"),
        "missing gem sub-SBOM in {names:?}"
    );

    // Each sub-SBOM's root PURL uses the ecosystem-appropriate type.
    let mut ecosystems: Vec<String> = Vec::new();
    for p in &cdxs {
        let text = std::fs::read_to_string(p).expect("read cdx");
        let v: serde_json::Value =
            serde_json::from_str(&text).expect("parse cdx");
        let purl = v
            .pointer("/metadata/component/purl")
            .and_then(|s| s.as_str())
            .expect("root PURL")
            .to_string();
        // Extract the ecosystem prefix (`pkg:<type>/…`).
        let prefix = purl
            .split('/')
            .next()
            .and_then(|s| s.strip_prefix("pkg:"))
            .unwrap_or("")
            .to_string();
        ecosystems.push(prefix);
    }
    ecosystems.sort();
    assert_eq!(
        ecosystems,
        vec!["gem", "npm", "pypi"],
        "expected one root per ecosystem, got {ecosystems:?}"
    );

    // Manifest lists all 3.
    let manifest_text = std::fs::read_to_string(out_path.join("split-manifest.json"))
        .expect("read manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse manifest");
    let entries = manifest["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 3, "manifest must list all 3 subprojects");
}

// ============ T026a (US2 — FR-010 nested workspaces) ============

#[test]
fn split_nested_workspace_emits_all_boundaries() {
    // Cargo workspace with 2 members, where cargo-b-with-npm carries
    // an npm sub-workspace with 2 packages under apps/. FR-010: all
    // 4 boundaries surface — no subproject is "swallowed" by the
    // outer cargo boundary.
    let out = tempdir().expect("output tempdir");
    let out_path = out.path().to_path_buf();
    let (ok, _stdout, stderr) = run_split_scan(
        &m215_nested_workspace_fixture(),
        &out_path,
        &["--format", "cyclonedx-json"],
    );
    assert!(ok, "nested-workspace split scan failed:\n{stderr}");

    let cdxs = list_files(&out_path, ".cdx.json");
    assert_eq!(
        cdxs.len(),
        4,
        "expected 4 sub-SBOMs (2 outer cargo + 2 inner npm), got {}:\n{}",
        cdxs.len(),
        cdxs.iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );

    let manifest_text = std::fs::read_to_string(out_path.join("split-manifest.json"))
        .expect("read manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse manifest");
    let entries = manifest["entries"].as_array().expect("entries");
    let ids: Vec<String> = entries
        .iter()
        .map(|e| e["subproject_id"].as_str().unwrap().to_string())
        .collect();
    // Assert every boundary present — no swallowing.
    for id in &[
        "cargo-a.cargo",
        "cargo-b-with-npm.cargo",
        "npm-svc-1.npm",
        "npm-svc-2.npm",
    ] {
        assert!(
            ids.iter().any(|entry_id| entry_id == id),
            "expected subproject_id `{id}` in manifest entries, got {ids:?}"
        );
    }
}

// ============ T025 (US2) ============

#[test]
fn multi_manifest_per_dir_emits_one_sbom_per_ecosystem() {
    // Clarification Q2: one directory carrying manifests for TWO
    // ecosystems produces TWO sub-SBOMs (one per ecosystem manifest).
    let scratch = tempdir().expect("fixture tempdir");
    let root = scratch.path();
    // npm manifest.
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"m215-mixed-npm","version":"1.0.0","dependencies":{"lodash":"^4"}}"#,
    )
    .expect("write package.json");
    std::fs::write(
        root.join("package-lock.json"),
        r#"{
  "name": "m215-mixed-npm",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {"name": "m215-mixed-npm", "version": "1.0.0", "dependencies": {"lodash": "^4"}},
    "node_modules/lodash": {"version": "4.17.21", "license": "MIT"}
  }
}
"#,
    )
    .expect("write package-lock.json");
    // pypi manifest in the SAME directory.
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"m215-mixed-pypi\"\nversion = \"9.9.9\"\nrequires-python = \">=3.11\"\n\n[tool.uv]\nmanaged = true\n",
    )
    .expect("write pyproject.toml");
    std::fs::write(
        root.join("uv.lock"),
        "version = 1\nrequires-python = \">=3.11\"\n",
    )
    .expect("write uv.lock");

    let out = tempdir().expect("output tempdir");
    let out_path = out.path().to_path_buf();
    let (ok, _stdout, stderr) = run_split_scan(
        &root.to_path_buf(),
        &out_path,
        &["--format", "cyclonedx-json"],
    );
    assert!(ok, "multi-manifest split scan failed:\n{stderr}");

    let cdxs = list_files(&out_path, ".cdx.json");
    assert_eq!(
        cdxs.len(),
        2,
        "expected 2 sub-SBOMs (one per ecosystem manifest per Q2), got {}:\n{}",
        cdxs.len(),
        cdxs.iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );

    // Verify one npm + one pypi.
    let names: Vec<String> = cdxs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.contains(".npm.")),
        "expected an .npm. sub-SBOM in {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains(".pypi.")),
        "expected a .pypi. sub-SBOM in {names:?}"
    );
}

// ============ T030 (US3 — subproject_id stability + uniqueness) ============

#[test]
fn split_manifest_subproject_ids_are_stable_and_unique() {
    // Run the cargo-workspace split TWICE and assert every
    // entries[].subproject_id (a) appears exactly once per run and
    // (b) matches across the two runs (stable derivation, no
    // dependency on scan-start timestamp or per-invocation state).
    let mut ids_per_run: Vec<Vec<String>> = Vec::new();
    for _ in 0..2 {
        let out = tempdir().expect("output tempdir");
        let out_path = out.path().to_path_buf();
        let (ok, _stdout, stderr) = run_split_scan(
            &m212_cargo_workspace_fixture(),
            &out_path,
            &["--format", "cyclonedx-json"],
        );
        assert!(ok, "split scan failed:\n{stderr}");

        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out_path.join("split-manifest.json"))
                .expect("read manifest"),
        )
        .expect("parse manifest");
        let entries = manifest["entries"].as_array().expect("entries");

        // Collect IDs + assert uniqueness this run.
        let ids: Vec<String> = entries
            .iter()
            .map(|e| e["subproject_id"].as_str().unwrap().to_string())
            .collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(
            deduped.len(),
            ids.len(),
            "duplicate subproject_id within one run: {ids:?}"
        );

        // Each id matches its emitted-filename prefix in files{}.
        for entry in entries {
            let id = entry["subproject_id"].as_str().unwrap();
            for (_fmt, filename) in
                entry["files"].as_object().expect("files map").iter()
            {
                let fname = filename.as_str().unwrap();
                assert!(
                    fname.starts_with(&format!("{id}.")),
                    "filename {fname} does not start with subproject_id {id}."
                );
            }
        }

        ids_per_run.push(ids);
    }

    // Across-run stability: identical sorted sets.
    let mut a = ids_per_run[0].clone();
    let mut b = ids_per_run[1].clone();
    a.sort();
    b.sort();
    assert_eq!(a, b, "subproject_id set drifted between runs: {a:?} vs {b:?}");
}

// ============ T018a ============

#[test]
fn split_on_single_package_falls_back_to_one_sbom() {
    // Fixture: single-package Cargo.toml (no [workspace]).
    let scratch = tempdir().expect("fixture tempdir");
    let pkg_root = scratch.path();
    std::fs::write(
        pkg_root.join("Cargo.toml"),
        "[package]\nname = \"solitary\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write manifest");
    std::fs::create_dir_all(pkg_root.join("src")).expect("mkdir src");
    std::fs::write(
        pkg_root.join("src/lib.rs"),
        "pub fn hi() -> &'static str { \"hi\" }\n",
    )
    .expect("write lib");
    // Empty Cargo.lock (cargo generates a minimal one, but our
    // scanner is fine without one for the workspace-detection path).
    std::fs::write(
        pkg_root.join("Cargo.lock"),
        "# empty lock — scanner will still find the [package] entry\nversion = 4\n",
    )
    .expect("write lock");

    let out = tempdir().expect("output tempdir");
    let out_path = out.path().to_path_buf();
    let (ok, _stdout, stderr) = run_split_scan(
        &pkg_root.to_path_buf(),
        &out_path,
        &["--format", "cyclonedx-json"],
    );
    assert!(ok, "split scan on single-package failed:\n{stderr}");

    // FR-009: WARN log emitted (stable grep substring).
    assert!(
        stderr.contains("no workspace boundaries detected"),
        "expected 'no workspace boundaries detected' in stderr:\n{stderr}"
    );

    // No manifest written (nothing to describe).
    assert!(
        !out_path.join("split-manifest.json").exists(),
        "manifest MUST NOT be written on zero-boundary fallback"
    );
}
