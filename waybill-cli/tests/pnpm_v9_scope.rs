//! #1297: real pnpm v9 input must support the same dev exclusion as npm.
//!
//! Fixture generated with pnpm 9.15.9 `install --lockfile-only --ignore-scripts`.
//! Unlike synthetic reader tests, this preserves package-manager-authored
//! importer and snapshot structure. No install or network is needed to replay.
//! Runtime string-width and dev wrap-ansi share string-width and strip-ansi.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

// -----------------------------------------------------------------------
// Test helpers: invoke the real CLI and normalize format-specific identities.
// -----------------------------------------------------------------------

fn scan(format: &str, exclude_dev: bool) -> Value {
    scan_fixture(format, exclude_dev, "pnpm_v9_scope", false)
}

fn scan_fixture(
    format: &str,
    exclude_dev: bool,
    fixture_name: &str,
    child_manifest: bool,
) -> Value {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture_name);
    for name in ["package.json", "pnpm-lock.yaml"] {
        std::fs::copy(fixture.join(name), temp.path().join(name)).expect("copy generated fixture");
    }
    if child_manifest {
        std::fs::create_dir_all(temp.path().join("packages/runtime")).expect("member directory");
        std::fs::copy(
            fixture.join("packages/runtime/package.json"),
            temp.path().join("packages/runtime/package.json"),
        )
        .expect("copy member manifest");
    }
    let output_path = temp.path().join("out.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_waybill"));
    command.args(["--offline", "sbom", "scan", "--path"]);
    command.arg(temp.path());
    command.args(["--format", format, "--output"]);
    command.arg(&output_path);
    if exclude_dev {
        command.args(["--exclude-scope", "dev"]);
    }
    let output = command.output().expect("run waybill");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&std::fs::read(output_path).expect("read SBOM")).expect("parse SBOM")
}

fn package_set(document: &Value, format: &str) -> BTreeSet<String> {
    let values: Vec<&str> = match format {
        "cyclonedx-json" => document["components"]
            .as_array()
            .expect("components")
            .iter()
            .filter_map(|component| component["purl"].as_str())
            .collect(),
        "spdx-2.3-json" => document["packages"]
            .as_array()
            .expect("packages")
            .iter()
            .flat_map(|package| package["externalRefs"].as_array().into_iter().flatten())
            .filter(|reference| reference["referenceType"] == "purl")
            .filter_map(|reference| reference["referenceLocator"].as_str())
            .collect(),
        "spdx-3-json" => document["@graph"]
            .as_array()
            .expect("graph")
            .iter()
            .filter_map(|element| element["software_packageUrl"].as_str())
            .collect(),
        _ => panic!("unsupported test format"),
    };
    values
        .into_iter()
        .filter(|value| !value.starts_with("pkg:npm/waybill-fixture-"))
        .map(str::to_owned)
        .collect()
}

fn check_contract(format: &str) {
    let runtime = runtime_set();
    let mut all = runtime.clone();
    all.extend(
        [
            "wrap-ansi@7.0.0",
            "ansi-styles@4.3.0",
            "color-convert@2.0.1",
            "color-name@1.1.4",
        ]
        .into_iter()
        .map(|identity| format!("pkg:npm/{identity}")),
    );
    assert_eq!(
        package_set(&scan(format, false), format),
        all,
        "unfiltered inventory"
    );
    assert_eq!(
        package_set(&scan(format, true), format),
        runtime,
        "dev-only closure must leave while the shared runtime closure remains"
    );
}

fn runtime_set() -> BTreeSet<String> {
    [
        "ansi-regex@5.0.1",
        "emoji-regex@8.0.0",
        "is-fullwidth-code-point@3.0.0",
        "string-width@4.2.3",
        "strip-ansi@6.0.1",
    ]
    .into_iter()
    .map(|identity| format!("pkg:npm/{identity}"))
    .collect()
}

// -----------------------------------------------------------------------
// End of helpers. Each format independently exercises the CLI contract.
// -----------------------------------------------------------------------

#[test]
fn pnpm_v9_cyclonedx_excludes_dev_and_keeps_shared_runtime() {
    check_contract("cyclonedx-json");
}

#[test]
fn pnpm_v9_spdx23_excludes_dev_and_keeps_shared_runtime() {
    check_contract("spdx-2.3-json");
}

#[test]
fn pnpm_v9_spdx3_excludes_dev_and_keeps_shared_runtime() {
    check_contract("spdx-3-json");
}

#[test]
fn pnpm_v9_workspace_runtime_closure_survives_with_or_without_member_manifest() {
    for format in ["cyclonedx-json", "spdx-2.3-json", "spdx-3-json"] {
        for child_manifest in [false, true] {
            let document = scan_fixture(format, true, "pnpm_v9_scope_workspace", child_manifest);
            assert_eq!(
                package_set(&document, format),
                runtime_set(),
                "runtime evidence from the lockfile must survive global propagation; child_manifest={child_manifest}, format={format}"
            );
        }
    }
}

#[test]
fn pnpm_v9_optional_workspace_closure_cannot_be_demoted_to_development() {
    for format in ["cyclonedx-json", "spdx-2.3-json", "spdx-3-json"] {
        for child_manifest in [false, true] {
            let document = scan_fixture(
                format,
                true,
                "pnpm_v9_scope_optional_workspace",
                child_manifest,
            );
            assert_eq!(
                package_set(&document, format),
                runtime_set(),
                "optional importer evidence must not become development; child_manifest={child_manifest}, format={format}"
            );
        }
    }
}
