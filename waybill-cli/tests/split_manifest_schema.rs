//! Milestone 215 — T028: split-manifest.json schema-drift regression.
//!
//! Runs a small `--split` fixture through the emit pipeline, loads
//! the generated `split-manifest.json`, and validates it against the
//! v1 schema pinned at `waybill-cli/contracts/split-manifest-v1.schema.json`.
//! Fails if manifest emission drifts from the contract shape.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn m212_cargo_workspace_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/compiler_pipeline/two_binaries_diverge")
}

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts/split-manifest-v1.schema.json")
}

fn waybill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

fn validator() -> &'static jsonschema::Validator {
    static CELL: OnceLock<jsonschema::Validator> = OnceLock::new();
    CELL.get_or_init(|| {
        let raw = std::fs::read_to_string(schema_path())
            .expect("read split-manifest v1 schema");
        let schema: serde_json::Value =
            serde_json::from_str(&raw).expect("parse split-manifest v1 schema");
        jsonschema::validator_for(&schema).expect("compile split-manifest v1 schema")
    })
}

#[test]
fn split_manifest_validates_against_v1_schema() {
    // Run a real split scan against the m212 cargo-workspace fixture.
    let home = tempdir().expect("home tempdir");
    let out = tempdir().expect("output tempdir");
    let status = Command::new(waybill_bin())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(m212_cargo_workspace_fixture())
        .arg("--split")
        .arg("--output-dir")
        .arg(out.path())
        .arg("--format")
        .arg("cyclonedx-json")
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path())
        .env("CARGO_HOME", home.path().join(".cargo"))
        .env("GOMODCACHE", home.path().join("go-mod"))
        .env("M2_REPO", home.path().join(".m2"))
        .current_dir(workspace_root())
        .status()
        .expect("spawn waybill");
    assert!(status.success(), "split scan failed with {status}");

    let manifest_path = out.path().join("split-manifest.json");
    assert!(
        manifest_path.exists(),
        "manifest not written at {}",
        manifest_path.display()
    );
    let raw = std::fs::read_to_string(&manifest_path).expect("read manifest");
    let doc: serde_json::Value =
        serde_json::from_str(&raw).expect("parse manifest JSON");

    // Validate against the pinned v1 schema.
    let errors: Vec<String> = validator()
        .iter_errors(&doc)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "split-manifest.json failed v1 schema validation:\n  {}\n\nManifest was:\n{}",
        errors.join("\n  "),
        serde_json::to_string_pretty(&doc).unwrap_or_default(),
    );
}
