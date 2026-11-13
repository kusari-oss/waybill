//! #1046 — a Nix-built Haskell dependency whose overlay pins a version the
//! nixpkgs package set does not carry is one component, not two.
//!
//! | member | declared | pinned set | closure | expected |
//! |---|---|---|---|---|
//! | `overlaid` | `>= 2.0.0.0` | 1.0.0.0 | 2.1.0.0, GHC-built, git source | merged; Hackage decides the identity |
//! | `relaxed` | `>=1.0 && <1.5` | 1.6 | 1.6 (exact pair, `confirmed`) | merged; C221, the build is outside the range |
//! | `namesake` | (no range) | absent | 1.0, **not** GHC-built, tarball source | not merged: not a Haskell package |
//! | `variants` | `>= 3.0` | 2.0 | 3.1 and 3.2, both in range | not merged: two candidates |
//!
//! The closure is JSON handed to a stub `nix`; Hackage is a local stub or a
//! closed port, never the real service (Principle VII).

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Scan};
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_haskell_overlay")
}

fn read(name: &str) -> String {
    let p = dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn scan_with(extra: &[&str]) -> Scan {
    let closure = read("closure.json");
    let packages = read("hackage-packages.nix");
    let mut args = vec!["--nix-closure", "--no-deps-dev", "--no-clearly-defined"];
    args.extend_from_slice(extra);
    NixStub {
        project: dir().join("project"),
        closure: &closure,
        package_set: Some((&packages, "{ }\n")),
        ..Default::default()
    }
    .scan(&args)
}

fn components(cdx: &Value) -> Vec<&Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        if let Some(cs) = v["components"].as_array() {
            for c in cs {
                out.push(c);
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(cdx, &mut out);
    out
}

fn named<'a>(cdx: &'a Value, short: &str) -> Vec<&'a Value> {
    let name = format!("waybill-fixture-{short}");
    components(cdx).into_iter().filter(|c| c["name"] == name).collect()
}

fn prop(c: &Value, name: &str) -> Option<String> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

fn spdx23_count(doc: &Value, name: &str) -> usize {
    doc["packages"].as_array().into_iter().flatten().filter(|p| p["name"] == name).count()
}

fn spdx3_count(doc: &Value, name: &str) -> usize {
    doc["@graph"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["type"] == "software_Package" && e["name"] == name)
        .count()
}

/// The component count of `waybill-fixture-<short>` in each of the three
/// formats, which must agree.
fn count_everywhere(s: &Scan, short: &str) -> usize {
    let name = format!("waybill-fixture-{short}");
    let cdx = named(&s.cdx, short).len();
    assert_eq!(spdx23_count(&s.spdx23, &name), cdx, "{name}: SPDX 2.3 disagrees with CycloneDX");
    assert_eq!(spdx3_count(&s.spdx3, &name), cdx, "{name}: SPDX 3 disagrees with CycloneDX");
    cdx
}

// ---------------------------------------------------------------
// US1 — one component per dependency
// ---------------------------------------------------------------

#[test]
fn m1046_us1_a_declared_and_built_dependency_is_one_component() {
    let s = scan_with(&[]);
    for (short, range) in [("overlaid", ">= 2.0.0.0"), ("relaxed", ">=1.0 && <1.5")] {
        assert_eq!(count_everywhere(&s, short), 1, "{short} must be one component");
        let c = named(&s.cdx, short)[0];
        assert_eq!(prop(c, "waybill:sbom-tier").as_deref(), Some("build"), "{short}: the build survives");
        assert_eq!(
            prop(c, "waybill:requirement-ranges").as_deref(),
            Some(serde_json::json!([range]).to_string().as_str()),
            "{short}: the declared range moves onto the survivor"
        );
        assert_eq!(prop(c, "waybill:haskell-version-unresolved-reason"), None, "{short}: no longer unresolved");
    }
}

/// FR-003, FR-004: a namesake no GHC built, and two in-range variants, stay
/// unmerged.
#[test]
fn m1046_us1_a_namesake_and_ambiguous_variants_stay_unmerged() {
    let s = scan_with(&[]);
    assert_eq!(count_everywhere(&s, "namesake"), 2, "not a Haskell package: design and build both stay");
    assert_eq!(count_everywhere(&s, "variants"), 3, "two variants in range: nothing merges");
}

