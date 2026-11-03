//! Milestone 1273 US3 (#1280) — a resolved component carries the license the
//! cached package set declares.
//!
//! Measured at 0.14.0: **0 of 66** emitted hackage components carried a
//! license, though `hackage-packages.nix` — the file the resolution already
//! downloads, parses and caches — declares one for every package in it. Probe
//! B measured 19,437 declarations against 19,253 attribute blocks, so the
//! zero was never a coverage limit in the source; nothing read the field.
//!
//! # Cache isolation (mandatory)
//!
//! **No test here reads the host's `~/.cache/waybill/nixpkgs/`.** Each test
//! seeds a `tempfile::tempdir()` with the synthetic package set below and
//! points `WAYBILL_NIXPKGS_CACHE` at it, per `nix_haskell_resolution_m926.rs`.
//! A test that read the real cache would pass here and fail on CI, and
//! Principle VII forbids it.
//!
//! The parser's own coverage of the three declaration families lives in unit
//! tests over inline literals (`package_set.rs::license_tests`), which need no
//! cache at all. What *these* tests add is the rest of the pipeline: the
//! resolution call site, the native-field mapping, and all three emitters —
//! any of which can be broken while the parser's tests stay green.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// Every license shape `license-shapes.sh` found in the real file, on
/// synthetic packages. Not invented forms: each is one of the three measured
/// families, including the two FR-011/FR-011a cases.
const PACKAGES: &str = r#"
  waybill-fixture-attr-bsd3 = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-attr-bsd3";
      version = "1.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.bsd3;
  }) { };
  waybill-fixture-spdx-mit = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-spdx-mit";
      version = "2.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.meta.getLicenseFromSpdxId "MIT";
  }) { };
  waybill-fixture-compound = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-compound";
      version = "3.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = "(BSD-2-Clause OR Apache-2.0)";
  }) { };
  waybill-fixture-unversioned = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-unversioned";
      version = "4.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = "GPL";
  }) { };
  waybill-fixture-unknown-lic = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-unknown-lic";
      version = "5.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = "unknown";
  }) { };
  waybill-fixture-public-domain = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-public-domain";
      version = "6.0.0";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
      license = lib.licenses.publicDomain;
  }) { };
"#;

const CONFIG: &str = "self: super: {\n}\n";

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/haskell_hpack")
        .join(name)
}

fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rev_dir = dir.path().join(REV);
    std::fs::create_dir_all(&rev_dir).unwrap();
    std::fs::write(rev_dir.join("hackage-packages.nix"), PACKAGES).unwrap();
    std::fs::write(rev_dir.join("configuration-common.nix"), CONFIG).unwrap();
    // One series only, matching the fixture's `flake.nix`. Without that file
    // `candidate_series` returns every KNOWN_GHC_SERIES and an offline miss on
    // any one degrades the whole pass (#975) — which made US2's first fixture
    // pass vacuously before the implementation existed.
    std::fs::write(rev_dir.join("configuration-ghc-9.6.x.nix"), CONFIG).unwrap();
    dir
}

/// All three formats from one scan, so the native-field mapping is asserted
/// where it is specified rather than in CycloneDX alone.
struct Docs {
    cdx: Value,
    spdx2: Value,
    spdx3: Value,
}

fn scan(root: &Path, cache: &Path) -> Docs {
    let out = tempfile::tempdir().unwrap();
    let cdx = out.path().join("o.cdx.json");
    let spdx2 = out.path().join("o.spdx2.json");
    let spdx3 = out.path().join("o.spdx3.json");
    let result = Command::new(binary_path())
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
        .arg(format!("spdx-2.3-json={}", spdx2.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3.display()))
        .env("WAYBILL_NIXPKGS_CACHE", cache)
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let read = |p: &Path| -> Value {
        serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
    };
    Docs {
        cdx: read(&cdx),
        spdx2: read(&spdx2),
        spdx3: read(&spdx3),
    }
}

