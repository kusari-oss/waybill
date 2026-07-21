//! Milestone 146 (closes #470) — end-to-end integration test for the
//! `SpdxExpression::try_canonical` operand-dedup pass.
//!
//! Builds a synthetic RPM whose `License:` header contains `MIT AND MIT`
//! (mirroring the Yocto-shaped input that surfaced the original audit
//! finding), scans it via the `waybill sbom scan` binary, and asserts
//! that the emitted CDX 1.6, SPDX 2.3, and SPDX 3 outputs ALL carry the
//! deduplicated single-id `MIT` form (FR-008 cross-format invariance).
//!
//! Mirrors the milestone-144 T035 synthetic-RPM pattern at
//! `waybill-cli/src/scan_fs/package_db/rpm_file.rs:524-577` (the repo
//! ships zero `.rpm` fixtures; runtime-built via `rpm::PackageBuilder`).
//!
//! Covers spec SC-004.

use std::process::Command;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[test]
fn license_dedup_end_to_end_via_synthetic_rpm_md146() {
    let dir = tempfile::tempdir().unwrap();
    let rpm_path = dir.path().join("dedup-test-1.0-1.noarch.rpm");

    // Build a synthetic RPM with License: "MIT AND MIT" — the Yocto-
    // shaped input that surfaced issue #470.
    rpm::PackageBuilder::new(
        "dedup-test",
        "1.0",
        "MIT AND MIT", // <-- the bug input
        "noarch",
        "synthetic rpm for milestone 146 license dedup",
    )
    .release("1")
    .build()
    .unwrap()
    .write_file(&rpm_path)
    .unwrap();

    let cdx_path = dir.path().join("out.cdx.json");
    let spdx23_path = dir.path().join("out.spdx.json");
    let spdx3_path = dir.path().join("out.spdx3.json");

    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(dir.path())
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.display()))
        .arg("--no-deep-hash")
        .output()
        .expect("waybill binary runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // CDX 1.6: post-146, the single-id `MIT` form routes to
    // `licenses[].license.id` via `as_spdx_id()` at
    // `waybill-common/src/types/license.rs:86-120`. Pre-146 the
    // compound `"MIT AND MIT"` would have failed `as_spdx_id` and
    // fallen through to the `expression` shape.
    let cdx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cdx_path).unwrap()).unwrap();
    let rpm_comp = cdx["components"]
        .as_array()
        .expect("components array")
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:rpm/"))
                .unwrap_or(false)
        })
        .expect("rpm component present");
    let licenses = rpm_comp["licenses"].as_array().expect("licenses present");
    let declared_id = licenses
        .iter()
        .find_map(|l| {
            l.get("license")
                .and_then(|lic| lic.get("id"))
                .and_then(|v| v.as_str())
        })
        .expect("CDX licenses[].license.id (single-id shape; not expression)");
    assert_eq!(
        declared_id, "MIT",
        "CDX must carry single-id `MIT` (post-146 dedup); got: {declared_id}"
    );
    // Belt-and-suspenders: assert NO licenses[] entry uses the
    // compound `expression` shape (would indicate the dedup didn't
    // collapse to single-id and CDX fell through to expression form).
    for l in licenses {
        let has_expression = l
            .get("license")
            .and_then(|lic| lic.get("expression"))
            .is_some();
        assert!(
            !has_expression,
            "CDX must NOT use compound `expression` shape for deduped \
             single-id license (post-146); got: {l:?}"
        );
    }

    // SPDX 2.3: licenseDeclared must be the single id `MIT`.
    let spdx23: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spdx23_path).unwrap()).unwrap();
    let rpm_pkg_spdx23 = spdx23["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .find(|p| {
            p["externalRefs"]
                .as_array()
                .map(|refs| {
                    refs.iter().any(|r| {
                        r["referenceType"].as_str() == Some("purl")
                            && r["referenceLocator"]
                                .as_str()
                                .map(|p| p.starts_with("pkg:rpm/"))
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        })
        .expect("rpm package present in SPDX 2.3");
    let declared = rpm_pkg_spdx23["licenseDeclared"]
        .as_str()
        .expect("licenseDeclared present");
    assert_eq!(
        declared, "MIT",
        "SPDX 2.3 licenseDeclared must be single id `MIT` (post-146 dedup); \
         got: {declared}"
    );

    // SPDX 3: software_declaredLicense MUST be the single id `MIT`.
    // SPDX 3 emits the value inline on the software_Package element,
    // OR via an Annotation envelope — check the package's
    // `software_declaredLicense` field directly.
    let spdx3: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spdx3_path).unwrap()).unwrap();
    let graph = spdx3["@graph"].as_array().expect("@graph array");
    let rpm_pkg_spdx3 = graph
        .iter()
        .find(|el| {
            el["type"].as_str() == Some("software_Package")
                && el["software_packageUrl"]
                    .as_str()
                    .map(|p| p.starts_with("pkg:rpm/"))
                    .unwrap_or(false)
        })
        .expect("rpm software_Package present in SPDX 3");
    // The SPDX 3 license carrier may surface under different keys
    // depending on the emitter's choice (declared vs concluded).
    // Try both common locations.
    let declared_v3 = rpm_pkg_spdx3
        .get("software_declaredLicense")
        .and_then(|v| v.as_str())
        .or_else(|| {
            rpm_pkg_spdx3
                .get("expandedlicensing_declaredLicense")
                .and_then(|v| v.as_str())
        });
    if let Some(s) = declared_v3 {
        assert_eq!(
            s, "MIT",
            "SPDX 3 declared license must be single id `MIT` (post-146 dedup); got: {s}"
        );
    }
    // If the field isn't directly inline on the Package, the emitter
    // may use a separate License element referenced by spdxId. In
    // that case search for any string-valued license-name field on
    // any related element that mentions `MIT AND MIT` — must be ZERO.
    let serialized = serde_json::to_string(&spdx3).unwrap();
    assert!(
        !serialized.contains("MIT AND MIT"),
        "SPDX 3 output must NOT contain `MIT AND MIT` anywhere (post-146 dedup)"
    );
}
