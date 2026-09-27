//! A project's own declared license must reach its emitted SBOM.
//!
//! Issue #954. These are integration tests on purpose, and the reason is worth
//! stating because it cost a wrong conclusion during implementation.
//!
//! The cargo reader's unit tests assert that `build_cargo_main_module_entry`
//! returns the declared license. They passed while the emitted SBOM contained
//! no license at all, because the defect was not in the reader: when a
//! `Cargo.lock` is present, the lockfile-derived entry for the same PURL wins,
//! and the branch that augments it with the manifest's facts copied
//! annotations, tier, depends and parent — but not the license. A unit test on
//! the producing function cannot see that. Only a test that reads the document
//! can.
//!
//! So every assertion here reads emitted output, and the lockfile case is a
//! first-class scenario rather than an afterthought.

mod common;

use std::path::Path;
use std::process::Command;

use common::normalize::apply_fake_home_env;

/// Declared licenses found on the document's primary component, per format.
#[derive(Debug, PartialEq, Eq)]
struct LicenseView {
    cdx: Vec<String>,
    cdx_acknowledgement: Vec<String>,
    spdx23: Vec<String>,
    spdx3_present: bool,
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(path, body).expect("write fixture file");
}

/// Scan `root` and read the primary component's declared licenses out of all
/// three formats.
fn scan(root: &Path) -> LicenseView {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let cdx = tmp.path().join("o.cdx.json");
    let s23 = tmp.path().join("o.spdx.json");
    let s3 = tmp.path().join("o.spdx3.json");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    apply_fake_home_env(&mut cmd, fake_home.path());
    let out = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s23.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .output()
        .expect("waybill should run");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let read = |p: &Path| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(p).expect("read output"))
            .expect("valid JSON")
    };
    let c = read(&cdx);
    let a = read(&s23);
    let g = read(&s3);

    let primary_ref = c["metadata"]["component"]["bom-ref"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let primary_name = c["metadata"]["component"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let cdx_licenses = c["metadata"]["component"]["licenses"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // SPDX 2.3: find the package matching the primary component by name and
    // read its `licenseDeclared`, ignoring the NOASSERTION placeholder.
    let spdx23 = a["packages"]
        .as_array()
        .map(|pkgs| {
            pkgs.iter()
                .filter(|p| p["name"].as_str() == Some(primary_name.as_str()))
                .filter_map(|p| p["licenseDeclared"].as_str())
                .filter(|v| *v != "NOASSERTION")
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // SPDX 3: presence of any license-expression element mentioning the value
    // is enough here; the exact vocabulary is asserted by the SPDX 3 goldens.
    let spdx3_present = g["@graph"]
        .as_array()
        .map(|els| {
            els.iter().any(|e| {
                e["type"]
                    .as_str()
                    .is_some_and(|t| t.contains("LicenseExpression"))
            })
        })
        .unwrap_or(false);

    let _ = primary_ref;
    LicenseView {
        cdx: cdx_licenses
            .iter()
            .filter_map(|l| {
                l["license"]["id"]
                    .as_str()
                    .or_else(|| l["license"]["name"].as_str())
                    .or_else(|| l["expression"].as_str())
            })
            .map(str::to_string)
            .collect(),
        cdx_acknowledgement: cdx_licenses
            .iter()
            .filter_map(|l| l["license"]["acknowledgement"].as_str())
            .map(str::to_string)
            .collect(),
        spdx23,
        spdx3_present,
    }
}

/// A single-crate cargo project declaring `license` literally.
fn literal_license_project(root: &Path, license: &str) {
    write(
        &root.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"waybill-fixture-declared\"\nversion = \"1.0.0\"\n\
             edition = \"2021\"\nlicense = \"{license}\"\n"
        ),
    );
    write(&root.join("src/main.rs"), "fn main() {}\n");
}

/// A cargo workspace whose member inherits the license, optionally with a
/// lockfile present. The lockfile is what made this feature fail end to end
/// while every unit test passed.
fn workspace_project(root: &Path, with_lockfile: bool) {
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n\n\
         [workspace.package]\nversion = \"9.9.9\"\nlicense = \"Apache-2.0\"\n",
    );
    write(
        &root.join("member/Cargo.toml"),
        "[package]\nname = \"waybill-fixture-member\"\nversion.workspace = true\n\
         license.workspace = true\nedition = \"2021\"\n",
    );
    write(&root.join("member/src/main.rs"), "fn main() {}\n");
    if with_lockfile {
        write(
            &root.join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\nversion = 4\n\n\
             [[package]]\nname = \"waybill-fixture-member\"\nversion = \"9.9.9\"\n",
        );
    }
}

// ---------------------------------------------------------------------------
// T016 / T017 / T018 — US1: the declared license reaches every format
// ---------------------------------------------------------------------------

#[test]
fn m954_us1_declared_license_reaches_all_three_formats() {
    let tmp = tempfile::tempdir().expect("tempdir");
    literal_license_project(tmp.path(), "Apache-2.0");
    let got = scan(tmp.path());

    assert_eq!(
        got.cdx,
        vec!["Apache-2.0".to_string()],
        "CycloneDX must carry the declared license"
    );
    assert_eq!(
        got.cdx_acknowledgement,
        vec!["declared".to_string()],
        "the license is the project's own declaration, not a third-party conclusion (FR-002)"
    );
    assert_eq!(
        got.spdx23,
        vec!["Apache-2.0".to_string()],
        "SPDX 2.3 licenseDeclared must carry it too"
    );
    assert!(
        got.spdx3_present,
        "SPDX 3 must emit a license-expression element"
    );
}

#[test]
fn m954_us1_control_a_project_declaring_nothing_emits_nothing() {
    // The control for the test above. Without this, an assertion that a license
    // is present could pass for a reason unrelated to extraction — and a test
    // that cannot fail proves nothing.
    let tmp = tempfile::tempdir().expect("tempdir");
    write(
        &tmp.path().join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-unlicensed\"\nversion = \"1.0.0\"\n\
         edition = \"2021\"\n",
    );
    write(&tmp.path().join("src/main.rs"), "fn main() {}\n");
    let got = scan(tmp.path());
    assert!(
        got.cdx.is_empty() && got.spdx23.is_empty(),
        "a project declaring no license must emit none, got cdx={:?} spdx23={:?}",
        got.cdx,
        got.spdx23
    );
}

#[test]
fn m954_us1_offline_scan_still_carries_the_license() {
    // FR-012 / SC-002. Every scan here is already `--offline`; this test states
    // the requirement explicitly so that a future change routing licenses
    // through enrichment fails here rather than silently regressing offline
    // behaviour.
    let tmp = tempfile::tempdir().expect("tempdir");
    literal_license_project(tmp.path(), "MIT");
    let got = scan(tmp.path());
    assert_eq!(got.cdx, vec!["MIT".to_string()]);
    assert_eq!(got.cdx_acknowledgement, vec!["declared".to_string()]);
}

#[test]
fn m954_us1_no_concluded_licenses_are_invented() {
    // FR-003 / SC-006. A manifest declaration must not appear as a third-party
    // conclusion; enrichment owns that slot and this feature does not touch it.
    let tmp = tempfile::tempdir().expect("tempdir");
    literal_license_project(tmp.path(), "Apache-2.0");
    let got = scan(tmp.path());
    assert!(
        !got.cdx_acknowledgement.iter().any(|a| a == "concluded"),
        "a declared license must not be emitted as concluded, got {:?}",
        got.cdx_acknowledgement
    );
}

// ---------------------------------------------------------------------------
// T019 — FR-007: extraction must never cost a scan or a component
// ---------------------------------------------------------------------------

#[test]
fn m954_fr007_malformed_license_field_does_not_fail_the_scan() {
    // The manifest parses for identity; `license` is the wrong TOML type. The
    // scan must succeed and the component must still be emitted.
    let tmp = tempfile::tempdir().expect("tempdir");
    write(
        &tmp.path().join("Cargo.toml"),
        "[package]\nname = \"waybill-fixture-badtype\"\nversion = \"1.0.0\"\n\
         edition = \"2021\"\nlicense = 42\n",
    );
    write(&tmp.path().join("src/main.rs"), "fn main() {}\n");
    // `scan` asserts a zero exit itself; reaching the assertions proves FR-007.
    let got = scan(tmp.path());
    assert!(
        got.cdx.is_empty(),
        "a non-string license must not be emitted as an identifier"
    );
}

// ---------------------------------------------------------------------------
// T021 / T022 — FR-011a / FR-011b: workspace inheritance
// ---------------------------------------------------------------------------

#[test]
fn m954_fr011a_workspace_inherited_license_reaches_the_sbom() {
    let tmp = tempfile::tempdir().expect("tempdir");
    workspace_project(tmp.path(), false);
    let got = scan(tmp.path());
    assert_eq!(
        got.cdx,
        vec!["Apache-2.0".to_string()],
        "`license.workspace = true` must resolve against [workspace.package]"
    );
}

#[test]
fn m954_fr011a_inheritance_survives_a_lockfile() {
    // THE regression guard for this feature.
    //
    // With a `Cargo.lock` present the lockfile-derived entry wins the PURL and
    // is augmented in place with the manifest's facts. That branch copies a
    // named set of fields; the license was missing from it, so this scenario
    // emitted nothing while the lockfile-free scenario above emitted correctly.
    // Unit tests on the reader could not distinguish the two.
    let tmp = tempfile::tempdir().expect("tempdir");
    workspace_project(tmp.path(), true);
    let got = scan(tmp.path());
    assert_eq!(
        got.cdx,
        vec!["Apache-2.0".to_string()],
        "a declared license must survive the lockfile augment-in-place path"
    );
    assert_eq!(got.cdx_acknowledgement, vec!["declared".to_string()]);
    assert_eq!(got.spdx23, vec!["Apache-2.0".to_string()]);
}

#[test]
fn m954_fr011a_lockfile_and_lockfile_free_scans_agree() {
    // Stronger than the two tests above taken separately: the presence of a
    // lockfile must not change the declared license at all. This is the
    // property that was violated, and it is the one worth guarding.
    let with = tempfile::tempdir().expect("tempdir");
    workspace_project(with.path(), true);
    let without = tempfile::tempdir().expect("tempdir");
    workspace_project(without.path(), false);

    let a = scan(with.path());
    let b = scan(without.path());
    assert_eq!(
        a.cdx, b.cdx,
        "licenses diverged between lockfile and lockfile-free scans of the same manifests"
    );
    assert_eq!(a.cdx_acknowledgement, b.cdx_acknowledgement);
    assert!(
        !a.cdx.is_empty(),
        "control: both scans emitted nothing, so agreement is vacuous"
    );
}

#[test]
fn m954_fr011b_unresolvable_inheritance_emits_nothing_and_succeeds() {
    // The workspace declares a version but no license, so the member inherits
    // nothing. Absent, not an error.
    let tmp = tempfile::tempdir().expect("tempdir");
    write(
        &tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n\n\
         [workspace.package]\nversion = \"9.9.9\"\n",
    );
    write(
        &tmp.path().join("member/Cargo.toml"),
        "[package]\nname = \"waybill-fixture-orphan\"\nversion.workspace = true\n\
         license.workspace = true\nedition = \"2021\"\n",
    );
    write(&tmp.path().join("member/src/main.rs"), "fn main() {}\n");
    let got = scan(tmp.path());
    assert!(
        got.cdx.is_empty(),
        "an inheritance nothing satisfies is a missing declaration, got {:?}",
        got.cdx
    );
}

// ---------------------------------------------------------------------------
// T019 / US2 — FR-004: an unrecognised declaration is preserved
// ---------------------------------------------------------------------------

#[test]
fn m954_fr004_uncanonicalisable_license_is_preserved_in_the_document() {
    // `AllRightsReserved` is a real legacy spelling and not a valid SPDX
    // expression. It must survive into the document rather than be dropped:
    // SPDX 2.3 mints a `LicenseRef-` for it, so `licenseDeclared` must not be
    // NOASSERTION.
    let tmp = tempfile::tempdir().expect("tempdir");
    literal_license_project(tmp.path(), "AllRightsReserved");
    let got = scan(tmp.path());
    assert!(
        !got.spdx23.is_empty(),
        "an unrecognised declaration must not become NOASSERTION — dropping it \
         loses the most legally significant thing the manifest says"
    );
    assert!(
        got.spdx23.iter().all(|v| v.starts_with("LicenseRef-")),
        "it must be a non-listed license reference, never presented as a listed \
         identifier (FR-004c), got {:?}",
        got.spdx23
    );
    assert!(
        !got.cdx.is_empty(),
        "CycloneDX must carry it too, got {:?}",
        got.cdx
    );
}
