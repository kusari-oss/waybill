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

// ---------------------------------------------------------------
// US2 — a VCS-built released version is reachable by advisories
// ---------------------------------------------------------------

const OVERLAID_PATH: &str = "/package/waybill-fixture-overlaid-2.1.0.0/waybill-fixture-overlaid.cabal";
const PUBLISHED: &str = "pkg:hackage/waybill-fixture-overlaid@2.1.0.0?vcs_url=git+https://example.invalid/waybill-fixture-overlaid.git%401111111111111111111111111111111111111111";

/// A Hackage stub answering `status` for the overlaid version and 404 for
/// everything else.
async fn hackage(status: u16) -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(OVERLAID_PATH))
        .respond_with(ResponseTemplate::new(status))
        .mount(&s)
        .await;
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&s).await;
    s
}

fn scan_against(server: &wiremock::MockServer, extra: &[&str]) -> Scan {
    let closure = read("closure.json");
    let packages = read("hackage-packages.nix");
    let mut args = vec!["--nix-closure", "--no-deps-dev", "--no-clearly-defined"];
    args.extend_from_slice(extra);
    NixStub {
        project: dir().join("project"),
        closure: &closure,
        package_set: Some((&packages, "{ }\n")),
        hackage_url: Some(server.uri()),
        ..Default::default()
    }
    .scan(&args)
}

fn requested_paths(reqs: &[wiremock::Request]) -> Vec<String> {
    let mut v: Vec<String> = reqs.iter().map(|r| r.url.path().to_string()).collect();
    v.sort();
    v
}

/// 200: the overlaid version is published, so the merged component is
/// `pkg:hackage`, with the checkout it was built from as `vcs_url`, in all
/// three formats.
#[tokio::test]
async fn m1046_us2_a_published_overlay_version_is_a_hackage_package() {
    let server = hackage(200).await;
    let s = scan_against(&server, &[]);
    let o = named(&s.cdx, "overlaid");
    assert_eq!(o.len(), 1, "merged");
    assert_eq!(o[0]["purl"], PUBLISHED);
    assert_eq!(prop(o[0], "waybill:closure-identity").as_deref(), Some("hackage-published"));
    let p23 = s.spdx23["packages"].as_array().unwrap().iter()
        .find(|p| p["name"] == "waybill-fixture-overlaid").unwrap();
    assert!(
        p23["externalRefs"].as_array().into_iter().flatten()
            .any(|r| r["referenceType"] == "purl" && r["referenceLocator"] == PUBLISHED),
        "SPDX 2.3 carries the identity: {}", p23["externalRefs"]
    );
    assert!(s.spdx3.to_string().contains(PUBLISHED), "SPDX 3 carries the identity");
    // No edge names the old generic PURL.
    assert!(common::endpoints::dangling(&s.cdx, &s.spdx23, &s.spdx3).is_empty());
}

/// 404: not published (a fork's own version number) — `pkg:generic`.
#[tokio::test]
async fn m1046_us2_an_unpublished_version_stays_generic() {
    let server = hackage(404).await;
    let s = scan_against(&server, &[]);
    let o = named(&s.cdx, "overlaid");
    assert_eq!(o[0]["purl"], "pkg:generic/waybill-fixture-overlaid@2.1.0.0");
    assert_eq!(prop(o[0], "waybill:closure-identity").as_deref(), Some("hackage-unpublished"));
}

/// Only GHC-built members whose name the pinned set carries are asked about:
/// `overlaid` and the two `variants`, never `namesake` (not in the set, and
/// not GHC-built) and never `relaxed` (`confirmed` from the set).
#[tokio::test]
async fn m1046_us2_only_ghc_built_members_the_set_carries_are_asked_about() {
    let server = hackage(200).await;
    let _ = scan_against(&server, &[]);
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(
        requested_paths(&reqs),
        vec![
            OVERLAID_PATH.to_string(),
            "/package/waybill-fixture-variants-3.1/waybill-fixture-variants.cabal".to_string(),
            "/package/waybill-fixture-variants-3.2/waybill-fixture-variants.cabal".to_string(),
        ]
    );
}

/// `--no-hackage-lookup`: nothing is asked, the component stays generic and
/// says why, and the merge still happens.
#[tokio::test]
async fn m1046_us2_the_opt_out_asks_nothing() {
    let server = hackage(200).await;
    let s = scan_against(&server, &["--no-hackage-lookup"]);
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
    let o = named(&s.cdx, "overlaid");
    assert_eq!(o.len(), 1, "the merge does not depend on the lookup");
    assert_eq!(o[0]["purl"], "pkg:generic/waybill-fixture-overlaid@2.1.0.0");
    assert_eq!(prop(o[0], "waybill:closure-identity").as_deref(), Some("hackage-unchecked-opted-out"));
}

/// The default `NixStub` points at a closed port: unreachable, generic.
#[test]
fn m1046_us2_an_unreachable_hackage_stays_generic() {
    let s = scan_with(&[]);
    let o = named(&s.cdx, "overlaid");
    assert_eq!(o[0]["purl"], "pkg:generic/waybill-fixture-overlaid@2.1.0.0");
    assert_eq!(prop(o[0], "waybill:closure-identity").as_deref(), Some("hackage-unreachable"));
}

// ---------------------------------------------------------------
// US4 — a build outside the declared range
// ---------------------------------------------------------------

/// `relaxed` declares `>=1.0 && <1.5` and was built at 1.6: one component,
/// recording C221 identically in all three formats; `overlaid`, in range,
/// records none.
#[test]
fn m1046_us4_an_out_of_range_build_records_c221_in_every_format() {
    let s = scan_with(&[]);
    let want = r#"{"requirement-ranges":[">=1.0 && <1.5"],"version":"1.6"}"#;
    let r = named(&s.cdx, "relaxed");
    assert_eq!(r.len(), 1);
    assert_eq!(prop(r[0], "waybill:requirement-unsatisfied").as_deref(), Some(want));
    assert_eq!(prop(named(&s.cdx, "overlaid")[0], "waybill:requirement-unsatisfied"), None, "in range: no C221");

    let e = waybill::parity::extractors::EXTRACTORS.iter().find(|e| e.row_id == "C221").expect("C221 registered");
    let (a, b, c) = ((e.cdx)(&s.cdx), (e.spdx23)(&s.spdx23), (e.spdx3)(&s.spdx3));
    assert!(!a.is_empty(), "C221: CycloneDX extractor found nothing");
    assert_eq!(a, b, "C221: CycloneDX and SPDX 2.3 disagree");
    assert_eq!(a, c, "C221: CycloneDX and SPDX 3 disagree");
}
