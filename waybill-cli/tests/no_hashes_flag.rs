//! `--no-hashes` must suppress content hashes in EVERY emitted format.
//!
//! Issue #1002. Before this test the flag had no coverage at all, which
//! is why a leak survived: CycloneDX gated `components[].hashes[]` on
//! it and SPDX 2.3 gated `packages[].checksums[]`, but SPDX 3's
//! `build_packages` never received the flag and emitted
//! `verifiedUsing[]` regardless. Measured on the npm fixture before the
//! fix: `CDX=0  SPDX3=1` — a scan run with hashes explicitly suppressed
//! still published one.
//!
//! An operator passing `--no-hashes` is asking for content digests to
//! stay out of the document. One format ignoring that is a correctness
//! bug, not a cosmetic one, so this asserts all three together rather
//! than per format — the #1000 / #1009 lesson that each emitter has to
//! be checked independently.

mod common;

use std::process::Command;

use common::fixture_path;
use common::normalize::apply_fake_home_env;

/// Count of elements carrying a content hash, per format.
struct HashCounts {
    cdx: usize,
    spdx23: usize,
    spdx3: usize,
}

fn scan(no_hashes: bool) -> HashCounts {
    let fx = fixture_path("npm/node-modules-walk");
    assert!(fx.exists(), "fixture missing: {}", fx.display());
    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let cdx = tmp.path().join("o.cdx.json");
    let s23 = tmp.path().join("o.spdx.json");
    let s3 = tmp.path().join("o.spdx3.json");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fx)
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
        .arg("--no-deep-hash");
    if no_hashes {
        cmd.arg("--no-hashes");
    }
    let out = cmd.output().expect("waybill should run");
    assert!(
        out.status.success(),
        "scan failed (no_hashes={no_hashes}): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let read = |p: &std::path::Path| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(p).expect("read output"))
            .expect("valid JSON")
    };
    let c = read(&cdx);
    let a = read(&s23);
    let g = read(&s3);

    HashCounts {
        cdx: c["components"]
            .as_array()
            .map(|arr| arr.iter().filter(|x| x.get("hashes").is_some()).count())
            .unwrap_or(0),
        spdx23: a["packages"]
            .as_array()
            .map(|arr| arr.iter().filter(|x| x.get("checksums").is_some()).count())
            .unwrap_or(0),
        spdx3: g["@graph"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|e| {
                        e.get("verifiedUsing").is_some()
                            && matches!(
                                e["type"].as_str(),
                                Some("software_Package") | Some("software_File")
                            )
                    })
                    .count()
            })
            .unwrap_or(0),
    }
}

#[test]
fn m1002_no_hashes_suppresses_content_hashes_in_every_format() {
    // Control: without the flag at least one hash is emitted, otherwise
    // the assertion below would pass vacuously on a fixture that simply
    // has no hashes to suppress.
    let with = scan(false);
    assert!(
        with.cdx > 0 || with.spdx23 > 0 || with.spdx3 > 0,
        "control failed: the fixture emits no hashes at all, so this test \
         cannot prove --no-hashes does anything (cdx={} spdx23={} spdx3={})",
        with.cdx,
        with.spdx23,
        with.spdx3
    );

    let without = scan(true);
    assert_eq!(
        (without.cdx, without.spdx23, without.spdx3),
        (0, 0, 0),
        "--no-hashes must suppress hashes in all three formats; got \
         cdx={} spdx23={} spdx3={} (control emitted cdx={} spdx23={} spdx3={})",
        without.cdx,
        without.spdx23,
        without.spdx3,
        with.cdx,
        with.spdx23,
        with.spdx3
    );
}
