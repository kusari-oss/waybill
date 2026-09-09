//! Exercise document naming through the real filesystem scan and SPDX writer.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

mod common;

const REPO_ARGS: &[&str] = &[
    "--repo",
    "https://github.com/aeraki-mesh/aeraki.git",
    "--git-ref",
    "1.0.5",
];

fn scan(path: &Path, args: &[&str]) -> (Value, String) {
    let (spdx2, stderr) = scan_format(path, args, "spdx-2.3-json");
    let (spdx3, stderr3) = scan_format(path, args, "spdx-3-json");
    assert_spdx3_names(&spdx3, spdx2["name"].as_str().unwrap());
    for warning in ["Cannot derive SPDX document name", "without a revision"] {
        if stderr.contains(warning) {
            assert!(stderr3.contains(warning), "{stderr3}");
        }
    }
    (spdx2, stderr)
}

fn scan_format(path: &Path, args: &[&str], format: &str) -> (Value, String) {
    let output_dir = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let output = output_dir.path().join("chosen-filename.json");
    let mut command = Command::new(common::bin());
    common::normalize::apply_fake_home_env(&mut command, fake_home.path());
    let result = command
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(path)
        .args(["--format", format, "--no-deep-hash", "--output"])
        .arg(&output)
        .args(args)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(result.status.success(), "{stderr}");
    (
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap(),
        stderr,
    )
}

fn assert_spdx3_names(document: &Value, expected: &str) {
    let graph = document["@graph"].as_array().unwrap();
    assert!(graph.iter().any(|e| e["type"] == "SpdxDocument"));
    for element in graph
        .iter()
        .filter(|e| e["type"] == "SpdxDocument" || e["type"] == "software_Sbom")
    {
        assert_eq!(element["name"], expected, "{element}");
    }
}

fn npm_project(path: &Path) {
    // This dependency sorts before the root by both PURL and SPDX 3's hashed IRI.
    std::fs::write(
        path.join("package.json"),
        r#"{"name":"z-root","version":"2.4.6","license":"MIT","dependencies":{"aaa-dependency4":"1.0.0"}}"#,
    ).unwrap();
    std::fs::write(
        path.join("package-lock.json"),
        r#"{
          "name":"z-root","version":"2.4.6","lockfileVersion":3,
          "packages":{
            "":{"name":"z-root","version":"2.4.6","license":"MIT","dependencies":{"aaa-dependency4":"1.0.0"}},
            "node_modules/aaa-dependency4":{"version":"1.0.0","license":"Apache-2.0"}
          }
        }"#,
    ).unwrap();
}

fn without_timestamps(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("created");
            map.remove("annotationDate");
            for child in map.values_mut() {
                without_timestamps(child);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(without_timestamps),
        _ => {}
    }
}

