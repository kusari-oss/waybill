//! Cross-format parity for C198, on a document that actually carries
//! the field (#1248).
//!
//! `holistic_parity::parity_apk` already walks every catalog row
//! against the shared apk fixture, but that fixture's installed-db
//! stanzas carry no `o:` field and the deb fixture's carry no
//! `Source:`, so C198's three extractors all read empty there and
//! compare equal having compared nothing. That is the failure mode
//! C185's extractor documents from its own experience: "The
//! fixture-driven gate would not have caught that — it holds no
//! closure data, so both sides read empty and compare equal."
//!
//! So this test builds its own rootfs. It asserts the three sets are
//! equal AND non-empty, because equality alone is exactly what a
//! silently-broken emitter would also satisfy. It also re-checks row
//! A1, which a second `purl` externalRef in SPDX 2.3 breaks unless
//! `spdx23_purl` filters the source row out.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;

/// Synthetic coordinates throughout, per
/// `feedback_fixture_synthetic_package_names`: real OS coordinates in
/// a fixture trip Kusari Inspector's advisory scan (PRs #285, #640).
/// The shapes are what matter and all three are represented — an
/// origin that differs from the installed name, one that equals it,
/// and a stanza with no origin at all.
fn write_rootfs(root: &Path) {
    let apk_db = root.join("lib/apk/db");
    std::fs::create_dir_all(&apk_db).expect("mkdir apk db");
    std::fs::write(
        apk_db.join("installed"),
        // `waybill-fixture-libcrypto` and `waybill-fixture-libssl`
        // share an origin, as libcrypto3/libssl3 do with openssl —
        // the case that makes the set smaller than the component
        // count, and the reason the assertions below compare sets
        // rather than counts.
        "P:waybill-fixture-libcrypto\nV:1.2.3-r0\nA:x86_64\no:waybill-fixture-ssl\n\
         \n\
         P:waybill-fixture-libssl\nV:1.2.3-r0\nA:x86_64\no:waybill-fixture-ssl\n\
         \n\
         P:waybill-fixture-z\nV:1.3.2-r0\nA:x86_64\no:waybill-fixture-z\n\
         \n\
         P:waybill-fixture-keys\nV:2.5-r0\nA:x86_64\n",
    )
    .expect("write installed");
    std::fs::create_dir_all(root.join("etc")).expect("mkdir etc");
    std::fs::write(root.join("etc/alpine-release"), "3.23.4\n").expect("write alpine-release");
}

struct Triple {
    cdx: serde_json::Value,
    spdx23: serde_json::Value,
    spdx3: serde_json::Value,
}

fn triple_scan(root: &Path) -> Triple {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let cdx = tmp.path().join("out.cdx.json");
    let s23 = tmp.path().join("out.spdx.json");
    let s3 = tmp.path().join("out.spdx3.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s23.to_string_lossy()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.to_string_lossy()))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("waybill runs");
    assert!(
        out.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read = |p: &PathBuf| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(p).expect("read output")).expect("parse")
    };
    Triple {
        cdx: read(&cdx),
        spdx23: read(&s23),
        spdx3: read(&s3),
    }
}

fn row(id: &str) -> &'static waybill::parity::extractors::ParityExtractor {
    waybill::parity::extractors::EXTRACTORS
        .iter()
        .find(|e| e.row_id == id)
        .unwrap_or_else(|| panic!("row {id} is not registered in EXTRACTORS"))
}