#[test]
fn m1046_us1_no_edge_names_a_removed_twin() {
    let s = scan_with(&[]);
    // CONTROL: there are references to check.
    assert!(common::endpoints::reference_count(&s.cdx, &s.spdx23, &s.spdx3) > 0);
    let dangling = common::endpoints::dangling(&s.cdx, &s.spdx23, &s.spdx3);
    assert!(dangling.is_empty(), "{dangling:#?}");
}

/// CONTROL: without `--nix-closure` nothing is merged; the versionless
/// design-tier component is still there.
#[test]
fn m1046_us1_without_the_closure_the_design_twin_remains() {
    let closure = read("closure.json");
    let packages = read("hackage-packages.nix");
    let s = NixStub {
        project: dir().join("project"),
        closure: &closure,
        package_set: Some((&packages, "{ }\n")),
        ..Default::default()
    }
    .scan(&["--no-deps-dev", "--no-clearly-defined"]);
    let o = named(&s.cdx, "overlaid");
    assert_eq!(o.len(), 1);
    assert_eq!(o[0]["purl"], "pkg:hackage/waybill-fixture-overlaid");
    assert_eq!(prop(o[0], "waybill:sbom-tier").as_deref(), Some("design"));
}

// ---------------------------------------------------------------
// US3 — the VCS source is in the document
// ---------------------------------------------------------------

const VCS: &str = "git+https://example.invalid/waybill-fixture-overlaid.git@1111111111111111111111111111111111111111";

fn cdx_refs(c: &Value) -> Vec<(String, String)> {
    c["externalReferences"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| (r["type"].as_str().unwrap_or("").to_string(), r["url"].as_str().unwrap_or("").to_string()))
        .collect()
}

fn by_name<'a>(cdx: &'a Value, name: &str) -> &'a Value {
    let found: Vec<&Value> = components(cdx).into_iter().filter(|c| c["name"] == name).collect();
    assert_eq!(found.len(), 1, "one component named {name}");
    found[0]
}

/// The git checkout (`url`/`rev` under `structuredAttrs`), and the package
/// whose `src` it is, both carry the `vcs` reference, in all three formats.
#[test]
fn m1046_us3_a_git_checkout_and_its_package_carry_a_vcs_reference() {
    let s = scan_with(&[]);
    let member = by_name(&s.cdx, "waybill-fixture-overlaid-1111111");
    assert_eq!(cdx_refs(member), vec![("vcs".to_string(), VCS.to_string())], "a checkout is not a download");
    let pkg = named(&s.cdx, "overlaid")[0];
    assert_eq!(cdx_refs(pkg), vec![("vcs".to_string(), VCS.to_string())]);

    // SPDX 2.3: externalRefs OTHER/vcs (A10).
    let p23 = s.spdx23["packages"].as_array().unwrap().iter()
        .find(|p| p["name"] == "waybill-fixture-overlaid").unwrap();
    assert!(
        p23["externalRefs"].as_array().into_iter().flatten().any(|r| {
            r["referenceCategory"] == "OTHER" && r["referenceType"] == "vcs" && r["referenceLocator"] == VCS
        }),
        "SPDX 2.3 carries the vcs reference: {}", p23["externalRefs"]
    );
    // SPDX 3: software_sourceInfo (A10).
    let p3 = s.spdx3["@graph"].as_array().unwrap().iter()
        .find(|e| e["type"] == "software_Package" && e["name"] == "waybill-fixture-overlaid").unwrap();
    assert_eq!(p3["software_sourceInfo"], VCS);
}

/// CONTROL: a tarball fetch (`urls` in `env`, no `rev`) keeps its download
/// reference and gains no `vcs`; a package built from it carries none.
#[test]
fn m1046_us3_a_tarball_source_keeps_its_distribution_reference() {
    let s = scan_with(&[]);
    let member = by_name(&s.cdx, "waybill-fixture-namesake-1.0.tar.gz");
    assert_eq!(
        cdx_refs(member),
        vec![("distribution".to_string(), "https://example.invalid/waybill-fixture-namesake-1.0.tar.gz".to_string())]
    );
    for c in named(&s.cdx, "namesake") {
        assert!(cdx_refs(c).iter().all(|(t, _)| t != "vcs"), "{c}");
    }
}