/// The CycloneDX `licenses[]` values for one component, as plain strings.
/// Both shapes CDX permits — `license.id` for a list identifier and
/// `license.name` / `expression` for anything else — are collected, because
/// which one a value takes is a property of the value, not of this milestone.
fn cdx_licenses(doc: &Value, name: &str) -> Vec<String> {
    let Some(c) = doc
        .get("components")
        .and_then(|v| v.as_array())
        .and_then(|a| {
            a.iter()
                .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
        })
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(arr) = c.get("licenses").and_then(|v| v.as_array()) {
        for entry in arr {
            for key in ["id", "name"] {
                if let Some(v) = entry.pointer(&format!("/license/{key}")).and_then(|v| v.as_str())
                {
                    out.push(v.to_string());
                }
            }
            if let Some(v) = entry.get("expression").and_then(|v| v.as_str()) {
                out.push(v.to_string());
            }
        }
    }
    out.sort();
    out
}

fn spdx2_declared(doc: &Value, name: &str) -> Option<String> {
    doc.get("packages")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))?
        .get("licenseDeclared")?
        .as_str()
        .map(str::to_string)
}

fn spdx3_license_texts(doc: &Value) -> Vec<String> {
    doc.get("@graph")
        .and_then(|v| v.as_array())
        .map(|g| {
            g.iter()
                .filter_map(|e| {
                    e.get("simplelicensing_licenseExpression")
                        .and_then(|v| v.as_str())
                })
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn version_of(doc: &Value, name: &str) -> Option<String> {
    doc.get("components")?
        .as_array()?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

// ───────────────────────────────────────────────────────────────────────
// T044 / C15 / SC-004 — the primary gain
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us3_resolved_component_carries_declared_license() {
    let cache = seed_cache();
    let d = scan(&fixture("license_project"), cache.path());

    // Precondition: these really were resolved. Without it the test could
    // pass by asserting licenses on components that are not there at all.
    assert_eq!(
        version_of(&d.cdx, "waybill-fixture-attr-bsd3").as_deref(),
        Some("1.0.0"),
        "the pass must have resolved this component, or the license \
         assertion below is vacuous"
    );

    assert_eq!(
        cdx_licenses(&d.cdx, "waybill-fixture-attr-bsd3"),
        vec!["BSD-3-Clause".to_string()],
        "a `lib.licenses.bsd3` declaration must reach the document as \
         BSD-3-Clause. Measured at 0.14.0: 0 of 66 components carried any \
         license."
    );
    assert_eq!(
        cdx_licenses(&d.cdx, "waybill-fixture-spdx-mit"),
        vec!["MIT".to_string()]
    );
    assert_eq!(
        cdx_licenses(&d.cdx, "waybill-fixture-compound"),
        vec!["(BSD-2-Clause OR Apache-2.0)".to_string()],
        "a compound expression survives as an expression"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T046 / C16 / FR-011 — uncanonicalisable, preserved and flagged
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us3_uncanonicalisable_license_is_preserved_and_flagged() {
    let cache = seed_cache();
    let d = scan(&fixture("license_project"), cache.path());

    // `"GPL"` is a real declaration with no version, 438 of 994 bare strings
    // at the measured revision. It is preserved rather than dropped, and the
    // `LicenseRef-` prefix is what records that it is NOT a canonical SPDX
    // identifier — SPDX's own mechanism for a license off its list, and the
    // one milestone 185 and milestone 202 already emit.
    let got = cdx_licenses(&d.cdx, "waybill-fixture-unversioned");
    assert_eq!(
        got,
        vec!["LicenseRef-GPL".to_string()],
        "the declaration must survive, marked as not-canonical"
    );

    // `publicDomain` has no clean SPDX identifier, so it takes the same path
    // rather than being mapped to something it is not.
    let pd = cdx_licenses(&d.cdx, "waybill-fixture-public-domain");
    assert_eq!(pd, vec!["LicenseRef-publicDomain".to_string()]);
}

// ───────────────────────────────────────────────────────────────────────
// T047 / C17 / FR-011a — "unknown" is not a license
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us3_unknown_yields_no_license() {
    let cache = seed_cache();
    let d = scan(&fixture("license_project"), cache.path());

    // Resolved, so the component is certainly present and the assertion is
    // about the license field rather than about the component's absence.
    assert_eq!(
        version_of(&d.cdx, "waybill-fixture-unknown-lic").as_deref(),
        Some("5.0.0")
    );
    // Publishing the string would be worse than an absent field: a consumer
    // filtering for components with NO license information would not find
    // it, so the document would answer "what is this licensed under" with a
    // value that is not a license. 360 of 994 bare strings at the measured
    // revision say `"unknown"`.
    assert!(
        cdx_licenses(&d.cdx, "waybill-fixture-unknown-lic").is_empty(),
        "a declaration that the license is unknown yields an ABSENT field, \
         not the string `unknown`; got {:?}",
        cdx_licenses(&d.cdx, "waybill-fixture-unknown-lic")
    );
}

// ───────────────────────────────────────────────────────────────────────
// T048 / C18 / FR-012 — no license is invented
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us3_unresolved_component_gets_no_license() {
    let cache = seed_cache();
    let d = scan(&fixture("license_project"), cache.path());

    // `waybill-fixture-absent` is declared by the project and is NOT in the
    // seeded package set, so the pass cannot resolve it. FR-012 holds
    // structurally here rather than by assertion: the attaching code runs
    // only inside `apply_resolved`, which an unresolved component never
    // reaches. The test is what keeps that structural property true.
    // Present, and unresolved. CycloneDX omits the `version` key entirely
    // rather than emitting an empty string, so "absent or empty" is the
    // shape — asserted explicitly, because `None` here would otherwise be
    // indistinguishable from the component not being emitted at all, and
    // this test would then pass by checking the licenses of nothing.
    let present = d
        .cdx
        .get("components")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter().any(|c| {
                c.get("name").and_then(|v| v.as_str()) == Some("waybill-fixture-absent")
            })
        })
        .unwrap_or(false);
    assert!(
        present,
        "the component is emitted versionless (FR-005a), not dropped"
    );
    assert!(
        version_of(&d.cdx, "waybill-fixture-absent")
            .map(|v| v.is_empty())
            .unwrap_or(true),
        "and it carries no version, so the pass did not resolve it"
    );
    assert!(
        cdx_licenses(&d.cdx, "waybill-fixture-absent").is_empty(),
        "and carries no license, because none was resolved for it"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T049 / FR-010 — the NATIVE field, in all three formats
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us3_license_is_emitted_in_all_three_formats() {
    let cache = seed_cache();
    let d = scan(&fixture("license_project"), cache.path());

    // Principle V's primary case: the license goes into each format's own
    // field, not into a `waybill:` annotation. CycloneDX `licenses[]`, SPDX
    // 2.3 `PackageLicenseDeclared`, SPDX 3
    // `simplelicensing_LicenseExpression`.
    assert_eq!(
        cdx_licenses(&d.cdx, "waybill-fixture-attr-bsd3"),
        vec!["BSD-3-Clause".to_string()],
        "CycloneDX native licenses[]"
    );
    let s2 = spdx2_declared(&d.spdx2, "waybill-fixture-attr-bsd3");
    assert_eq!(
        s2.as_deref(),
        Some("BSD-3-Clause"),
        "SPDX 2.3 native licenseDeclared"
    );
    let s3 = spdx3_license_texts(&d.spdx3);
    assert!(
        s3.iter().any(|t| t.contains("BSD-3-Clause")),
        "SPDX 3 native simplelicensing_licenseExpression; got {s3:?}"
    );

    // And no `waybill:` property carries the license itself.
    let cdx_text = d.cdx.to_string();
    assert!(
        !cdx_text.contains("waybill:nixpkgs-license")
            && !cdx_text.contains("waybill:declared-license"),
        "no waybill:* annotation may carry the license -- the native field does"
    );
}
