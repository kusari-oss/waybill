//! Milestone 107 — SC-006 polyglot-robustness regression test.
//!
//! Build a temp fixture containing well-formed manifests from all three
//! new Yocto readers (opkg installed-DB, Yocto image manifest, BitBake
//! recipe walker) AND a deliberately-malformed file in each ecosystem.
//! Scan it once and assert:
//!
//! 1. The scan exits 0 (no abort across ecosystems).
//! 2. At least one expected component emerges from EACH of the three
//!    well-formed manifests.
//! 3. The malformed files don't poison sibling readers.
//!
//! Mirrors `tests/polyglot_robustness_ecosystem_106.rs`. Locks in the
//! SC-006 polyglot-safety guarantee against regressions; complements
//! `offline_mode_audit_ecosystem_107.rs` (offline-only audit) by
//! exercising the actual runtime behavior end-to-end.

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir parent");
    }
    std::fs::write(path, contents).expect("write file");
}

fn build_fixture(root: &Path) {
    // The opkg reader looks for ONE `/var/lib/opkg/status` at the
    // scan-root level (mirroring how Yocto/OE actually lays out a
    // rootfs). We exercise the parser's "tolerate garbage between
    // well-formed stanzas" path by putting a malformed stanza
    // alongside good ones in the SAME file — that's the realistic
    // shape of robustness pressure (operator inspecting a partially-
    // corrupted image dump).
    write(
        &root.join("var/lib/opkg/status"),
        "Package: waybill-fixture-libcore\n\
         Version: 1.2.3\n\
         Architecture: waybill-fixture-arch\n\
         Status: install user installed\n\
         \n\
         this-is-garbage-without-a-colon\n\
         a-malformed-stanza-fragment\n\
         \n\
         Package: waybill-fixture-libssl\n\
         Version: 3.0.5\n\
         Architecture: waybill-fixture-arch\n\
         Status: install user installed\n",
    );

    // ── Yocto image manifest layout ────────────────────────────
    // The walker iterates `build/tmp/deploy/images/<machine>/*.manifest`,
    // so we put one well-formed manifest under a "good-machine" dir
    // and one malformed manifest under a "bad-machine" dir — both at
    // the same level. The well-formed one MUST still emit.
    write(
        &root.join("build/tmp/deploy/images/waybill-fixture-good/good.manifest"),
        "waybill-fixture-gst waybill-fixture-arch 1.22.7\n",
    );
    write(
        &root.join("build/tmp/deploy/images/waybill-fixture-bad/broken.manifest"),
        "only-one-token\n\
         two tokens here\n\
         four tokens are here too\n",
    );

    // ── BitBake recipe layer (well-formed + unexpanded-variable) ─
    write(
        &root.join("meta-waybill-fixture/recipes-waybill/waybill-fixture-recipe/waybill-fixture-recipe_1.0.0.bb"),
        "SUMMARY = \"fixture\"\nLICENSE = \"MIT\"\n",
    );
    // Unexpanded `${...}` filename — silent-skip path per FR-008.
    write(
        &root.join("meta-waybill-fixture/recipes-waybill/waybill-fixture-shared/${PN}_${PV}.bb"),
        "SUMMARY = \"unexpanded variables in filename\"\n",
    );
}

#[test]
fn well_formed_yocto_manifests_emit_components_despite_neighboring_malformed_files() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fixture_root = workdir.path().join("fixture");
    build_fixture(&fixture_root);

    let out_path = workdir.path().join("sbom.cdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_root.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "scan unexpectedly failed: status={:?}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"]
        .as_array()
        .expect("components[] present");
    let purls: Vec<String> = components
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();

    // opkg installed-DB well-formed stanzas BOTH MUST surface despite
    // the malformed garbage block between them in the same status file.
    assert!(
        purls.iter().any(|p| p == "pkg:opkg/waybill-fixture-libcore@1.2.3?arch=waybill-fixture-arch"),
        "opkg well-formed component (libcore) missing; got purls: {purls:#?}",
    );
    assert!(
        purls.iter().any(|p| p == "pkg:opkg/waybill-fixture-libssl@3.0.5?arch=waybill-fixture-arch"),
        "opkg well-formed component (libssl) past the garbage block missing; got purls: {purls:#?}",
    );

    // Yocto image-manifest well-formed line in `good-machine/` MUST
    // surface despite the malformed `bad-machine/broken.manifest`
    // sibling.
    assert!(
        purls.iter().any(|p| p == "pkg:opkg/waybill-fixture-gst@1.22.7?arch=waybill-fixture-arch"),
        "yocto-manifest well-formed component (gst) missing; got purls: {purls:#?}",
    );

    // BitBake recipe well-formed file MUST surface. Milestone 128
    // (FR-001) migrated the recipe PURL from `pkg:bitbake/...?layer=`
    // to `pkg:generic/...?openembedded=true&layer=...` (qualifiers
    // alphabetized by `Purl::new`).
    assert!(
        purls.iter().any(|p| p == "pkg:generic/waybill-fixture-recipe@1.0.0?layer=meta-waybill-fixture&openembedded=true"),
        "bitbake-recipe well-formed component missing; got purls: {purls:#?}",
    );

    // Negative assertion: the unexpanded `${PN}_${PV}.bb` recipe MUST
    // NOT have produced a placeholder component (FR-008 silent skip).
    assert!(
        !purls.iter().any(|p| p.contains("%24") || p.contains("PN") || p.contains("PV")),
        "unexpanded BitBake recipe should be silently skipped; got purls: {purls:#?}",
    );
}
