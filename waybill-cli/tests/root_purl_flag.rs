//! Issue #359 — `--root-purl <PURL>` integration tests. The new
//! single-flag form takes precedence over the discrete milestone-077
//! `--root-name`/`--root-version`/`--root-purl-type`/`--no-root-purl`
//! surface (clap-`conflicts_with` mutex); operators can express full
//! purl-spec features the discrete flags don't reach (qualifiers,
//! subpaths, custom namespace splits) and the value emits verbatim
//! across CDX 1.6 / SPDX 2.3 / SPDX 3.

use std::path::Path;
use std::process::Command;

fn run_scan_cdx(path: &Path, extra_args: &[&str]) -> serde_json::Value {
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
        .arg("--file-inventory=off")
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let status = cmd.status().expect("mikebom should run");
    assert!(status.success(), "scan failed: {extra_args:?}");
    let raw = std::fs::read(&out_path).expect("read sbom");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn run_scan_spdx23(path: &Path, extra_args: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.spdx.json");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", out_path.to_string_lossy()))
        .arg("--file-inventory=off")
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let status = cmd.status().expect("mikebom should run");
    assert!(status.success(), "scan failed: {extra_args:?}");
    let raw = std::fs::read(&out_path).expect("read spdx");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn run_scan_spdx3(path: &Path, extra_args: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.spdx3.json");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(format!("spdx-3-json={}", out_path.to_string_lossy()))
        .arg("--file-inventory=off")
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let status = cmd.status().expect("mikebom should run");
    assert!(status.success(), "scan failed: {extra_args:?}");
    let raw = std::fs::read(&out_path).expect("read spdx3");
    serde_json::from_slice(&raw).expect("valid JSON")
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn root_purl_flag_emits_verbatim_purl_across_all_three_formats() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path();
    // The test PURL exercises features the discrete flags can't
    // reach: a slashed-namespace (`github.com/example/svc`) and a
    // qualifier (`?arch=amd64`). Both MUST survive to the wire.
    let purl = "pkg:golang/github.com/example/svc@v1.2.3?arch=amd64";

    let cdx = run_scan_cdx(path, &["--root-purl", purl]);
    assert_eq!(
        cdx["metadata"]["component"]["purl"].as_str(),
        Some(purl),
        "CDX metadata.component.purl must emit the operator-supplied PURL verbatim"
    );
    assert_eq!(
        cdx["metadata"]["component"]["name"].as_str(),
        Some("github.com/example/svc"),
        "CDX metadata.component.name must equal the PURL's namespace-prefixed name"
    );
    assert_eq!(
        cdx["metadata"]["component"]["version"].as_str(),
        Some("v1.2.3"),
        "CDX metadata.component.version must equal the PURL's version segment"
    );

    let spdx23 = run_scan_spdx23(path, &["--root-purl", purl]);
    let root = spdx23["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["SPDXID"]
                .as_str()
                .is_some_and(|id| id.starts_with("SPDXRef-DocumentRoot"))
        })
        .expect("SPDX 2.3 root Package");
    let purl_ref = root["externalRefs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["referenceType"].as_str() == Some("purl"))
        .expect("root externalRefs[purl] entry");
    assert_eq!(
        purl_ref["referenceLocator"].as_str(),
        Some(purl),
        "SPDX 2.3 root externalRefs[purl] must equal the operator-supplied PURL verbatim"
    );

    let spdx3 = run_scan_spdx3(path, &["--root-purl", purl]);
    let root3 = spdx3["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .find(|el| {
            el["type"].as_str() == Some("software_Package")
                && el["software_packageUrl"].as_str() == Some(purl)
        })
        .expect("SPDX 3 root software_Package with verbatim PURL");
    // The externalIdentifier[packageUrl] entry must also carry the
    // same verbatim value (SPDX 3 emits both slots).
    let ext_id = root3["externalIdentifier"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["externalIdentifierType"].as_str() == Some("packageUrl"))
        .expect("root externalIdentifier[packageUrl] entry");
    assert_eq!(
        ext_id["identifier"].as_str(),
        Some(purl),
        "SPDX 3 root externalIdentifier[packageUrl].identifier must equal the operator-supplied PURL"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn root_purl_invalid_value_exits_nonzero_at_clap_parse() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tmp.path().join("out.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path())
        .arg("--output")
        .arg(&out_path)
        .arg("--root-purl")
        .arg("not-a-valid-purl")
        .status()
        .expect("mikebom should run");
    assert!(
        !status.success(),
        "invalid --root-purl value must fail at clap parse time"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn root_purl_conflicts_with_root_name() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tmp.path().join("out.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path())
        .arg("--output")
        .arg(&out_path)
        .arg("--root-purl")
        .arg("pkg:generic/foo@1.0.0")
        .arg("--root-name")
        .arg("other-name")
        .status()
        .expect("mikebom should run");
    assert!(
        !status.success(),
        "--root-purl + --root-name MUST be clap-rejected (mutually exclusive)"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn root_purl_conflicts_with_no_root_purl() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tmp.path().join("out.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path())
        .arg("--output")
        .arg(&out_path)
        .arg("--root-purl")
        .arg("pkg:generic/foo@1.0.0")
        .arg("--root-name")
        .arg("foo")
        .arg("--no-root-purl")
        .status()
        .expect("mikebom should run");
    assert!(
        !status.success(),
        "--root-purl + --no-root-purl MUST be clap-rejected (mutually exclusive)"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn root_purl_absent_preserves_existing_root_name_behavior() {
    // Regression guard for the milestone-077 surface: when
    // --root-purl is NOT set, --root-name continues to produce the
    // generic-typed default PURL exactly as before.
    let tmp = tempfile::tempdir().unwrap();
    let cdx = run_scan_cdx(tmp.path(), &["--root-name", "my-app", "--root-version", "9.9.9"]);
    assert_eq!(
        cdx["metadata"]["component"]["name"].as_str(),
        Some("my-app")
    );
    assert_eq!(
        cdx["metadata"]["component"]["version"].as_str(),
        Some("9.9.9")
    );
    assert_eq!(
        cdx["metadata"]["component"]["purl"].as_str(),
        Some("pkg:generic/my-app@9.9.9")
    );
}