#[test]
fn root_overrides_are_checkout_independent_and_only_change_document_name() {
    for prefix in ["tmp.PM9IVSVGio", "tmp.other-checkout-"] {
        let checkout = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        npm_project(checkout.path());
        let mut args = REPO_ARGS.to_vec();
        args.extend([
            "--root-name",
            "aeraki-mesh/aeraki",
            "--root-version",
            "1.0.5",
        ]);
        let (mut document, _) = scan(checkout.path(), &args);
        assert_eq!(document["name"], "aeraki-mesh/aeraki 1.0.5");
        let root_id = &document["documentDescribes"][0];
        let root = document["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| &p["SPDXID"] == root_id)
            .unwrap();
        assert_eq!(root["name"], "aeraki-mesh/aeraki");
        assert_eq!(root["versionInfo"], "1.0.5");

        // The old document label can still be requested explicitly.
        // Every other field must be identical, including package IDs,
        // PURLs, licenses, namespaces and all dependency relationships.
        let old_name = checkout.path().file_name().unwrap().to_str().unwrap();
        args.extend(["--scan-target-name", old_name]);
        let (mut old_label, _) = scan(checkout.path(), &args);
        assert_eq!(old_label["name"], old_name);
        document["name"] = json!(old_name);
        without_timestamps(&mut document);
        without_timestamps(&mut old_label);
        assert_eq!(document, old_label);
    }
}

#[test]
fn manifest_root_outside_first_package_is_used() {
    for prefix in ["tmp.checkout-a-", "tmp.checkout-b-"] {
        let checkout = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        npm_project(checkout.path());
        let (document, _) = scan(checkout.path(), REPO_ARGS);
        let roots = document["documentDescribes"].as_array().unwrap();
        assert!(
            !roots.contains(&document["packages"][0]["SPDXID"]),
            "{document}"
        );
        assert_eq!(document["name"], "z-root 2.4.6");
        assert!(document["relationships"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["relationshipType"] == "DEPENDS_ON"));
    }
}

#[test]
fn missing_and_unusable_roots_use_repository_and_exact_ref() {
    for prefix in ["tmp.first-", "tmp.second-"] {
        let checkout = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        let cases: &[&[&str]] = &[
            &[],
            &["--root-name", "NOASSERTION", "--root-version", "1.0.5"],
            &["--root-name", "tmp.PM9IVSVGio", "--root-version", "1.0.5"],
            &["--root-name", ".tmpAbCdEf", "--root-version", "1.0.5"],
            &[
                "--root-name",
                "valid-project",
                "--root-version",
                "NOASSERTION",
            ],
            &["--root-name", "valid-project"],
            &["--root-version", "1.0.5"],
        ];
        for extra in cases {
            let mut args = REPO_ARGS.to_vec();
            args.extend_from_slice(extra);
            let (document, _) = scan(checkout.path(), &args);
            assert_eq!(document["name"], "aeraki-mesh/aeraki 1.0.5", "{extra:?}");
        }
        std::fs::write(
            checkout.path().join("package.json"),
            r#"{"name":"","version":""}"#,
        )
        .unwrap();
        let (document, _) = scan(checkout.path(), REPO_ARGS);
        assert_eq!(document["name"], "aeraki-mesh/aeraki 1.0.5");
    }
}

#[test]
fn metadata_file_and_explicit_document_name_keep_precedence() {
    let checkout = tempfile::tempdir().unwrap();
    let metadata_dir = tempfile::tempdir().unwrap();
    let metadata = metadata_dir.path().join("metadata.json");
    std::fs::write(&metadata, r#"{"scan_target_name":"Reviewed release"}"#).unwrap();
    let mut args = REPO_ARGS.to_vec();
    args.extend([
        "--root-name",
        "aeraki-mesh/aeraki",
        "--root-version",
        "1.0.5",
        "--metadata-file",
        metadata.to_str().unwrap(),
    ]);
    assert_eq!(scan(checkout.path(), &args).0["name"], "Reviewed release");
    args.truncate(args.len() - 2);
    args.extend(["--scan-target-name", "Explicit document"]);
    assert_eq!(scan(checkout.path(), &args).0["name"], "Explicit document");
}

#[test]
fn no_identity_is_reported_without_leaking_checkout_name() {
    let checkout = tempfile::Builder::new()
        .prefix("tmp.unidentified-")
        .tempdir()
        .unwrap();
    let (document, stderr) = scan(checkout.path(), &[]);
    assert_eq!(
        document["name"],
        "Waybill source scan (identity unavailable)"
    );
    assert!(
        stderr.contains("Cannot derive SPDX document name"),
        "{stderr}"
    );
    assert!(stderr.contains("--repo and --git-ref"), "{stderr}");
}

#[test]
fn repository_fallback_handles_ssh_and_missing_ref() {
    let checkout = tempfile::tempdir().unwrap();
    let (document, _) = scan(
        checkout.path(),
        &[
            "--repo",
            "git@github.com:aeraki-mesh/aeraki.git",
            "--git-ref",
            "release/1.0.5",
        ],
    );
    assert_eq!(document["name"], "aeraki-mesh/aeraki release/1.0.5");
    let (document, stderr) = scan(checkout.path(), &["--repo", REPO_ARGS[1]]);
    assert_eq!(document["name"], "aeraki-mesh/aeraki");
    assert!(stderr.contains("without a revision"), "{stderr}");
}

#[test]
fn unversioned_go_root_uses_scanned_repository_ref() {
    let checkout = tempfile::tempdir().unwrap();
    std::fs::write(
        checkout.path().join("go.mod"),
        "module github.com/aeraki-mesh/aeraki\n\ngo 1.20\n",
    )
    .unwrap();
    let (document, _) = scan(checkout.path(), REPO_ARGS);
    assert_eq!(document["name"], "aeraki-mesh/aeraki 1.0.5");
    assert!(document["packages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["versionInfo"] == "v0.0.0-unknown"));
}

#[test]
fn spdx3_uses_root_element_and_changes_only_document_labels() {
    for prefix in ["tmp.spdx3-first-", "tmp.spdx3-second-"] {
        let checkout = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        npm_project(checkout.path());
        let mut args = REPO_ARGS.to_vec();
        args.extend(["--sbom-type", "source"]);
        let (natural, _) = scan_format(checkout.path(), &args, "spdx-3-json");
        assert_spdx3_names(&natural, "z-root 2.4.6");
        let graph = natural["@graph"].as_array().unwrap();
        let document = graph.iter().find(|e| e["type"] == "SpdxDocument").unwrap();
        let first_package = graph
            .iter()
            .find(|e| e["type"] == "software_Package")
            .unwrap();
        assert!(!document["rootElement"]
            .as_array()
            .unwrap()
            .contains(&first_package["spdxId"]));

        args.extend([
            "--root-name",
            "aeraki-mesh/aeraki",
            "--root-version",
            "1.0.5",
        ]);
        let (mut updated, _) = scan_format(checkout.path(), &args, "spdx-3-json");
        assert_spdx3_names(&updated, "aeraki-mesh/aeraki 1.0.5");
        assert!(updated["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "software_Sbom"));
        args.extend(["--scan-target-name", "aeraki-mesh/aeraki"]);
        let (mut old_label, _) = scan_format(checkout.path(), &args, "spdx-3-json");
        assert_spdx3_names(&old_label, "aeraki-mesh/aeraki");
        for element in updated["@graph"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .filter(|e| e["type"] == "SpdxDocument" || e["type"] == "software_Sbom")
        {
            element["name"] = json!("aeraki-mesh/aeraki");
        }
        without_timestamps(&mut updated);
        without_timestamps(&mut old_label);
        assert_eq!(updated, old_label);

        let (cdx, _) = scan_format(checkout.path(), &args, "cyclonedx-json");
        assert!(cdx.get("name").is_none());
        assert_eq!(cdx["metadata"]["component"]["name"], "aeraki-mesh/aeraki");
        assert_eq!(cdx["metadata"]["component"]["version"], "1.0.5");
    }
}