#[test]
fn c198_agrees_across_all_three_formats_on_data_that_carries_it() {
    let root = tempfile::tempdir().expect("tempdir");
    write_rootfs(root.path());
    let scan = triple_scan(root.path());
    let r = row("C198");

    let cdx: BTreeSet<String> = (r.cdx)(&scan.cdx);
    let spdx23: BTreeSet<String> = (r.spdx23)(&scan.spdx23);
    let spdx3: BTreeSet<String> = (r.spdx3)(&scan.spdx3);

    // Non-empty FIRST. An emitter that stopped emitting, an extractor
    // keyed on a stale comment string, or a fixture that lost its
    // `o:` fields would all produce three empty sets that compare
    // equal — which is how this row could pass while testing nothing.
    assert!(
        !cdx.is_empty(),
        "no C198 values extracted from CycloneDX; the fixture or the emitter is not doing \
         what this test exists to check"
    );

    assert_eq!(cdx, spdx23, "CycloneDX and SPDX 2.3 disagree on C198");
    assert_eq!(spdx23, spdx3, "SPDX 2.3 and SPDX 3 disagree on C198");

    // Two of the three origin-bearing packages share an origin, and
    // the fourth has none, so three components collapse to two
    // distinct source PURLs.
    // No `distro=` qualifier: a bare `--path` scan does not pick up
    // `/etc/alpine-release`, which is pre-existing reader behaviour
    // and not this feature's concern. What matters is that the source
    // PURL carries exactly the qualifiers the installed PURL does, so
    // the two stay consistent however the scan was invoked — asserted
    // by `source_purl_mirrors_the_installed_purls_qualifiers` below.
    // The parity layer's canonical form for a scalar is its JSON
    // representation, so a string arrives quoted. Unwrapped here so
    // the expectation below reads as the PURLs it is about.
    let unquote = |v: &String| -> String {
        serde_json::from_str::<String>(v).unwrap_or_else(|_| v.clone())
    };
    let cdx: BTreeSet<String> = cdx.iter().map(unquote).collect();

    let expected: BTreeSet<String> = [
        "pkg:apk/alpine/waybill-fixture-ssl@1.2.3-r0?arch=source",
        "pkg:apk/alpine/waybill-fixture-z@1.3.2-r0?arch=source",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(cdx, expected, "unexpected source-package PURL set");

    // The stanza with no `o:` must not have contributed one.
    assert!(
        !cdx.iter().any(|v| v.contains("waybill-fixture-keys")),
        "a package with no o: field acquired a source package: {cdx:?}"
    );
}

#[test]
fn a1_purl_parity_survives_the_second_spdx23_purl_ref() {
    let root = tempfile::tempdir().expect("tempdir");
    write_rootfs(root.path());
    let scan = triple_scan(root.path());
    let r = row("A1");

    let cdx: BTreeSet<String> = (r.cdx)(&scan.cdx);
    let spdx23: BTreeSet<String> = (r.spdx23)(&scan.spdx23);
    let spdx3: BTreeSet<String> = (r.spdx3)(&scan.spdx3);

    assert!(!cdx.is_empty(), "no PURLs extracted from CycloneDX");
    assert_eq!(
        cdx, spdx23,
        "row A1 broke: SPDX 2.3 is contributing the #1248 source-package externalRef to the \
         PURL row, where CycloneDX has only the single-valued component.purl"
    );
    assert_eq!(spdx23, spdx3, "row A1: SPDX 2.3 and SPDX 3 disagree");

    // None of the A1 values may be a source-package identifier.
    assert!(
        !cdx.iter().any(|p| p.contains("arch=source")),
        "an arch=source PURL leaked into row A1: {cdx:?}"
    );
}

/// The source PURL must differ from the installed one in the `arch`
/// qualifier and the name, and in nothing else. A source PURL that
/// picked up a different `distro` or `epoch` would be matched against
/// a different distro release's advisories than the package it
/// describes.
#[test]
fn source_purl_mirrors_the_installed_purls_qualifiers() {
    let root = tempfile::tempdir().expect("tempdir");
    write_rootfs(root.path());
    let scan = triple_scan(root.path());

    let mut checked = 0usize;
    for c in scan.cdx["components"].as_array().expect("components") {
        let Some(installed) = c["purl"].as_str() else {
            continue;
        };
        let Some(source) = c["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == "waybill:source-package")
            .and_then(|p| p["value"].as_str())
        else {
            continue;
        };
        let quals = |p: &str| -> BTreeSet<String> {
            p.split_once('?')
                .map(|(_, q)| q.split('&').map(str::to_string).collect())
                .unwrap_or_default()
        };
        let installed_quals: BTreeSet<String> = quals(installed)
            .into_iter()
            .filter(|q| !q.starts_with("arch="))
            .collect();
        let source_quals: BTreeSet<String> = quals(source)
            .into_iter()
            .filter(|q| !q.starts_with("arch="))
            .collect();
        assert_eq!(
            installed_quals, source_quals,
            "qualifiers diverge between installed {installed} and source {source}"
        );
        assert!(
            source.contains("arch=source"),
            "source PURL {source} is missing arch=source"
        );
        checked += 1;
    }
    assert!(
        checked >= 3,
        "expected at least 3 components with a source package, checked {checked}"
    );
}
